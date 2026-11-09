#!/usr/bin/env bash
# End-to-end test against real kernel WireGuard (Linux, needs sudo).
#
# Creates a tunnel between wg-a (root netns) and wg-b (netns "wge2e"),
# triggers a handshake with ping, runs the exporter and checks the exposed
# metrics. The exporter runs as
#   - a binary: unprivileged user with CAP_NET_ADMIN (file capability);
#   - a container image with the documented minimal privileges (host
#     network, only NET_ADMIN, no-new-privileges, read-only);
#   - the systemd unit from contrib/ (installs /usr/local/bin/wireguard_exporter,
#     /etc/default/wireguard_exporter and the unit; disposable machines only).
#
# Usage: scripts/e2e.sh <binary>
#        scripts/e2e.sh --image <image>      (runtime: $E2E_RUNTIME, default docker)
#        scripts/e2e.sh --systemd <binary>
set -euo pipefail

USAGE="usage: $0 <binary> | --image <image> | --systemd <binary>"
BIN=""
IMAGE=""
MODE=binary
case "${1:-}" in
    --image) MODE=image; IMAGE="${2:?$USAGE}" ;;
    --systemd) MODE=systemd; BIN="$(realpath "${2:?$USAGE}")" ;;
    *) BIN="$(realpath "${1:?$USAGE}")" ;;
esac
UNIT_SRC="$(dirname "$(realpath "$0")")/../contrib/wireguard_exporter.service"
RUNTIME="${E2E_RUNTIME:-docker}"
CONTAINER=wge2e-exporter
PORT=19586
NS=wge2e
WORK="$(mktemp -d)"
# The wg AppArmor profile (Ubuntu 26.04+) only lets wg read files under
# /etc/wireguard, so the private keys must live there.
sudo mkdir -p /etc/wireguard
KEYDIR="$(sudo mktemp -d -p /etc/wireguard e2e.XXXXXX)"
umask 077
EXPORTER_PID=""

cleanup() {
    [[ -n "$EXPORTER_PID" ]] && kill "$EXPORTER_PID" 2>/dev/null || true
    [[ -n "$IMAGE" ]] && "$RUNTIME" rm -f "$CONTAINER" >/dev/null 2>&1 || true
    if [[ "$MODE" == systemd ]]; then
        sudo systemctl disable --now wireguard_exporter >/dev/null 2>&1 || true
        sudo rm -f /etc/systemd/system/wireguard_exporter.service /etc/default/wireguard_exporter \
            /usr/local/bin/wireguard_exporter
        sudo rm -rf /etc/wireguard_exporter
        sudo systemctl daemon-reload || true
    fi
    sudo ip link del wg-a 2>/dev/null || true
    sudo ip netns del "$NS" 2>/dev/null || true
    rm -rf "$WORK"
    sudo rm -rf "$KEYDIR"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*" >&2
    echo "---- metrics ----" >&2
    cat "$WORK/metrics" >&2 2>/dev/null || true
    if [[ -n "$IMAGE" ]]; then
        echo "---- container logs ----" >&2
        "$RUNTIME" logs "$CONTAINER" >&2 2>&1 || true
    fi
    if [[ "$MODE" == systemd ]]; then
        echo "---- systemd ----" >&2
        sudo systemctl status wireguard_exporter --no-pager >&2 2>&1 || true
        sudo journalctl -u wireguard_exporter --no-pager -n 50 >&2 2>&1 || true
    fi
    exit 1
}

A_KEY="$(wg genkey)"; A_PUB="$(echo "$A_KEY" | wg pubkey)"
B_KEY="$(wg genkey)"; B_PUB="$(echo "$B_KEY" | wg pubkey)"
echo "$A_KEY" | sudo tee "$KEYDIR/a.key" >/dev/null
echo "$B_KEY" | sudo tee "$KEYDIR/b.key" >/dev/null
wg genpsk | sudo tee "$KEYDIR/psk" >/dev/null

# Peer names; world-readable: the container's root has no DAC_OVERRIDE and
# the systemd dynamic user owns nothing. Written with sudo tee, which also
# rewrites in place (a single-file bind mount does not follow a new inode).
if [[ "$MODE" == systemd ]]; then
    sudo mkdir -p /etc/wireguard_exporter
    NAMES=/etc/wireguard_exporter/peers.yml
else
    NAMES="$WORK/peers.yml"
fi
write_names() { printf "$@" | sudo tee "$NAMES" >/dev/null; sudo chmod 0644 "$NAMES"; }
write_names '"%s":\n  name: peer-b\n  site: e2e\n' "$B_PUB"

# wg-b is created in the root netns and then moved: its UDP socket stays in
# the root netns, so both ends can talk over 127.0.0.1.
sudo ip netns add "$NS"
sudo ip link add wg-a type wireguard
sudo ip link add wg-b type wireguard
sudo ip link set wg-b netns "$NS"

sudo wg set wg-a listen-port 51820 private-key "$KEYDIR/a.key" \
    peer "$B_PUB" allowed-ips 10.99.0.2/32 endpoint 127.0.0.1:51821 \
    preshared-key "$KEYDIR/psk" persistent-keepalive 25
sudo ip addr add 10.99.0.1/24 dev wg-a
sudo ip link set wg-a up

sudo ip netns exec "$NS" wg set wg-b listen-port 51821 private-key "$KEYDIR/b.key" \
    peer "$A_PUB" allowed-ips 10.99.0.1/32 endpoint 127.0.0.1:51820 \
    preshared-key "$KEYDIR/psk"
sudo ip netns exec "$NS" ip addr add 10.99.0.2/24 dev wg-b
sudo ip netns exec "$NS" ip link set wg-b up

sudo ip netns exec "$NS" ping -c 3 -W 2 10.99.0.1 >/dev/null || fail "tunnel ping failed"

if [[ "$MODE" == systemd ]]; then
    sudo install -m 0755 "$BIN" /usr/local/bin/wireguard_exporter
    sudo install -m 0644 "$UNIT_SRC" /etc/systemd/system/wireguard_exporter.service
    echo "WG_EXPORTER_OPTS=\"--web.listen-address 127.0.0.1:$PORT --wireguard.peer-names-file $NAMES\"" \
        | sudo tee /etc/default/wireguard_exporter >/dev/null
    sudo systemctl daemon-reload
    sudo systemctl start wireguard_exporter
    systemd-analyze security wireguard_exporter.service --no-pager | tail -1 || true
elif [[ -n "$IMAGE" ]]; then
    # Minimal privileges documented in docs/installation.md ("Containers").
    "$RUNTIME" run -d --name "$CONTAINER" \
        --network host \
        --cap-drop ALL --cap-add NET_ADMIN \
        --security-opt no-new-privileges \
        --read-only \
        -v "$NAMES:/etc/wireguard_exporter/peers.yml:ro" \
        "$IMAGE" --web.listen-address "127.0.0.1:$PORT" \
        --wireguard.peer-names-file /etc/wireguard_exporter/peers.yml >/dev/null
else
    # Run as the current (unprivileged) user with only CAP_NET_ADMIN.
    cp "$BIN" "$WORK/wireguard_exporter"
    sudo setcap cap_net_admin+ep "$WORK/wireguard_exporter"
    "$WORK/wireguard_exporter" --web.listen-address "127.0.0.1:$PORT" \
        --wireguard.peer-names-file "$NAMES" &
    EXPORTER_PID=$!
fi

for _ in $(seq 1 50); do
    curl -fsS "http://127.0.0.1:$PORT/metrics" -o "$WORK/metrics" 2>/dev/null && break
    sleep 0.1
done
[[ -s "$WORK/metrics" ]] || fail "exporter did not respond"

M="$WORK/metrics"
value() { # value <metric-with-labels>
    { grep -F "$1 " "$M" || true; } | awk '{print $NF}' | head -1
}

grep -qF 'wireguard_scrape_success{interface="wg-a"} 1' "$M" || fail "scrape_success"
grep -qF 'wireguard_device_info{interface="wg-a",listen_port="51820"} 1' "$M" || fail "device_info"
grep -qF 'wireguard_device_peers{interface="wg-a"} 1' "$M" || fail "device_peers"
grep -qF "wireguard_peer_info{interface=\"wg-a\",public_key=\"$B_PUB\",endpoint=\"127.0.0.1:51821\",allowed_ips=\"10.99.0.2/32\",preshared_key=\"true\"} 1" "$M" \
    || fail "peer_info"
grep -qF "wireguard_peer_allowed_ip{interface=\"wg-a\",public_key=\"$B_PUB\",cidr=\"10.99.0.2/32\"} 1" "$M" \
    || fail "peer_allowed_ip"
grep -qF "wireguard_peer_persistent_keepalive_seconds{interface=\"wg-a\",public_key=\"$B_PUB\"} 25" "$M" \
    || fail "persistent_keepalive"
grep -qF "wireguard_peer_name{interface=\"wg-a\",public_key=\"$B_PUB\",name=\"peer-b\",site=\"e2e\"} 1" "$M" \
    || fail "peer_name"
grep -qF 'wireguard_exporter_peer_names_load_success 1' "$M" || fail "peer_names_load_success"
grep -qE '^wireguard_exporter_build_info\{version="[0-9.]+"\} 1$' "$M" || fail "build_info"
grep -qE '^wireguard_scrape_duration_seconds\{interface="wg-a"\} [0-9.e-]+$' "$M" || fail "scrape_duration"

LABELS="{interface=\"wg-a\",public_key=\"$B_PUB\"}"
HS="$(value "wireguard_peer_last_handshake_timestamp_seconds$LABELS")"
RX="$(value "wireguard_peer_receive_bytes_total$LABELS")"
TX="$(value "wireguard_peer_transmit_bytes_total$LABELS")"
NOW="$(date +%s)"

awk -v hs="$HS" -v now="$NOW" 'BEGIN { exit !(hs > now - 120 && hs <= now + 5) }' \
    || fail "handshake timestamp $HS not recent (now $NOW)"
[[ "${RX:-0}" -gt 0 ]] || fail "rx bytes = ${RX:-none}"
[[ "${TX:-0}" -gt 0 ]] || fail "tx bytes = ${TX:-none}"

# wg-b lives in another netns and must not be visible.
grep -q 'interface="wg-b"' "$M" && fail "wg-b from another netns is visible"

# Names are reloaded on change. Rewrite in place: a single-file bind mount
# does not follow a replaced inode.
write_names '"%s": peer-b-renamed\n' "$B_PUB"
curl -fsS "http://127.0.0.1:$PORT/metrics" -o "$M"
grep -qF "wireguard_peer_name{interface=\"wg-a\",public_key=\"$B_PUB\",name=\"peer-b-renamed\"} 1" "$M" \
    || fail "peer names were not reloaded"

echo "e2e OK ($MODE): handshake=$HS rx=$RX tx=$TX"

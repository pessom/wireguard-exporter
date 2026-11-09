#!/usr/bin/env bash
# End-to-end test against real kernel WireGuard (Linux, needs sudo).
#
# Creates a tunnel between wg-a (root netns) and wg-b (netns "wge2e"),
# triggers a handshake with ping, runs the exporter as an unprivileged user
# with CAP_NET_ADMIN and checks the exposed metrics.
#
# Usage: scripts/e2e.sh <path-to-wireguard_exporter-binary>
set -euo pipefail

BIN="$(realpath "${1:?usage: $0 <binary>}")"
PORT=19586
NS=wge2e
WORK="$(mktemp -d)"
umask 077
EXPORTER_PID=""

cleanup() {
    [[ -n "$EXPORTER_PID" ]] && kill "$EXPORTER_PID" 2>/dev/null || true
    sudo ip link del wg-a 2>/dev/null || true
    sudo ip netns del "$NS" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*" >&2
    echo "---- metrics ----" >&2
    cat "$WORK/metrics" >&2 2>/dev/null || true
    exit 1
}

wg genkey > "$WORK/a.key"; wg pubkey < "$WORK/a.key" > "$WORK/a.pub"
wg genkey > "$WORK/b.key"; wg pubkey < "$WORK/b.key" > "$WORK/b.pub"
A_PUB="$(cat "$WORK/a.pub")"
B_PUB="$(cat "$WORK/b.pub")"

# wg-b is created in the root netns and then moved: its UDP socket stays in
# the root netns, so both ends can talk over 127.0.0.1.
sudo ip netns add "$NS"
sudo ip link add wg-a type wireguard
sudo ip link add wg-b type wireguard
sudo ip link set wg-b netns "$NS"

sudo wg set wg-a listen-port 51820 private-key /dev/stdin < "$WORK/a.key" \
    peer "$B_PUB" allowed-ips 10.99.0.2/32 endpoint 127.0.0.1:51821
sudo ip addr add 10.99.0.1/24 dev wg-a
sudo ip link set wg-a up

sudo ip netns exec "$NS" wg set wg-b listen-port 51821 private-key /dev/stdin < "$WORK/b.key" \
    peer "$A_PUB" allowed-ips 10.99.0.1/32 endpoint 127.0.0.1:51820
sudo ip netns exec "$NS" ip addr add 10.99.0.2/24 dev wg-b
sudo ip netns exec "$NS" ip link set wg-b up

sudo ip netns exec "$NS" ping -c 3 -W 2 10.99.0.1 >/dev/null || fail "tunnel ping failed"

# Run as the current (unprivileged) user with only CAP_NET_ADMIN.
cp "$BIN" "$WORK/wireguard_exporter"
sudo setcap cap_net_admin+ep "$WORK/wireguard_exporter"
"$WORK/wireguard_exporter" --web.listen-address "127.0.0.1:$PORT" &
EXPORTER_PID=$!

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
grep -qF "wireguard_peer_info{interface=\"wg-a\",public_key=\"$B_PUB\",endpoint=\"127.0.0.1:51821\",allowed_ips=\"10.99.0.2/32\"} 1" "$M" \
    || fail "peer_info"

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

echo "e2e OK: handshake=$HS rx=$RX tx=$TX"

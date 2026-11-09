# Troubleshooting

## `wireguard_scrape_success 0`, `Operation not permitted`

The exporter lacks `CAP_NET_ADMIN` in the interface's network namespace.

- systemd: the unit must have `AmbientCapabilities=CAP_NET_ADMIN` and
  `CapabilityBoundingSet=CAP_NET_ADMIN`.
- Plain binary: `sudo setcap cap_net_admin+ep /usr/local/bin/wireguard_exporter`.
  File capabilities are lost when the binary is replaced, and ignored on
  filesystems mounted `nosuid` and under `no-new-privileges`.
- Containers: `--cap-add NET_ADMIN` (Kubernetes:
  `securityContext.capabilities.add: [NET_ADMIN]`). Run as UID 0 — a
  non-root user does not receive added capabilities.
- Rootless Docker/nerdctl/Podman cannot read the host's tunnels at all, see
  [Rootless runtimes](installation.md#rootless-runtimes).

## No `wireguard_*` interface metrics at all

The exporter sees only interfaces in its own network namespace.

- Container without `--network host` (Kubernetes: `hostNetwork: true`) sees
  only its own empty namespace.
- WireGuard in another container or pod: join that network namespace
  (`--network container:<name>`, or a sidecar).
- Check what the exporter would discover:
  `grep -l DEVTYPE=wireguard /sys/class/net/*/uevent` in the same namespace.
- With `--wireguard.interfaces`, only the listed interfaces are exported.

## Pod is rejected: `violates PodSecurity`

`hostNetwork` and `NET_ADMIN` need the `privileged` Pod Security Standard on
the namespace:
`kubectl label namespace <ns> pod-security.kubernetes.io/enforce=privileged`.

## `address already in use`

With the host network, port 9586 must be free on the node. Change it with
`--web.listen-address` (Helm: `port`).

## Handshake timestamp is old although the peer is fine

Expected for idle peers without `PersistentKeepalive`: WireGuard handshakes
only when there is traffic. See [Metrics → Semantics](metrics.md#semantics).

## `many-to-many matching not allowed` in peer-name queries

The same public key has two different names in `wireguard_peer_name`. See
[Peer names](prometheus.md#peer-names).

## Debug logging

`RUST_LOG=debug wireguard_exporter` (container: `-e RUST_LOG=debug`).

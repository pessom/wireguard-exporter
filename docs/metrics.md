# Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `wireguard_scrape_success` | gauge | `interface` | 1 if the interface was read, 0 on error |
| `wireguard_device_info` | gauge, always 1 | `interface`, `listen_port` | Interface information |
| `wireguard_device_peers` | gauge | `interface` | Number of configured peers |
| `wireguard_peer_info` | gauge, always 1 | `interface`, `public_key`, `endpoint`, `allowed_ips` | Peer information |
| `wireguard_peer_last_handshake_timestamp_seconds` | gauge | `interface`, `public_key` | Unix time of the last handshake, `0` = never |
| `wireguard_peer_receive_bytes_total` | counter | `interface`, `public_key` | Bytes received from the peer |
| `wireguard_peer_transmit_bytes_total` | counter | `interface`, `public_key` | Bytes sent to the peer |

Prometheus adds `job` and `instance` to every series.

## Label details

- `public_key`: base64 public key of the peer, e.g.
  `xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=`. To show human-readable
  names, see [Peer names](prometheus.md#peer-names).
- `endpoint`: last known `IP:port` of the peer, empty if unknown. It changes
  when a client roams, so it exists **only** in `wireguard_peer_info` and does
  not split the traffic and handshake series. Join it when needed:

  ```promql
  rate(wireguard_peer_receive_bytes_total[5m])
    * on (instance, interface, public_key) group_left (endpoint) wireguard_peer_info
  ```

- `allowed_ips`: comma-separated list of CIDRs, e.g. `10.0.0.2/32,fd00::2/128`.

## Semantics

- **Counters** are the kernel's per-peer byte counters. They reset when the
  interface or peer is recreated (`wg-quick down/up`, config reload that
  removes the peer); `rate()`/`increase()` handle resets.
- **Handshake**: while traffic flows (or with `PersistentKeepalive`),
  WireGuard renews the session about every 2 minutes. An idle peer without
  keepalive does not handshake at all, so an old timestamp is not necessarily
  a failure. A handshake older than ~3 minutes means the peer is not actively
  connected.
- Removed peers and interfaces disappear from the output on the next scrape
  (no stale series are kept).

## Cardinality

Per interface: 3 series + 4 per peer. 1000 peers ≈ 4000 series per instance.

Metric and label names are a public contract and do not change in minor or
patch releases.

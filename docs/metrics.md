# Metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `wireguard_scrape_success` | gauge | `interface` | 1 if the interface was read, 0 on error |
| `wireguard_scrape_duration_seconds` | gauge | `interface` | Time spent reading the interface |
| `wireguard_device_info` | gauge, always 1 | `interface`, `listen_port` | Interface information |
| `wireguard_device_peers` | gauge | `interface` | Number of configured peers |
| `wireguard_peer_info` | gauge, always 1 | `interface`, `public_key`, `endpoint`, `allowed_ips`, `preshared_key` | Peer information |
| `wireguard_peer_name` | gauge, always 1 | `interface`, `public_key`, `name`, *extra labels* | Friendly name from the [peer names file](configuration.md#peer-names-file); only for peers listed there |
| `wireguard_peer_allowed_ip` | gauge, always 1 | `interface`, `public_key`, `cidr` | One series per allowed IP range |
| `wireguard_peer_persistent_keepalive_seconds` | gauge | `interface`, `public_key` | Persistent keepalive interval, `0` = disabled |
| `wireguard_peer_last_handshake_timestamp_seconds` | gauge | `interface`, `public_key` | Unix time of the last handshake, `0` = never |
| `wireguard_peer_receive_bytes_total` | counter | `interface`, `public_key` | Bytes received from the peer |
| `wireguard_peer_transmit_bytes_total` | counter | `interface`, `public_key` | Bytes sent to the peer |
| `wireguard_exporter_build_info` | gauge, always 1 | `version` | Exporter version |
| `wireguard_exporter_peer_names_load_success` | gauge | — | 1 if the last (re)load of the peer names file succeeded; only with `--wireguard.peer-names-file` |

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
  `wireguard_peer_allowed_ip` has the same data as one series per CIDR, handy
  to find the peer that owns an address:
  `wireguard_peer_allowed_ip{cidr="10.0.0.2/32"}`.
- `preshared_key`: `"true"` if the peer uses a preshared key (the key itself
  is never exported).
- `endpoint` and `allowed_ips` can be disabled with
  `--no-collector.peer-endpoint` / `--no-collector.peer-allowed-ips`
  ([privacy](configuration.md#privacy)); the labels are then empty.

## Semantics

- **Counters** are the kernel's per-peer byte counters. They reset when the
  interface or peer is recreated (`wg-quick down/up`, config reload that
  removes the peer); `rate()`/`increase()` handle resets.
- **Handshake**: while traffic flows (or with `PersistentKeepalive`),
  WireGuard renews the session about every 2 minutes. An idle peer without
  keepalive does not handshake at all, so an old timestamp is not necessarily
  a failure. A handshake older than ~3 minutes means the peer is not actively
  connected; for peers with
  `wireguard_peer_persistent_keepalive_seconds > 0` it is a problem.
- Removed peers and interfaces disappear from the output on the next scrape
  (no stale series are kept).

## Cardinality

Per interface: 4 series, plus per peer 5 + one per allowed IP range
(+1 with a name). 1000 peers with one range each ≈ 6000–7000 series per
instance. `--no-collector.peer-allowed-ips` removes the per-CIDR series.

Metric and label names are a public contract and do not change in minor or
patch releases.

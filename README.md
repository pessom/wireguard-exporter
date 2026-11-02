# wireguard-exporter

Prometheus exporter for Linux kernel WireGuard interfaces. Reads data via
netlink (no `wg` binary needed), ships as a single static binary.

## Metrics

| Metric | Type | Labels |
|---|---|---|
| `wireguard_peer_last_handshake_timestamp_seconds` | gauge (unix time, 0 = never) | `interface`, `public_key` |
| `wireguard_peer_receive_bytes_total` | counter | `interface`, `public_key` |
| `wireguard_peer_transmit_bytes_total` | counter | `interface`, `public_key` |
| `wireguard_peer_info` | gauge, always 1 | `interface`, `public_key`, `endpoint`, `allowed_ips` |
| `wireguard_device_info` | gauge, always 1 | `interface`, `listen_port` |
| `wireguard_device_peers` | gauge | `interface` |
| `wireguard_scrape_success` | gauge (1/0) | `interface` |

`endpoint` is exposed only in `wireguard_peer_info`, so roaming clients do not
create new time series. Join it when needed:

```promql
wireguard_peer_receive_bytes_total * on (interface, public_key) group_left(endpoint) wireguard_peer_info
```

Friendly peer names are intentionally not part of the exporter; add them with
`metric_relabel_configs` in Prometheus (map `public_key` to a `peer` label).

## Usage

```
wireguard_exporter [--web.listen-address 0.0.0.0:9586] [--web.telemetry-path /metrics]
                   [--wireguard.interfaces wg0,wg1]
```

Without `--wireguard.interfaces` all WireGuard interfaces are discovered on
each scrape (`/sys/class/net/*/uevent` with `DEVTYPE=wireguard`).
Every flag has an env variable: `WG_EXPORTER_LISTEN`, `WG_EXPORTER_PATH`,
`WG_EXPORTER_INTERFACES`. Log level: `RUST_LOG` (default `info`).

## Privileges

Reading WireGuard state requires `CAP_NET_ADMIN`. Run as an unprivileged user
with an ambient capability, see `contrib/wireguard_exporter.service`.

## Install

Download `wireguard_exporter-<version>.linux-amd64.tar.gz` from
[Releases](https://github.com/pessom/wireguard-exporter/releases) and verify it
with `sha256sums.txt`.

## Development

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Releases are built by GitHub Actions on `v*` tags (tag must match the version
in `Cargo.toml`). See [CONTRIBUTING.md](CONTRIBUTING.md) for details, including
the end-to-end test against real WireGuard.

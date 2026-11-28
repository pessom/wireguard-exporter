# wireguard-exporter

[![CI](https://github.com/pessom/wireguard-exporter/actions/workflows/ci.yml/badge.svg)](https://github.com/pessom/wireguard-exporter/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/pessom/wireguard-exporter)](https://github.com/pessom/wireguard-exporter/releases/latest)
[![License: MIT](https://img.shields.io/github/license/pessom/wireguard-exporter)](LICENSE)
[![Artifact Hub](https://img.shields.io/endpoint?url=https://artifacthub.io/badge/repository/wireguard-exporter)](https://artifacthub.io/packages/search?repo=wireguard-exporter)

Prometheus exporter for Linux kernel WireGuard interfaces. Reads state via
netlink (no `wg` binary needed) and ships as a single static binary and a
`FROM scratch` container image for `amd64` and `arm64`.

- Per-peer traffic, last handshake, keepalive, endpoint, allowed IPs and
  PSK usage; per-interface status and peer count — see
  [metrics](docs/metrics.md).
- **Friendly peer names** from a simple YAML file, reloaded on change — see
  [peer names](docs/prometheus.md#peer-names).
- Privacy switches to hide peer endpoints and allowed IPs.
- Needs only `CAP_NET_ADMIN`: systemd unit, container and Kubernetes
  manifests run it with that single capability.
- Interfaces are discovered automatically on every scrape.
- Ready-made Helm chart, kustomize manifests, recording and alerting rules
  and a [Grafana dashboard](https://grafana.com/grafana/dashboards/26131-wireguard/) (ID `26131`).

## Compared to other WireGuard exporters

| | wireguard-exporter | [MindFlavor/prometheus_wireguard_exporter](https://github.com/MindFlavor/prometheus_wireguard_exporter) | [mdlayher/wireguard_exporter](https://github.com/mdlayher/wireguard_exporter) |
|---|---|---|---|
| Status | Maintained | Last release 3.6.6 (2022) | Archived (2023) |
| Reads state via | Netlink | `wg show … dump` | [wgctrl-go](https://github.com/WireGuard/wgctrl-go) |
| `wg` binary needed | No | Yes | No |
| Privileges | `CAP_NET_ADMIN` only | Root or a sudo rule for `wg` | — |
| Userspace WireGuard (wireguard-go, boringtun) | No, kernel only | Yes, through `wg` | Yes |
| Peer names | YAML file, reloaded on change, extra labels | Comments in the WireGuard config | Flag or TOML file |
| Prebuilt binaries / images | `amd64`, `arm64` | Docker image for `amd64`, `386`, `arm64`, `armv7`, `armv6` | Build from source |
| Helm chart, Grafana dashboard, alert rules | Included | — | — |

If you need userspace WireGuard or 32-bit ARM, one of the other exporters
may suit you better.

## Quick start

```sh
# Container, monitoring the host's tunnels
docker run -d --name wireguard-exporter --network host \
  --cap-drop ALL --cap-add NET_ADMIN --security-opt no-new-privileges --read-only \
  ghcr.io/pessom/wireguard-exporter:0.3.2

curl -s localhost:9586/metrics
```

```
wireguard_peer_last_handshake_timestamp_seconds{interface="wg0",public_key="xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg="} 1762679431.0
wireguard_peer_receive_bytes_total{interface="wg0",public_key="xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg="} 532
wireguard_peer_info{interface="wg0",public_key="xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=",endpoint="203.0.113.7:51820",allowed_ips="10.0.0.2/32"} 1
```

Binary + systemd, Kubernetes (Helm, kustomize, sidecar): see
[installation](docs/installation.md).

## Documentation

| | |
|---|---|
| [Installation](docs/installation.md) | Binary + systemd, Docker/nerdctl/Podman, Kubernetes; required privileges, rootless limits |
| [Ansible](docs/ansible.md) | Example role without external dependencies (`ansible-pull`, offline mirrors) |
| [Configuration](docs/configuration.md) | Flags, environment variables, interface discovery, security notes |
| [Metrics](docs/metrics.md) | Metrics, labels, semantics, cardinality |
| [Prometheus and Grafana](docs/prometheus.md) | Scrape configs, **peer names**, PromQL, alerts, dashboard |
| [Troubleshooting](docs/troubleshooting.md) | Common problems |
| [Contributing](CONTRIBUTING.md) | Development, tests, releasing |
| [Support](SUPPORT.md) | Where to ask questions and report problems |

## License

[MIT](LICENSE)

# wireguard-exporter

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
  and a Grafana dashboard.

## Quick start

```sh
# Container, monitoring the host's tunnels
docker run -d --name wireguard-exporter --network host \
  --cap-drop ALL --cap-add NET_ADMIN --security-opt no-new-privileges --read-only \
  ghcr.io/pessom/wireguard-exporter:0.2.0

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
| [Configuration](docs/configuration.md) | Flags, environment variables, interface discovery, security notes |
| [Metrics](docs/metrics.md) | Metrics, labels, semantics, cardinality |
| [Prometheus and Grafana](docs/prometheus.md) | Scrape configs, **peer names**, PromQL, alerts, dashboard |
| [Troubleshooting](docs/troubleshooting.md) | Common problems |
| [Contributing](CONTRIBUTING.md) | Development, tests, releasing |

## License

[MIT](LICENSE)

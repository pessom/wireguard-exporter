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

Reading WireGuard state requires `CAP_NET_ADMIN` in the network namespace of
the interfaces. The exporter only sees interfaces of its own network
namespace. Nothing else is needed: no kernel module access, `/dev/net/tun` or
writable sysfs.

On a host, run it as an unprivileged user with an ambient capability, see
`contrib/wireguard_exporter.service`.

## Install

Download `wireguard_exporter-<version>.linux-<amd64|arm64>.tar.gz` from
[Releases](https://github.com/pessom/wireguard-exporter/releases) and verify it
with `sha256sums.txt`.

## Containers

Multi-arch image (`linux/amd64`, `linux/arm64`, `FROM scratch`):
`ghcr.io/pessom/wireguard-exporter:<version>` (also `<major>.<minor>` and
`latest`).

To monitor the host's tunnels, share the host network and grant only
`NET_ADMIN`:

```sh
docker run -d --name wireguard-exporter \
  --network host \
  --cap-drop ALL --cap-add NET_ADMIN \
  --security-opt no-new-privileges \
  --read-only \
  ghcr.io/pessom/wireguard-exporter:<version>
```

`nerdctl run` and `podman run` take the same flags; see also
`contrib/docker-compose.yml`.

- The process runs as UID 0 with a single capability. A non-root user would
  need file capabilities, which `no-new-privileges`
  (`allowPrivilegeEscalation: false`) disables.
- **Rootless runtimes** (rootless Docker/nerdctl/Podman, user namespaces):
  the container's `NET_ADMIN` does not apply to the host network namespace,
  so the host's tunnels cannot be read. Use a rootful runtime, or run the
  exporter next to WireGuard in the same network namespace (sidecar,
  `--network container:<wireguard-container>`).
- Instead of the host network, the exporter can join the network namespace of
  the container that owns the tunnel (`--network container:<name>`).

## Kubernetes

The exporter runs as a DaemonSet with `hostNetwork: true` and `NET_ADMIN`.
These are not allowed by the `baseline`/`restricted` Pod Security Standards,
so its namespace must be labelled
`pod-security.kubernetes.io/enforce: privileged`.

Helm (OCI chart):

```sh
helm install wireguard-exporter oci://ghcr.io/pessom/charts/wireguard-exporter \
  --version <version> --namespace wireguard-exporter --create-namespace \
  --set serviceMonitor.enabled=true   # with Prometheus Operator
kubectl label namespace wireguard-exporter pod-security.kubernetes.io/enforce=privileged
```

Kustomize (plain manifests, namespace included):

```sh
kubectl apply -k contrib/kubernetes
kubectl apply -f contrib/kubernetes/servicemonitor.yaml   # with Prometheus Operator
```

To monitor a WireGuard container inside a pod, add the exporter as a sidecar
(no `hostNetwork`), see `contrib/kubernetes/examples/sidecar.yaml`.

## Development

```
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Releases are built by GitHub Actions on `v*` tags (tag must match the version
in `Cargo.toml`). See [CONTRIBUTING.md](CONTRIBUTING.md) for details, including
the end-to-end test against real WireGuard.

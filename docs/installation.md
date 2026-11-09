# Installation

The exporter must run in the **network namespace of the WireGuard interfaces**
it monitors and needs **`CAP_NET_ADMIN`** there. Nothing else: no root, no
`wg` binary, no kernel module access, no `/dev/net/tun`, no writable sysfs.

| Method | Use when |
|---|---|
| [Binary + systemd](#binary--systemd) | WireGuard runs on a regular Linux host |
| [Ansible](ansible.md) | Same, automated (example role, works with `ansible-pull`) |
| [Docker / nerdctl / Podman](#containers) | You run everything in containers |
| [Kubernetes: Helm](#kubernetes-helm) | Nodes have WireGuard interfaces (CNI, site-to-site, gateways) |
| [Kubernetes: kustomize](#kubernetes-kustomize) | Same, plain manifests |
| [Kubernetes: sidecar](#kubernetes-sidecar) | WireGuard runs inside a pod |

Supported architectures: `amd64`, `arm64`. Linux only.

## Binary + systemd

```sh
VERSION=0.3.1 ARCH=amd64   # or arm64
curl -fsSLO https://github.com/pessom/wireguard-exporter/releases/download/v$VERSION/wireguard_exporter-$VERSION.linux-$ARCH.tar.gz
curl -fsSL  https://github.com/pessom/wireguard-exporter/releases/download/v$VERSION/sha256sums.txt | sha256sum -c --ignore-missing
tar -xzf wireguard_exporter-$VERSION.linux-$ARCH.tar.gz
sudo install -m 0755 wireguard_exporter-$VERSION.linux-$ARCH/wireguard_exporter /usr/local/bin/
sudo install -m 0644 wireguard_exporter-$VERSION.linux-$ARCH/wireguard_exporter.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now wireguard_exporter
curl -s localhost:9586/metrics | head
```

The unit runs the exporter as a dynamic unprivileged user with only
`CAP_NET_ADMIN` and a strict sandbox (read-only filesystem, no new
privileges, restricted address families and system calls, protected kernel
interfaces; `systemd-analyze security wireguard_exporter` rates it about
1.5 "OK"). Do not add `PrivateUsers=` or `PrivateNetwork=`: the exporter
needs `CAP_NET_ADMIN` in the host network namespace. Flags go to
`/etc/default/wireguard_exporter`:

```sh
WG_EXPORTER_OPTS="--web.listen-address 10.0.0.1:9586 --wireguard.peer-names-file /etc/wireguard_exporter/peers.yml"
```

The [peer names file](configuration.md#peer-names-file) must be readable by
the dynamic user: directory `0755`, file `0644` (it contains no secrets):

```sh
sudo install -d -m 0755 /etc/wireguard_exporter
sudo install -m 0644 peers.yml /etc/wireguard_exporter/peers.yml
```

Without systemd, grant the capability to the binary instead and run it as any
user: `sudo setcap cap_net_admin+ep /usr/local/bin/wireguard_exporter`.

## Containers

Image: `ghcr.io/pessom/wireguard-exporter:<version>` (also `<major>.<minor>`
and `latest`), multi-arch, `FROM scratch` (only the static binary).

To monitor the **host's** tunnels, share the host network and grant only
`NET_ADMIN`:

```sh
docker run -d --name wireguard-exporter --restart unless-stopped \
  --network host \
  --cap-drop ALL --cap-add NET_ADMIN \
  --security-opt no-new-privileges \
  --read-only \
  ghcr.io/pessom/wireguard-exporter:0.3.1
```

- **nerdctl** and **Podman** take exactly the same flags
  (`nerdctl run ...`, `podman run ...`).
- **Docker Compose**: [`contrib/docker-compose.yml`](../contrib/docker-compose.yml).
- To monitor a tunnel owned by **another container**, join its network
  namespace instead of the host's: `--network container:<wireguard-container>`.
- Arguments after the image name are exporter flags, see
  [configuration](configuration.md).
- Peer names: mount the directory with the
  [names file](configuration.md#peer-names-file) read-only. The file must be
  world-readable (`0644`): the container's root has no `DAC_OVERRIDE`.

  ```sh
  docker run ... -v /etc/wireguard_exporter:/etc/wireguard_exporter:ro \
    ghcr.io/pessom/wireguard-exporter:0.3.1 \
    --wireguard.peer-names-file /etc/wireguard_exporter/peers.yml
  ```

### Why UID 0

The process runs as UID 0 but with a single capability (`NET_ADMIN`); all
others are dropped, privilege escalation is disabled and the filesystem is
read-only. A non-root user would need file capabilities on the binary, and
those are ignored when `no-new-privileges` (Kubernetes:
`allowPrivilegeEscalation: false`) is set — that trade-off is worse.

### Rootless runtimes

With rootless Docker/nerdctl/Podman (or any user namespace), the container's
`NET_ADMIN` is only valid in namespaces owned by its user namespace. The host
network namespace is not one of them, so **the host's tunnels cannot be read**
(`scrape_success 0`, `Operation not permitted` in logs). Use a rootful
runtime, or run the exporter next to a WireGuard instance that itself runs in
the rootless container's network namespace (sidecar).

## Kubernetes: Helm

The chart deploys a DaemonSet with `hostNetwork: true` and `NET_ADMIN`, a
headless Service and, optionally, a Prometheus Operator `ServiceMonitor`.
`hostNetwork` and `NET_ADMIN` are rejected by the `baseline` and `restricted`
Pod Security Standards, so the namespace must be `privileged`:

```sh
kubectl create namespace wireguard-exporter
kubectl label namespace wireguard-exporter pod-security.kubernetes.io/enforce=privileged
helm install wireguard-exporter oci://ghcr.io/pessom/charts/wireguard-exporter \
  --version 0.3.1 --namespace wireguard-exporter \
  --set serviceMonitor.enabled=true
```

Common values (see [`values.yaml`](../charts/wireguard-exporter/values.yaml)):

| Value | Default | Description |
|---|---|---|
| `interfaces` | `[]` | Fixed interface list; empty = autodiscovery |
| `port` | `9586` | Port, opened on every node (host network) |
| `listenAddress` | `0.0.0.0:<port>` | Bind address |
| `nodeSelector` / `tolerations` | all nodes | Where to run |
| `serviceMonitor.enabled` | `false` | Create a `ServiceMonitor` |
| `serviceMonitor.labels` | `{}` | Labels your Prometheus selects ServiceMonitors by (e.g. `release: kube-prometheus-stack`) |
| `serviceMonitor.instanceFromNodeName` | `true` | `instance` = node name instead of `IP:port` |
| `peerNames` | `{}` | [Peer names](configuration.md#peer-names-file) (public key → name or `{name, labels...}`), rendered into a ConfigMap; changes apply without a restart |
| `existingPeerNamesConfigMap` | `""` | Use your ConfigMap (key `peerNamesConfigMapKey`, default `peers.yml`) instead of `peerNames` |
| `collector.peerEndpoint` | `true` | Export peer endpoints ([privacy](configuration.md#privacy)) |
| `collector.peerAllowedIps` | `true` | Export allowed IPs ([privacy](configuration.md#privacy)) |
| `logLevel` | `info` | `RUST_LOG` |

## Kubernetes: kustomize

```sh
kubectl apply -k contrib/kubernetes                      # namespace, DaemonSet, Service
kubectl apply -f contrib/kubernetes/servicemonitor.yaml  # with Prometheus Operator
```

For peer names, add a ConfigMap (e.g. via `configMapGenerator`), mount it into
the DaemonSet and add `--wireguard.peer-names-file` to its `args`; the Helm
chart does exactly that with `peerNames`.

Without the Prometheus Operator, the Service carries
`prometheus.io/scrape: "true"` annotations, see
[Prometheus configuration](prometheus.md#kubernetes).

## Kubernetes: sidecar

If WireGuard runs inside a pod, add the exporter as a second container: the
containers share the pod network namespace, so neither `hostNetwork` nor a
privileged namespace is needed. Example:
[`contrib/kubernetes/examples/sidecar.yaml`](../contrib/kubernetes/examples/sidecar.yaml).

# Configuration

Every flag can also be set with an environment variable.

| Flag | Env | Default | Description |
|---|---|---|---|
| `--web.listen-address` | `WG_EXPORTER_LISTEN` | `0.0.0.0:9586` | Address and port to listen on |
| `--web.telemetry-path` | `WG_EXPORTER_PATH` | `/metrics` | Path of the metrics endpoint |
| `--wireguard.interfaces` | `WG_EXPORTER_INTERFACES` | *(empty)* | Interfaces to export, comma-separated or repeated. Empty: autodiscovery |
| `--wireguard.peer-names-file` | `WG_EXPORTER_PEER_NAMES_FILE` | *(none)* | [Peer names file](#peer-names-file) (YAML) |
| `--no-collector.peer-endpoint` | `WG_EXPORTER_NO_PEER_ENDPOINT=true` | off | Do not export peer endpoints ([privacy](#privacy)) |
| `--no-collector.peer-allowed-ips` | `WG_EXPORTER_NO_PEER_ALLOWED_IPS=true` | off | Do not export allowed IPs ([privacy](#privacy)) |
| — | `RUST_LOG` | `info` | Log level/filter: `error`, `warn`, `info`, `debug`, `trace` |
| `--version`, `--help` | | | |

## Interface discovery

Without `--wireguard.interfaces`, every scrape lists
`/sys/class/net/*/uevent` and exports all devices with `DEVTYPE=wireguard`.
Interfaces created or removed later are picked up automatically.

With a fixed list, interfaces that do not exist (yet) are reported with
`wireguard_scrape_success{interface="..."} 0`, which is useful to alert on a
tunnel that is expected to be up.

## Peer names file

Maps peer public keys to friendly names, exported as
`wireguard_peer_name{interface, public_key, name, ...} 1`
([why a separate metric](prometheus.md#peer-names)):

```yaml
# /etc/wireguard_exporter/peers.yml
"xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=": alice-laptop
"HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=":
  name: office-gateway
  site: fra          # any extra labels
  owner: network
```

- Keys are base64 public keys as shown by `wg show`. Quote them (`+`, `/`
  and `=` are fine unquoted too, quotes just avoid surprises).
- Value: a name, or a map with `name` and extra labels. Label names must
  match `[a-zA-Z_][a-zA-Z0-9_]*`; `interface`, `public_key`, `name`, `job`,
  `instance` and `__*` are reserved.
- Only peers that exist on an interface are exported; entries for unknown
  keys are ignored.
- The file holds no secrets: the exporter never needs your WireGuard
  configs or private keys.
- **Reload**: the file is checked on every scrape and reloaded when its
  modification time or size changes — no restart or signal needed. This
  works with Kubernetes ConfigMap volumes (symlink swap) and with in-place
  edits. With a single-file Docker bind mount, edit the file in place: an
  editor that replaces the file (new inode) is not seen inside the container;
  mount the directory instead.
- **Errors**: an invalid or missing file at startup stops the exporter. At
  runtime the previous names are kept, a warning is logged and
  `wireguard_exporter_peer_names_load_success` becomes 0.

## Privacy

`wireguard_peer_info` exposes `endpoint` (the public IP of each peer, e.g. a
user's home address) and `allowed_ips`. If that is personal or sensitive
data in your environment:

- `--no-collector.peer-endpoint` leaves `endpoint` empty;
- `--no-collector.peer-allowed-ips` leaves `allowed_ips` empty and disables
  `wireguard_peer_allowed_ip`.

An empty label is the same as no label in Prometheus, so queries keep
working; joins on these labels return nothing.

## Endpoints

| Path | Content |
|---|---|
| `/metrics` (configurable) | Metrics in OpenMetrics text format; state is read from the kernel on each request |
| `/` | Small HTML page with a link to the metrics; used by Kubernetes probes |

## Security notes

- The endpoint has no authentication or TLS. Bind it to an internal address
  (`--web.listen-address 10.0.0.1:9586`), firewall the port, or put a reverse
  proxy in front of it.
- With the host network the port is opened on every node address; Kubernetes
  NetworkPolicies generally do not apply to `hostNetwork` pods.
- Metrics contain peer **public** keys, endpoints (public IPs of clients) and
  allowed IPs, see [privacy](#privacy). Private and preshared keys are never
  exposed.

## Logging

Logs go to stderr (journald / container logs), with colors only on a
terminal. Failed interface reads are logged as warnings on every scrape:

```
WARN failed to read interface interface=wg0 error=...
```

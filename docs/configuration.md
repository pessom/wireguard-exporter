# Configuration

Every flag can also be set with an environment variable.

| Flag | Env | Default | Description |
|---|---|---|---|
| `--web.listen-address` | `WG_EXPORTER_LISTEN` | `0.0.0.0:9586` | Address and port to listen on |
| `--web.telemetry-path` | `WG_EXPORTER_PATH` | `/metrics` | Path of the metrics endpoint |
| `--wireguard.interfaces` | `WG_EXPORTER_INTERFACES` | *(empty)* | Interfaces to export, comma-separated or repeated. Empty: autodiscovery |
| — | `RUST_LOG` | `info` | Log level/filter: `error`, `warn`, `info`, `debug`, `trace` |
| `--version`, `--help` | | | |

## Interface discovery

Without `--wireguard.interfaces`, every scrape lists
`/sys/class/net/*/uevent` and exports all devices with `DEVTYPE=wireguard`.
Interfaces created or removed later are picked up automatically.

With a fixed list, interfaces that do not exist (yet) are reported with
`wireguard_scrape_success{interface="..."} 0`, which is useful to alert on a
tunnel that is expected to be up.

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
  allowed IPs. Private keys are never read or exposed.

## Logging

Logs go to stderr (journald / container logs), with colors only on a
terminal. Failed interface reads are logged as warnings on every scrape:

```
WARN failed to read interface interface=wg0 error=...
```

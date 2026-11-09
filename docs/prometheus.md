# Prometheus and Grafana

Ready-to-use files:

| File | Content |
|---|---|
| [`contrib/prometheus/peer-names.rules.yml`](../contrib/prometheus/peer-names.rules.yml) | Example peer names (`wireguard_peer_name`) |
| [`contrib/prometheus/wireguard.rules.yml`](../contrib/prometheus/wireguard.rules.yml) | Recording rules: rates and handshake age with peer names |
| [`contrib/prometheus/wireguard.alerts.yml`](../contrib/prometheus/wireguard.alerts.yml) | Alerting rules |
| [`contrib/prometheus/tests/`](../contrib/prometheus/tests) | `promtool test rules` unit tests for the rules |
| [`contrib/grafana/wireguard.json`](../contrib/grafana/wireguard.json) | Grafana dashboard |

## Scrape configuration

### Static targets

```yaml
scrape_configs:
  - job_name: wireguard
    static_configs:
      - targets: ["vpn-1.example.com:9586", "vpn-2.example.com:9586"]
```

The alerting rules assume `job="wireguard"`; adjust them if you use another
job name.

### File-based discovery

```yaml
scrape_configs:
  - job_name: wireguard
    file_sd_configs:
      - files: ["/etc/prometheus/targets/wireguard/*.yml"]
```

```yaml
# /etc/prometheus/targets/wireguard/vpn.yml
- targets: ["vpn-1.example.com:9586"]
  labels:
    site: fra
```

### Kubernetes

With the Prometheus Operator, enable the `ServiceMonitor`
(`--set serviceMonitor.enabled=true` in Helm, or
`contrib/kubernetes/servicemonitor.yaml`) and make sure its labels match your
Prometheus' `serviceMonitorSelector`
(kube-prometheus-stack: `--set serviceMonitor.labels.release=<release-name>`).
The job is then named after the Service (`wireguard-exporter`), so change
`job="wireguard"` in the alerting rules accordingly. Rules files go into a
`PrometheusRule` resource (`spec.groups` = the `groups` of the file).

Without the operator, the headless Service is annotated with
`prometheus.io/scrape`/`port`/`path`; use the usual annotation-based
`kubernetes_sd_configs` (role `endpoints`) job.

## Peer names

Metrics identify peers by public key. The kernel does not store peer names
(`# Name` comments in `wg0.conf` are not visible over netlink), so names come
from a separate source and are exported as an **info metric**:

```
wireguard_peer_name{interface="wg0",public_key="xTIB...Dg=",name="alice-laptop",site="fra"} 1
```

Names are deliberately not added as a label to every series: renaming a peer
would split its history, and the set of labels of existing series would
change. Instead, queries join the name in — the recording rules and the
dashboard in `contrib/` do this for you.

| Source of `wireguard_peer_name` | Names change | Use when |
|---|---|---|
| **A. Exporter: `--wireguard.peer-names-file`** (recommended) | edit the file, picked up on the next scrape | you can give the exporter a file (systemd, container volume, Helm `peerNames`) |
| B. Prometheus recording rules | reload Prometheus | names are managed centrally in Prometheus |
| C. `metric_relabel_configs` | reload Prometheus | you want a `peer` label on raw series and accept the drawbacks |
| D. Grafana value mappings | dashboard edit | names only for display, not in alerts |

A and B produce the same metric and can be combined; the rules below
deduplicate it with `max by (public_key, name)`.

### A. Names file in the exporter (recommended)

```yaml
# /etc/wireguard_exporter/peers.yml
"xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=": alice-laptop
"HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=":
  name: office-gateway
  site: fra
```

```sh
wireguard_exporter --wireguard.peer-names-file /etc/wireguard_exporter/peers.yml
```

Helm: put the same map into `peerNames` (or reference a ConfigMap with
`existingPeerNamesConfigMap`). Format, reload and error handling:
[configuration](configuration.md#peer-names-file).

### B. Recording rules in Prometheus

When the exporter cannot get a file, define the metric in Prometheus with
recording rules with static labels
([`peer-names.rules.yml`](../contrib/prometheus/peer-names.rules.yml)) — no
extra component and no escaping of `+`, `/`, `=` in keys:

```yaml
groups:
  - name: wireguard-peer-names
    interval: 1m
    rules:
      - record: wireguard_peer_name
        expr: vector(1)
        labels:
          public_key: "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg="
          name: alice-laptop
```

In Kubernetes, put the group into a `PrometheusRule`. Any other source of the
same metric works too (e.g. a `node_exporter` textfile).

### Using names in queries

Load [`wireguard.rules.yml`](../contrib/prometheus/wireguard.rules.yml). It
records:

| Series | Description |
|---|---|
| `wireguard_peer:name:info` | `max by (public_key, name) (wireguard_peer_name)` — one series per key |
| `wireguard_peer:receive_bytes:rate5m` | bytes/s received, with `name` |
| `wireguard_peer:transmit_bytes:rate5m` | bytes/s sent, with `name` |
| `wireguard_peer:handshake_age:seconds` | seconds since the last handshake, with `name` (never-connected peers excluded) |

Peers without a name are **kept** (without the `name` label) thanks to
`or on (job, instance, interface, public_key) ...`; a plain `group_left`
join would silently drop them. Ad hoc:

```promql
wireguard_peer_last_handshake_timestamp_seconds
  * on (public_key) group_left (name) wireguard_peer:name:info
```

Notes:

- One name per key. Two different names for the same key (e.g. different
  files on two hosts) make joins fail with
  `many-to-many matching not allowed`.
- Extra labels (`site`, `owner`, ...) are dropped by `max by (public_key, name)`.
  To use them, add them to the `by (...)` list and to `group_left (...)`.
- Renaming a peer does not touch the raw metrics; only the recorded series
  start a new series with the new `name`.

### C. `metric_relabel_configs`

Adds a `peer` label directly to every scraped series:

```yaml
scrape_configs:
  - job_name: wireguard
    static_configs:
      - targets: ["vpn-1.example.com:9586"]
    metric_relabel_configs:
      - source_labels: [public_key]
        regex: 'xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg='
        target_label: peer
        replacement: alice-laptop
```

Simple queries, but: the regex is RE2 and anchored — escape `+` in keys as
`\+`; every rename changes the label of the raw series (history splits);
the config grows by one rule per peer and needs a Prometheus reload.

### D. Grafana only

Use *Value mappings* (or *Rename by regex*) on the `public_key` field. Names
appear only in that dashboard, not in alerts or PromQL.

## Useful queries

```promql
# Traffic per peer, bytes/s
rate(wireguard_peer_receive_bytes_total[5m])
rate(wireguard_peer_transmit_bytes_total[5m])

# Peers connected right now (handshake within 3 minutes)
count by (instance, interface) (time() - wireguard_peer_last_handshake_timestamp_seconds < 180)

# Peers that never completed a handshake
wireguard_peer_last_handshake_timestamp_seconds == 0

# Current endpoint of each peer
wireguard_peer_info

# Total traffic per peer over the last 24 hours
increase(wireguard_peer_receive_bytes_total[24h]) + increase(wireguard_peer_transmit_bytes_total[24h])

# Interfaces that cannot be read
wireguard_scrape_success == 0

# Which peer owns 10.0.0.2?
wireguard_peer_allowed_ip{cidr="10.0.0.2/32"}

# Always-on peers (keepalive set) without a handshake for 5 minutes
(time() - wireguard_peer_last_handshake_timestamp_seconds > 300)
  and on (instance, interface, public_key) wireguard_peer_persistent_keepalive_seconds > 0

# Peers without a preshared key
wireguard_peer_info{preshared_key="false"}

# Exporter versions in use
count by (version) (wireguard_exporter_build_info)
```

## Alerts

[`wireguard.alerts.yml`](../contrib/prometheus/wireguard.alerts.yml) (needs
`wireguard.rules.yml`):

| Alert | Condition | Severity |
|---|---|---|
| `WireGuardExporterDown` | target down for 5m | critical |
| `WireGuardInterfaceReadFailed` | `wireguard_scrape_success == 0` for 5m | warning |
| `WireGuardPeerHandshakeStale` | handshake older than 5m for a peer with `PersistentKeepalive`, for 5m | warning |

Peers without keepalive (road warriors) go idle legitimately and are not
alerted on. To alert on a specific peer without keepalive, add a rule on
`wireguard_peer:handshake_age:seconds{name="..."}`.

Check and test the rules after changes:

```sh
promtool check rules contrib/prometheus/*.rules.yml contrib/prometheus/*.alerts.yml
promtool test rules contrib/prometheus/tests/wireguard.test.yml
```

## Grafana

Import [`contrib/grafana/wireguard.json`](../contrib/grafana/wireguard.json)
(*Dashboards → New → Import*) and pick the Prometheus data source. Variables:
`job`, `instance`, `interface`. Panels: overview stats, a peer table (name,
key, endpoint, allowed IPs, last handshake, traffic), traffic and handshake
age per peer, interface read status. The peer table also shows keepalive and
whether a preshared key is used.

The dashboard uses only the raw metrics and joins `wireguard_peer_name` when
it exists; peers without a name are shown by the first 8 characters of their
key.

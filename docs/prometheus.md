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

Metrics identify peers by public key only. The exporter does not know peer
names: the kernel does not store them (`# Name` comments in `wg0.conf` are
not visible over netlink). Until the exporter supports names natively
([#9](https://github.com/pessom/wireguard-exporter/issues/9)), add them on the
Prometheus side.

| Approach | Names change without restart | Names in alerts | Effort per query |
|---|---|---|---|
| **A. Name metric + join** (recommended) | reload of rules | yes | join (hidden in recording rules) |
| B. `metric_relabel_configs` | reload of config | yes | none |
| C. Grafana value mappings | yes | no | none |

### A. Name metric + join (recommended)

1. Describe peers as a metric `wireguard_peer_name{public_key, name} 1`. The
   simplest source is a recording rule with static labels — central, no
   extra component and no escaping of `+`, `/`, `=` in keys
   ([`peer-names.rules.yml`](../contrib/prometheus/peer-names.rules.yml)):

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

   Generate the file from your WireGuard configs or inventory if you have
   many peers. Any other source of the same metric works too, e.g. a
   `node_exporter` textfile (`/var/lib/node_exporter/textfile/wg_peers.prom`):

   ```
   wireguard_peer_name{public_key="xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=",name="alice-laptop"} 1
   ```

2. Load [`wireguard.rules.yml`](../contrib/prometheus/wireguard.rules.yml).
   It deduplicates the names (`max by (public_key, name)`, so the same file on
   several hosts is fine) and records:

   | Series | Description |
   |---|---|
   | `wireguard_peer:receive_bytes:rate5m` | bytes/s received, with `name` |
   | `wireguard_peer:transmit_bytes:rate5m` | bytes/s sent, with `name` |
   | `wireguard_peer:handshake_age:seconds` | seconds since the last handshake, with `name` (never-connected peers excluded) |

   Peers without a name are **kept** (without the `name` label) thanks to
   `or on (job, instance, interface, public_key) ...`; a plain
   `group_left` join would silently drop them.

3. Use the recorded series in dashboards and alerts, or join ad hoc:

   ```promql
   wireguard_peer_last_handshake_timestamp_seconds
     * on (public_key) group_left (name) max by (public_key, name) (wireguard_peer_name)
   ```

Rules:

- One name per key. Two different names for the same key make the join
  fail with `many-to-many matching not allowed`.
- Renaming a peer does not touch the raw metrics; only the recorded series
  start a new series with the new `name`.
- Extra labels (`owner`, `site`, `critical`) can be added to
  `wireguard_peer_name` and to `group_left (name, owner, ...)`.

### B. `metric_relabel_configs`

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
      - source_labels: [public_key]
        regex: 'HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw='
        target_label: peer
        replacement: office-gateway
```

Simple queries, but: the regex is RE2 and anchored — escape `+` in keys as
`\+`; every rename changes the label of the raw series (history splits);
the config grows by one rule per peer and needs a Prometheus reload.

### C. Grafana only

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
```

## Alerts

[`wireguard.alerts.yml`](../contrib/prometheus/wireguard.alerts.yml) (needs
`wireguard.rules.yml`):

| Alert | Condition | Severity |
|---|---|---|
| `WireGuardExporterDown` | target down for 5m | critical |
| `WireGuardInterfaceReadFailed` | `wireguard_scrape_success == 0` for 5m | warning |
| `WireGuardPeerHandshakeStale` | handshake older than 5m, for 5m | warning |

`WireGuardPeerHandshakeStale` fits always-on peers (site-to-site,
`PersistentKeepalive`). Road-warrior clients go idle legitimately; restrict
the alert, e.g. `wireguard_peer:handshake_age:seconds{interface="wg-s2s"} > 300`,
or mark critical peers with a label in `wireguard_peer_name`.

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
age per peer, interface read status.

The dashboard uses only the raw metrics and joins `wireguard_peer_name` when
it exists; peers without a name are shown by the first 8 characters of their
key.

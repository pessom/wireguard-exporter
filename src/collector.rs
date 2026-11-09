//! Builds Prometheus metrics from a [`Source`] on every scrape.
//!
//! A fresh registry is created per scrape: counters are set to the absolute
//! values reported by the kernel, and peers that disappeared are not kept.

use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

use crate::model::Source;
use crate::names::Snapshot;

/// What to include in the output (privacy / cardinality switches).
#[derive(Debug, Clone)]
pub struct Options {
    /// `endpoint` label of `wireguard_peer_info` (peer public IPs).
    pub peer_endpoint: bool,
    /// `allowed_ips` label of `wireguard_peer_info` and `wireguard_peer_allowed_ip`.
    pub peer_allowed_ips: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            peer_endpoint: true,
            peer_allowed_ips: true,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct InterfaceLabels {
    interface: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct DeviceInfoLabels {
    interface: String,
    listen_port: String,
}

/// Labels of per-peer time series. Deliberately without `endpoint`:
/// roaming clients would otherwise create a new series on every move.
#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct PeerLabels {
    interface: String,
    public_key: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct PeerInfoLabels {
    interface: String,
    public_key: String,
    /// Empty when unknown or disabled (an empty label equals no label).
    endpoint: String,
    allowed_ips: String,
    preshared_key: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct AllowedIpLabels {
    interface: String,
    public_key: String,
    cidr: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct BuildInfoLabels {
    version: String,
}

type FloatGauge = Gauge<f64, AtomicU64>;

/// Collects all metrics and returns them in OpenMetrics text format.
pub fn collect(source: &dyn Source, opts: &Options, names: Option<&Snapshot>) -> String {
    let mut registry = Registry::default();

    let build_info = Family::<BuildInfoLabels, Gauge>::default();
    let scrape_success = Family::<InterfaceLabels, Gauge>::default();
    let scrape_duration = Family::<InterfaceLabels, FloatGauge>::default();
    let device_info = Family::<DeviceInfoLabels, Gauge>::default();
    let device_peers = Family::<InterfaceLabels, Gauge>::default();
    let peer_info = Family::<PeerInfoLabels, Gauge>::default();
    let peer_name = Family::<Vec<(String, String)>, Gauge>::default();
    let allowed_ip = Family::<AllowedIpLabels, Gauge>::default();
    let keepalive = Family::<PeerLabels, Gauge>::default();
    let last_handshake = Family::<PeerLabels, FloatGauge>::default();
    let rx_bytes = Family::<PeerLabels, Counter>::default();
    let tx_bytes = Family::<PeerLabels, Counter>::default();

    registry.register(
        "wireguard_exporter_build_info",
        "Exporter build information, value is always 1",
        build_info.clone(),
    );
    if let Some(snapshot) = names {
        let ok = Gauge::<i64, std::sync::atomic::AtomicI64>::default();
        ok.set(i64::from(snapshot.ok));
        registry.register(
            "wireguard_exporter_peer_names_load_success",
            "Whether the last (re)load of the peer names file succeeded (1) or failed (0)",
            ok,
        );
    }
    registry.register(
        "wireguard_scrape_success",
        "Whether reading the WireGuard interface succeeded (1) or failed (0)",
        scrape_success.clone(),
    );
    registry.register(
        "wireguard_scrape_duration_seconds",
        "Time spent reading the WireGuard interface",
        scrape_duration.clone(),
    );
    registry.register(
        "wireguard_device_info",
        "WireGuard interface information, value is always 1",
        device_info.clone(),
    );
    registry.register(
        "wireguard_device_peers",
        "Number of peers configured on the WireGuard interface",
        device_peers.clone(),
    );
    registry.register(
        "wireguard_peer_info",
        "WireGuard peer information, value is always 1",
        peer_info.clone(),
    );
    registry.register(
        "wireguard_peer_name",
        "Friendly peer name from the peer names file, value is always 1",
        peer_name.clone(),
    );
    registry.register(
        "wireguard_peer_allowed_ip",
        "Allowed IP range of the peer, one series per CIDR, value is always 1",
        allowed_ip.clone(),
    );
    registry.register(
        "wireguard_peer_persistent_keepalive_seconds",
        "Persistent keepalive interval of the peer, 0 if disabled",
        keepalive.clone(),
    );
    registry.register(
        "wireguard_peer_last_handshake_timestamp_seconds",
        "Unix timestamp of the last handshake with the peer, 0 if none",
        last_handshake.clone(),
    );
    // prometheus-client appends the `_total` suffix to counters.
    registry.register(
        "wireguard_peer_receive_bytes",
        "Bytes received from the peer",
        rx_bytes.clone(),
    );
    registry.register(
        "wireguard_peer_transmit_bytes",
        "Bytes sent to the peer",
        tx_bytes.clone(),
    );

    build_info
        .get_or_create(&BuildInfoLabels {
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
        .set(1);

    for interface in source.interfaces() {
        let iface_labels = InterfaceLabels {
            interface: interface.clone(),
        };
        let started = Instant::now();
        let result = source.read(&interface);
        scrape_duration
            .get_or_create(&iface_labels)
            .set(started.elapsed().as_secs_f64());
        let device = match result {
            Ok(device) => device,
            Err(err) => {
                tracing::warn!(%interface, error = %err, "failed to read interface");
                scrape_success.get_or_create(&iface_labels).set(0);
                continue;
            }
        };
        scrape_success.get_or_create(&iface_labels).set(1);
        device_info
            .get_or_create(&DeviceInfoLabels {
                interface: interface.clone(),
                listen_port: device.listen_port.to_string(),
            })
            .set(1);
        device_peers
            .get_or_create(&iface_labels)
            .set(i64::try_from(device.peers.len()).unwrap_or(i64::MAX));

        for peer in &device.peers {
            let labels = PeerLabels {
                interface: interface.clone(),
                public_key: peer.public_key.clone(),
            };
            peer_info
                .get_or_create(&PeerInfoLabels {
                    interface: interface.clone(),
                    public_key: peer.public_key.clone(),
                    endpoint: if opts.peer_endpoint {
                        peer.endpoint.clone().unwrap_or_default()
                    } else {
                        String::new()
                    },
                    allowed_ips: if opts.peer_allowed_ips {
                        peer.allowed_ips.join(",")
                    } else {
                        String::new()
                    },
                    preshared_key: peer.has_preshared_key.to_string(),
                })
                .set(1);
            if opts.peer_allowed_ips {
                for cidr in &peer.allowed_ips {
                    allowed_ip
                        .get_or_create(&AllowedIpLabels {
                            interface: interface.clone(),
                            public_key: peer.public_key.clone(),
                            cidr: cidr.clone(),
                        })
                        .set(1);
                }
            }
            if let Some(meta) = names.and_then(|n| n.names.get(&peer.public_key)) {
                let mut name_labels = vec![
                    ("interface".to_string(), interface.clone()),
                    ("public_key".to_string(), peer.public_key.clone()),
                    ("name".to_string(), meta.name.clone()),
                ];
                name_labels.extend(meta.labels.iter().cloned());
                peer_name.get_or_create(&name_labels).set(1);
            }
            keepalive
                .get_or_create(&labels)
                .set(i64::from(peer.persistent_keepalive_seconds));
            last_handshake
                .get_or_create(&labels)
                .set(peer.last_handshake_seconds);
            rx_bytes.get_or_create(&labels).inc_by(peer.rx_bytes);
            tx_bytes.get_or_create(&labels).inc_by(peer.tx_bytes);
        }
    }

    let mut out = String::new();
    // Writing into a String cannot fail.
    encode(&mut out, &registry).expect("encoding metrics into String");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Device, PeerStats, ReadResult};

    struct FakeSource;

    impl Source for FakeSource {
        fn interfaces(&self) -> Vec<String> {
            vec!["wg0".into(), "wg-broken".into()]
        }

        fn read(&self, interface: &str) -> ReadResult {
            match interface {
                "wg0" => Ok(Device {
                    listen_port: 51820,
                    peers: vec![
                        PeerStats {
                            public_key: "AAAA=".into(),
                            endpoint: Some("192.0.2.1:51820".into()),
                            allowed_ips: vec!["10.0.0.2/32".into(), "10.1.0.0/24".into()],
                            last_handshake_seconds: 1_793_613_028.0,
                            rx_bytes: 1024,
                            tx_bytes: 2048,
                            persistent_keepalive_seconds: 25,
                            has_preshared_key: true,
                        },
                        PeerStats {
                            public_key: "BBBB=".into(),
                            endpoint: None,
                            allowed_ips: vec![],
                            last_handshake_seconds: 0.0,
                            rx_bytes: 0,
                            tx_bytes: 0,
                            persistent_keepalive_seconds: 0,
                            has_preshared_key: false,
                        },
                    ],
                }),
                _ => Err("no such device".into()),
            }
        }
    }

    fn output() -> String {
        collect(&FakeSource, &Options::default(), None)
    }

    #[test]
    fn exports_handshake_timestamp() {
        assert!(output().contains(
            r#"wireguard_peer_last_handshake_timestamp_seconds{interface="wg0",public_key="AAAA="} 1793613028.0"#
        ));
    }

    #[test]
    fn exports_byte_counters_with_total_suffix() {
        let out = output();
        assert!(out.contains(
            r#"wireguard_peer_receive_bytes_total{interface="wg0",public_key="AAAA="} 1024"#
        ));
        assert!(out.contains(
            r#"wireguard_peer_transmit_bytes_total{interface="wg0",public_key="AAAA="} 2048"#
        ));
    }

    #[test]
    fn peer_without_handshake_is_zero() {
        assert!(output().contains(
            r#"wireguard_peer_last_handshake_timestamp_seconds{interface="wg0",public_key="BBBB="} 0.0"#
        ));
    }

    #[test]
    fn endpoint_only_in_info_metric() {
        let out = output();
        assert!(out.contains(
            r#"wireguard_peer_info{interface="wg0",public_key="AAAA=",endpoint="192.0.2.1:51820",allowed_ips="10.0.0.2/32,10.1.0.0/24",preshared_key="true"} 1"#
        ));
        for line in out
            .lines()
            .filter(|l| l.starts_with("wireguard_peer_receive"))
        {
            assert!(
                !line.contains("endpoint"),
                "unexpected endpoint label: {line}"
            );
        }
    }

    #[test]
    fn device_metrics() {
        let out = output();
        assert!(out.contains(r#"wireguard_device_info{interface="wg0",listen_port="51820"} 1"#));
        assert!(out.contains(r#"wireguard_device_peers{interface="wg0"} 2"#));
    }

    #[test]
    fn scrape_success_per_interface() {
        let out = output();
        assert!(out.contains(r#"wireguard_scrape_success{interface="wg0"} 1"#));
        assert!(out.contains(r#"wireguard_scrape_success{interface="wg-broken"} 0"#));
    }

    #[test]
    fn output_is_terminated_openmetrics() {
        assert!(output().trim_end().ends_with("# EOF"));
    }

    #[test]
    fn peer_without_allowed_ips_has_empty_label() {
        assert!(output().contains(
            r#"wireguard_peer_info{interface="wg0",public_key="BBBB=",endpoint="",allowed_ips="",preshared_key="false"} 1"#
        ));
    }

    #[test]
    fn failed_interface_has_no_peer_metrics() {
        let out = output();
        for line in out.lines().filter(|l| !l.starts_with('#')) {
            if line.contains(r#"interface="wg-broken""#) {
                assert!(
                    line.starts_with("wireguard_scrape_success")
                        || line.starts_with("wireguard_scrape_duration_seconds"),
                    "unexpected series for failed interface: {line}"
                );
            }
        }
    }

    struct NoInterfaces;

    impl Source for NoInterfaces {
        fn interfaces(&self) -> Vec<String> {
            Vec::new()
        }
        fn read(&self, _interface: &str) -> ReadResult {
            unreachable!("read must not be called without interfaces")
        }
    }

    #[test]
    fn no_interfaces_gives_only_build_info() {
        let out = collect(&NoInterfaces, &Options::default(), None);
        assert!(out.trim_end().ends_with("# EOF"));
        assert!(
            out.lines()
                .all(|l| l.starts_with('#') || l.starts_with("wireguard_exporter_build_info{")),
            "expected only HELP/TYPE/EOF lines and build_info, got:\n{out}"
        );
    }

    /// Counters must reflect the kernel's absolute values on every scrape,
    /// not accumulate across scrapes.
    struct Counting(std::sync::atomic::AtomicU64);

    impl Source for Counting {
        fn interfaces(&self) -> Vec<String> {
            vec!["wg0".into()]
        }
        fn read(&self, _interface: &str) -> ReadResult {
            let n = self.0.fetch_add(100, std::sync::atomic::Ordering::SeqCst) + 100;
            Ok(Device {
                listen_port: 0,
                peers: vec![PeerStats {
                    public_key: "AAAA=".into(),
                    endpoint: None,
                    allowed_ips: vec![],
                    last_handshake_seconds: 0.0,
                    rx_bytes: n,
                    tx_bytes: n,
                    persistent_keepalive_seconds: 0,
                    has_preshared_key: false,
                }],
            })
        }
    }

    #[test]
    fn counters_are_absolute_per_scrape() {
        let src = Counting(std::sync::atomic::AtomicU64::new(0));
        let _ = collect(&src, &Options::default(), None);
        let second = collect(&src, &Options::default(), None);
        assert!(second.contains(
            r#"wireguard_peer_receive_bytes_total{interface="wg0",public_key="AAAA="} 200"#
        ));
    }

    #[test]
    fn peers_disappear_when_removed() {
        struct Empty;
        impl Source for Empty {
            fn interfaces(&self) -> Vec<String> {
                vec!["wg0".into()]
            }
            fn read(&self, _i: &str) -> ReadResult {
                Ok(Device {
                    listen_port: 0,
                    peers: vec![],
                })
            }
        }
        let _ = collect(&FakeSource, &Options::default(), None);
        let out = collect(&Empty, &Options::default(), None);
        assert!(!out.contains("AAAA="));
        assert!(out.contains(r#"wireguard_device_peers{interface="wg0"} 0"#));
    }
    #[test]
    fn build_info_and_scrape_duration() {
        let out = output();
        assert!(out.contains(&format!(
            r#"wireguard_exporter_build_info{{version="{}"}} 1"#,
            env!("CARGO_PKG_VERSION")
        )));
        assert!(out.contains(r#"wireguard_scrape_duration_seconds{interface="wg0"} "#));
        assert!(out.contains(r#"wireguard_scrape_duration_seconds{interface="wg-broken"} "#));
    }

    #[test]
    fn keepalive_per_peer() {
        let out = output();
        assert!(out.contains(
            r#"wireguard_peer_persistent_keepalive_seconds{interface="wg0",public_key="AAAA="} 25"#
        ));
        assert!(out.contains(
            r#"wireguard_peer_persistent_keepalive_seconds{interface="wg0",public_key="BBBB="} 0"#
        ));
    }

    #[test]
    fn allowed_ip_series_per_cidr() {
        let out = output();
        assert!(out.contains(
            r#"wireguard_peer_allowed_ip{interface="wg0",public_key="AAAA=",cidr="10.0.0.2/32"} 1"#
        ));
        assert!(out.contains(
            r#"wireguard_peer_allowed_ip{interface="wg0",public_key="AAAA=",cidr="10.1.0.0/24"} 1"#
        ));
        assert!(!out.contains(r#"wireguard_peer_allowed_ip{interface="wg0",public_key="BBBB=""#));
    }

    #[test]
    fn privacy_options_blank_endpoint_and_allowed_ips() {
        let opts = Options {
            peer_endpoint: false,
            peer_allowed_ips: false,
        };
        let out = collect(&FakeSource, &opts, None);
        assert!(out.contains(
            r#"wireguard_peer_info{interface="wg0",public_key="AAAA=",endpoint="",allowed_ips="",preshared_key="true"} 1"#
        ));
        assert!(!out.contains("192.0.2.1"));
        assert!(!out.contains("10.0.0.2/32"));
        assert!(
            !out.lines()
                .any(|l| l.starts_with("wireguard_peer_allowed_ip{")),
            "allowed_ip series must be disabled"
        );
    }

    fn snapshot(ok: bool) -> Snapshot {
        use crate::names::PeerMeta;
        let mut names = crate::names::NameMap::new();
        names.insert(
            "AAAA=".into(),
            PeerMeta {
                name: "alice".into(),
                labels: vec![("site".into(), "fra".into())],
            },
        );
        names.insert(
            "ZZZZ=".into(),
            PeerMeta {
                name: "absent".into(),
                labels: vec![],
            },
        );
        Snapshot {
            names: std::sync::Arc::new(names),
            ok,
        }
    }

    #[test]
    fn peer_names_only_for_present_peers() {
        let out = collect(&FakeSource, &Options::default(), Some(&snapshot(true)));
        assert!(out.contains(
            r#"wireguard_peer_name{interface="wg0",public_key="AAAA=",name="alice",site="fra"} 1"#
        ));
        assert!(
            !out.contains("absent"),
            "names of absent peers must not be exported"
        );
        assert!(!out.contains(r#"wireguard_peer_name{interface="wg0",public_key="BBBB=""#));
        assert!(out.contains("wireguard_exporter_peer_names_load_success 1"));
    }

    #[test]
    fn peer_names_load_failure_is_reported() {
        let out = collect(&FakeSource, &Options::default(), Some(&snapshot(false)));
        assert!(out.contains("wireguard_exporter_peer_names_load_success 0"));
    }

    #[test]
    fn no_names_file_no_load_metric() {
        assert!(!output().contains("wireguard_exporter_peer_names_load_success"));
    }
}

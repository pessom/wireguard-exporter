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

use crate::model::Source;

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
    endpoint: String,
    allowed_ips: String,
}

type FloatGauge = Gauge<f64, AtomicU64>;

/// Collects all metrics and returns them in OpenMetrics text format.
pub fn collect(source: &dyn Source) -> String {
    let mut registry = Registry::default();

    let scrape_success = Family::<InterfaceLabels, Gauge>::default();
    let device_info = Family::<DeviceInfoLabels, Gauge>::default();
    let device_peers = Family::<InterfaceLabels, Gauge>::default();
    let peer_info = Family::<PeerInfoLabels, Gauge>::default();
    let last_handshake = Family::<PeerLabels, FloatGauge>::default();
    let rx_bytes = Family::<PeerLabels, Counter>::default();
    let tx_bytes = Family::<PeerLabels, Counter>::default();

    registry.register(
        "wireguard_scrape_success",
        "Whether reading the WireGuard interface succeeded (1) or failed (0)",
        scrape_success.clone(),
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

    for interface in source.interfaces() {
        let iface_labels = InterfaceLabels {
            interface: interface.clone(),
        };
        let device = match source.read(&interface) {
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
                    endpoint: peer.endpoint.clone().unwrap_or_default(),
                    allowed_ips: peer.allowed_ips.join(","),
                })
                .set(1);
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
                        },
                        PeerStats {
                            public_key: "BBBB=".into(),
                            endpoint: None,
                            allowed_ips: vec![],
                            last_handshake_seconds: 0.0,
                            rx_bytes: 0,
                            tx_bytes: 0,
                        },
                    ],
                }),
                _ => Err("no such device".into()),
            }
        }
    }

    fn output() -> String {
        collect(&FakeSource)
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
            r#"wireguard_peer_info{interface="wg0",public_key="AAAA=",endpoint="192.0.2.1:51820",allowed_ips="10.0.0.2/32,10.1.0.0/24"} 1"#
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
            r#"wireguard_peer_info{interface="wg0",public_key="BBBB=",endpoint="",allowed_ips=""} 1"#
        ));
    }

    #[test]
    fn failed_interface_has_no_peer_metrics() {
        let out = output();
        for line in out.lines().filter(|l| !l.starts_with('#')) {
            if line.contains(r#"interface="wg-broken""#) {
                assert!(
                    line.starts_with("wireguard_scrape_success"),
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
    fn no_interfaces_gives_no_samples() {
        let out = collect(&NoInterfaces);
        assert!(out.trim_end().ends_with("# EOF"));
        assert!(
            out.lines().all(|l| l.starts_with('#')),
            "expected only HELP/TYPE/EOF lines, got:\n{out}"
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
                }],
            })
        }
    }

    #[test]
    fn counters_are_absolute_per_scrape() {
        let src = Counting(std::sync::atomic::AtomicU64::new(0));
        let _ = collect(&src);
        let second = collect(&src);
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
        let _ = collect(&FakeSource);
        let out = collect(&Empty);
        assert!(!out.contains("AAAA="));
        assert!(out.contains(r#"wireguard_device_peers{interface="wg0"} 0"#));
    }
}

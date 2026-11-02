//! Real data source: Linux kernel WireGuard via netlink.
//! Requires CAP_NET_ADMIN (or root).

use std::fs;
use std::path::Path;

use crate::model::{ReadResult, Source};

pub struct NetlinkSource {
    /// Fixed list of interfaces; empty means autodiscovery.
    interfaces: Vec<String>,
}

impl NetlinkSource {
    pub fn new(interfaces: Vec<String>) -> Self {
        Self { interfaces }
    }
}

impl Source for NetlinkSource {
    fn interfaces(&self) -> Vec<String> {
        if self.interfaces.is_empty() {
            discover(Path::new("/sys/class/net"))
        } else {
            self.interfaces.clone()
        }
    }

    #[cfg(target_os = "linux")]
    fn read(&self, interface: &str) -> ReadResult {
        use crate::model::{Device, PeerStats, handshake_to_unix};
        use defguard_wireguard_rs::{Kernel, WGApi, WireguardInterfaceApi};

        let api = WGApi::<Kernel>::new(interface).map_err(|e| e.to_string())?;
        let host = api.read_interface_data().map_err(|e| e.to_string())?;

        let mut peers: Vec<PeerStats> = host
            .peers
            .values()
            .map(|p| PeerStats {
                public_key: p.public_key.to_string(),
                endpoint: p.endpoint.map(|e| e.to_string()),
                allowed_ips: p.allowed_ips.iter().map(ToString::to_string).collect(),
                last_handshake_seconds: handshake_to_unix(p.last_handshake),
                rx_bytes: p.rx_bytes,
                tx_bytes: p.tx_bytes,
            })
            .collect();
        // HashMap order is random; keep output stable.
        peers.sort_by(|a, b| a.public_key.cmp(&b.public_key));

        Ok(Device {
            listen_port: host.listen_port,
            peers,
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn read(&self, _interface: &str) -> ReadResult {
        Err("kernel WireGuard is supported on Linux only".into())
    }
}

/// Finds WireGuard interfaces: `/sys/class/net/<if>/uevent` has `DEVTYPE=wireguard`.
fn discover(sys_class_net: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(sys_class_net) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|e| {
            fs::read_to_string(e.path().join("uevent"))
                .map(|s| s.lines().any(|l| l.trim() == "DEVTYPE=wireguard"))
                .unwrap_or(false)
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_sysfs(tag: &str, devices: &[(&str, &str)]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("wg-exporter-test-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (name, uevent) in devices {
            let dir = root.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("uevent"), uevent).unwrap();
        }
        root
    }

    #[test]
    fn discovers_only_wireguard_devices_sorted() {
        let root = fake_sysfs(
            "mixed",
            &[
                (
                    "wg2-vlg",
                    "DEVTYPE=wireguard\nINTERFACE=wg2-vlg\nIFINDEX=7\n",
                ),
                ("eth0", "INTERFACE=eth0\nIFINDEX=2\n"),
                ("wg0-server", "DEVTYPE=wireguard\nINTERFACE=wg0-server\n"),
                ("br0", "DEVTYPE=bridge\nINTERFACE=br0\n"),
            ],
        );
        assert_eq!(discover(&root), vec!["wg0-server", "wg2-vlg"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_entries_without_uevent() {
        let root = fake_sysfs("no-uevent", &[("wg0", "DEVTYPE=wireguard\n")]);
        fs::create_dir_all(root.join("lo")).unwrap();
        fs::write(root.join("bonding_masters"), "").unwrap();
        assert_eq!(discover(&root), vec!["wg0"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn devtype_must_match_exactly() {
        let root = fake_sysfs(
            "exact",
            &[
                ("wgx", "DEVTYPE=wireguard-like\n"),
                ("wg1", "  DEVTYPE=wireguard  \n"),
            ],
        );
        assert_eq!(discover(&root), vec!["wg1"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn read_is_unsupported_off_linux() {
        assert!(NetlinkSource::new(vec![]).read("wg0").is_err());
    }

    #[test]
    fn missing_sysfs_gives_empty_list() {
        assert!(discover(Path::new("/nonexistent/sys/class/net")).is_empty());
    }

    #[test]
    fn fixed_interfaces_skip_discovery() {
        let src = NetlinkSource::new(vec!["wg9".into()]);
        assert_eq!(src.interfaces(), vec!["wg9"]);
    }
}

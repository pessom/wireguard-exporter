//! Platform-independent snapshot of WireGuard state.
//!
//! The collector works only with these types, so metrics encoding can be
//! unit-tested without a real WireGuard interface (and on non-Linux hosts).

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub listen_port: u16,
    pub peers: Vec<PeerStats>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeerStats {
    /// Base64-encoded public key, as shown by `wg show`.
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    /// Unix timestamp of the last handshake; 0 if there was none.
    pub last_handshake_seconds: f64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    /// Persistent keepalive interval in seconds; 0 if disabled.
    pub persistent_keepalive_seconds: u16,
    /// Whether a (non-zero) preshared key is configured.
    pub has_preshared_key: bool,
}

/// Converts the kernel's "last handshake" value into a unix timestamp.
/// `None` and the epoch itself both mean "never".
// Used by the Linux netlink source only.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn handshake_to_unix(ts: Option<SystemTime>) -> f64 {
    ts.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Result of reading one interface.
pub type ReadResult = Result<Device, String>;

/// Source of WireGuard data (real netlink on Linux, fake in tests).
pub trait Source: Send + Sync + 'static {
    /// Interfaces to export.
    fn interfaces(&self) -> Vec<String>;
    /// Reads current state of a single interface.
    fn read(&self, interface: &str) -> ReadResult;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn handshake_none_is_zero() {
        assert_eq!(handshake_to_unix(None), 0.0);
    }

    #[test]
    fn handshake_epoch_is_zero() {
        assert_eq!(handshake_to_unix(Some(UNIX_EPOCH)), 0.0);
    }

    #[test]
    fn handshake_is_unix_seconds() {
        let t = UNIX_EPOCH + Duration::from_secs(1_793_613_028);
        assert_eq!(handshake_to_unix(Some(t)), 1_793_613_028.0);
    }
}

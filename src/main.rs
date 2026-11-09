mod collector;
mod model;
mod names;
mod netlink;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use clap::Parser;

use crate::collector::Options;
use crate::model::Source;
use crate::names::PeerNames;
use crate::netlink::NetlinkSource;

const CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

/// Prometheus exporter for kernel WireGuard interfaces.
#[derive(Parser, Debug)]
#[command(name = "wireguard_exporter", version, about)]
struct Args {
    /// Address to listen on.
    #[arg(
        long = "web.listen-address",
        env = "WG_EXPORTER_LISTEN",
        default_value = "0.0.0.0:9586"
    )]
    listen: SocketAddr,

    /// Path to expose metrics on.
    #[arg(
        long = "web.telemetry-path",
        env = "WG_EXPORTER_PATH",
        default_value = "/metrics"
    )]
    path: String,

    /// Interfaces to export (comma-separated or repeated). Empty: autodiscover.
    #[arg(
        long = "wireguard.interfaces",
        env = "WG_EXPORTER_INTERFACES",
        value_delimiter = ','
    )]
    interfaces: Vec<String>,

    /// YAML file mapping peer public keys to names (and extra labels),
    /// exported as wireguard_peer_name. Reloaded when it changes.
    #[arg(
        long = "wireguard.peer-names-file",
        env = "WG_EXPORTER_PEER_NAMES_FILE"
    )]
    peer_names_file: Option<PathBuf>,

    /// Do not export peer endpoints (public IPs) in wireguard_peer_info.
    #[arg(
        long = "no-collector.peer-endpoint",
        env = "WG_EXPORTER_NO_PEER_ENDPOINT"
    )]
    no_peer_endpoint: bool,

    /// Do not export allowed IPs (wireguard_peer_info label and wireguard_peer_allowed_ip).
    #[arg(
        long = "no-collector.peer-allowed-ips",
        env = "WG_EXPORTER_NO_PEER_ALLOWED_IPS"
    )]
    no_peer_allowed_ips: bool,
}

impl Args {
    fn options(&self) -> Options {
        Options {
            peer_endpoint: !self.no_peer_endpoint,
            peer_allowed_ips: !self.no_peer_allowed_ips,
        }
    }
}

#[derive(Clone)]
struct AppState {
    source: Arc<dyn Source>,
    names: Option<Arc<PeerNames>>,
    options: Arc<Options>,
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    // Netlink calls and file checks are blocking.
    let collect = move || {
        let snapshot = state.names.as_ref().map(|n| n.snapshot());
        collector::collect(state.source.as_ref(), &state.options, snapshot.as_ref())
    };
    match tokio::task::spawn_blocking(collect).await {
        Ok(body) => (StatusCode::OK, [(header::CONTENT_TYPE, CONTENT_TYPE)], body).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "collector task failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "collector failed").into_response()
        }
    }
}

/// HTTP routes: metrics on `path`, a small index page on `/`.
fn router(state: AppState, path: &str) -> Router {
    let index = Html(format!(
        "<html><head><title>WireGuard Exporter</title></head><body>\
         <h1>WireGuard Exporter</h1><p><a href=\"{path}\">{path}</a></p></body></html>"
    ));
    Router::new()
        .route(path, get(metrics))
        .route("/", get(move || async move { index }))
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = term => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs go to stderr; colors only on a terminal (not in journald).
    let stderr_is_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(stderr_is_tty)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let names = args
        .peer_names_file
        .clone()
        .map(PeerNames::load)
        .transpose()?
        .map(Arc::new);
    let state = AppState {
        source: Arc::new(NetlinkSource::new(args.interfaces.clone())),
        names,
        options: Arc::new(args.options()),
    };
    let app = router(state, &args.path);

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;
    tracing::info!(
        listen = %args.listen,
        path = %args.path,
        interfaces = ?args.interfaces,
        peer_names_file = ?args.peer_names_file,
        peer_endpoint = !args.no_peer_endpoint,
        peer_allowed_ips = !args.no_peer_allowed_ips,
        version = env!("CARGO_PKG_VERSION"),
        "wireguard_exporter started"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Device, PeerStats, ReadResult};
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use clap::CommandFactory;
    use tower::ServiceExt;

    struct OnePeer;

    impl Source for OnePeer {
        fn interfaces(&self) -> Vec<String> {
            vec!["wg0".into()]
        }
        fn read(&self, _interface: &str) -> ReadResult {
            Ok(Device {
                listen_port: 51820,
                peers: vec![PeerStats {
                    public_key: "AAAA=".into(),
                    endpoint: None,
                    allowed_ips: vec!["10.0.0.2/32".into()],
                    last_handshake_seconds: 1.0,
                    rx_bytes: 1,
                    tx_bytes: 2,
                    persistent_keepalive_seconds: 0,
                    has_preshared_key: false,
                }],
            })
        }
    }

    async fn get(path: &str, metrics_path: &str) -> (StatusCode, String, String) {
        let state = AppState {
            source: Arc::new(OnePeer),
            names: None,
            options: Arc::new(Options::default()),
        };
        let resp = router(state, metrics_path)
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let ctype = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, ctype, String::from_utf8(body.to_vec()).unwrap())
    }

    // ---- HTTP ----

    #[tokio::test]
    async fn metrics_endpoint_returns_openmetrics() {
        let (status, ctype, body) = get("/metrics", "/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ctype, CONTENT_TYPE);
        assert!(body.contains(r#"wireguard_scrape_success{interface="wg0"} 1"#));
        assert!(body.trim_end().ends_with("# EOF"));
    }

    #[tokio::test]
    async fn index_links_to_metrics_path() {
        let (status, ctype, body) = get("/", "/metrics").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ctype.starts_with("text/html"));
        assert!(body.contains(r#"<a href="/metrics">"#));
    }

    #[tokio::test]
    async fn custom_metrics_path() {
        let (status, _, body) = get("/wg", "/wg").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("wireguard_device_peers"));
        let (status, _, _) = get("/metrics", "/wg").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unknown_path_is_404() {
        let (status, _, _) = get("/nope", "/metrics").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // ---- CLI ----

    #[test]
    fn cli_definition_is_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn cli_defaults() {
        let a = Args::try_parse_from(["wireguard_exporter"]).unwrap();
        assert_eq!(a.listen, "0.0.0.0:9586".parse().unwrap());
        assert_eq!(a.path, "/metrics");
        assert!(a.interfaces.is_empty());
        assert!(a.peer_names_file.is_none());
        let o = a.options();
        assert!(o.peer_endpoint && o.peer_allowed_ips);
    }

    #[test]
    fn cli_privacy_flags_and_names_file() {
        let a = Args::try_parse_from([
            "wireguard_exporter",
            "--no-collector.peer-endpoint",
            "--no-collector.peer-allowed-ips",
            "--wireguard.peer-names-file",
            "/etc/wireguard_exporter/peers.yml",
        ])
        .unwrap();
        let o = a.options();
        assert!(!o.peer_endpoint && !o.peer_allowed_ips);
        assert_eq!(
            a.peer_names_file.as_deref(),
            Some(std::path::Path::new("/etc/wireguard_exporter/peers.yml"))
        );
    }

    #[test]
    fn cli_interfaces_comma_separated_and_repeated() {
        let a = Args::try_parse_from([
            "wireguard_exporter",
            "--wireguard.interfaces",
            "wg0,wg1",
            "--wireguard.interfaces",
            "wg2",
        ])
        .unwrap();
        assert_eq!(a.interfaces, vec!["wg0", "wg1", "wg2"]);
    }

    #[test]
    fn cli_custom_listen_and_path() {
        let a = Args::try_parse_from([
            "wireguard_exporter",
            "--web.listen-address",
            "127.0.0.1:9999",
            "--web.telemetry-path",
            "/wg",
        ])
        .unwrap();
        assert_eq!(a.listen, "127.0.0.1:9999".parse().unwrap());
        assert_eq!(a.path, "/wg");
    }

    #[test]
    fn cli_rejects_bad_listen_address() {
        assert!(
            Args::try_parse_from(["wireguard_exporter", "--web.listen-address", "nonsense"])
                .is_err()
        );
    }
}

# Contributing

Thanks for your interest! Bug reports, ideas and pull requests are welcome.

## Getting started

1. Install Rust via [rustup](https://rustup.rs) (stable, at least the version
   in `rust-version` of `Cargo.toml`) with `clippy` and `rustfmt`.
2. Clone the repository and run the checks:

   ```sh
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

Unit tests do not need WireGuard or root and also run on macOS: netlink
access is hidden behind the `Source` trait (`src/model.rs`), and tests use
fake sources.

## End-to-end test (Linux)

`scripts/e2e.sh` checks the exporter against real kernel WireGuard: it creates
a tunnel between two network namespaces, triggers a handshake and verifies the
metrics. It needs `sudo`, `wireguard-tools`, `iproute2` and `setcap`
(`libcap2-bin`):

```sh
cargo build --release
scripts/e2e.sh target/release/wireguard_exporter
```

CI runs it on every push and pull request.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | CLI flags, HTTP server, routes |
| `src/collector.rs` | builds metrics from a `Source` on every scrape |
| `src/netlink.rs` | real `Source`: kernel WireGuard via netlink, interface discovery |
| `src/model.rs` | platform-independent data types and the `Source` trait |
| `contrib/` | systemd unit shipped in release archives |
| `scripts/e2e.sh` | end-to-end test |

## Pull requests

- Keep a PR focused on one change; add or update tests for it.
- CI (`fmt`, `clippy`, unit tests, e2e) must be green.
- Update `README.md` when flags, metrics or behaviour change.
- Write commit messages in the imperative mood, with a short summary line
  (≤ 72 chars), e.g. `collector: add wireguard_device_peers metric`.

## Metrics compatibility

Metric and label names are a public contract: dashboards and alerts depend on
them.

- Do not rename or remove metrics or labels in a minor/patch release.
- Do not add high-cardinality or frequently changing labels (such as
  `endpoint`) to per-peer time series; put them into `wireguard_peer_info`.
- Follow [Prometheus naming conventions](https://prometheus.io/docs/practices/naming/):
  base units (seconds, bytes), `_total` for counters.

## Releasing (maintainers)

1. Bump `version` in `Cargo.toml` (SemVer), run `cargo check` to update
   `Cargo.lock`, commit.
2. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The `Release` workflow verifies the tag matches `Cargo.toml`, runs tests,
   builds a static `x86_64-unknown-linux-musl` binary and publishes
   `wireguard_exporter-X.Y.Z.linux-amd64.tar.gz` with `sha256sums.txt`.

## Reporting bugs

Open an issue with the exporter version (`wireguard_exporter --version`),
kernel version (`uname -r`), the command line used and the relevant part of
`/metrics` or logs. Never post private keys.

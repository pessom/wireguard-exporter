# Contributing

Thanks for your interest! Bug reports, ideas and pull requests are welcome.
Please follow the [Code of Conduct](CODE_OF_CONDUCT.md).

- Questions: [Discussions → Q&A](https://github.com/pessom/wireguard-exporter/discussions/categories/q-a).
- Bugs and feature requests: [issue templates](https://github.com/pessom/wireguard-exporter/issues/new/choose).
- For larger changes (new metrics, flags, deployment options) open a feature
  request first, so the design can be agreed before you write code.

## Getting started

1. Install Rust via [rustup](https://rustup.rs). The compiler version is
   pinned in `rust-toolchain.toml`; rustup installs it (with `clippy` and
   `rustfmt`) automatically.
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

# the same against the container image (E2E_RUNTIME=nerdctl|podman also work)
mkdir -p dist/amd64 && cp target/x86_64-unknown-linux-musl/release/wireguard_exporter dist/amd64/
docker build -t wireguard-exporter:dev .
scripts/e2e.sh --image wireguard-exporter:dev
```

CI runs both variants on every push and pull request, and weekly on a
schedule.

## Dependencies

- `deny.toml` configures [cargo-deny](https://embarkstudios.github.io/cargo-deny/):
  RustSec advisories, allowed licenses, duplicate crates and sources. CI runs
  it on every PR and weekly; locally: `cargo deny check`.
- Dependabot bumps direct crates (weekly), GitHub Actions and the Rust
  toolchain (monthly), skipping releases younger than 7 days.
- The `cargo update` workflow refreshes transitive dependencies in
  `Cargo.lock` monthly. It runs the full CI first and opens a PR only if it
  passes; that PR shows no checks (close and reopen it to run CI).
- GitHub Actions are pinned by commit SHA with the version in a comment.
- When bumping `rust-toolchain.toml`, keep `rust-version` in `Cargo.toml` in
  sync.

## Project layout

| Path | Purpose |
|---|---|
| `src/main.rs` | CLI flags, HTTP server, routes |
| `src/collector.rs` | builds metrics from a `Source` on every scrape |
| `src/netlink.rs` | real `Source`: kernel WireGuard via netlink, interface discovery |
| `src/model.rs` | platform-independent data types and the `Source` trait |
| `contrib/` | systemd unit (shipped in release archives), docker-compose example |
| `contrib/kubernetes/` | kustomize manifests, ServiceMonitor, sidecar example |
| `contrib/prometheus/` | peer names example, recording and alerting rules, `promtool` tests |
| `contrib/grafana/` | Grafana dashboard |
| `docs/` | user documentation |
| `charts/wireguard-exporter/` | Helm chart (published to `oci://ghcr.io/pessom/charts`) |
| `Dockerfile` | runtime image from prebuilt binaries in `dist/<arch>/` |
| `scripts/e2e.sh` | end-to-end test |

## Pull requests

- Keep a PR focused on one change; add or update tests for it.
- CI (`fmt`, `clippy`, unit tests, `cargo deny`, e2e for binary and image,
  Helm/kustomize validation, `promtool` rule tests) must be green.
- Update `docs/` (and `README.md` if needed) when flags, metrics or behaviour
  change; keep `contrib/prometheus` rules, their tests and the Grafana
  dashboard in sync with metric changes.
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

1. Bump the version (SemVer) in `Cargo.toml` (run `cargo check` to update
   `Cargo.lock`), `version`/`appVersion` in `charts/wireguard-exporter/Chart.yaml`,
   `newTag` in `contrib/kubernetes/kustomization.yaml` and the versions in
   examples in `contrib/`, `docs/` and `README.md`
   (`grep -rnE 'wireguard-exporter:[0-9]|--version [0-9]|VERSION=[0-9]' contrib docs README.md`);
   commit.
2. Tag and push: `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. The `Release` workflow verifies the tag matches `Cargo.toml`, the chart and
   the kustomization, runs tests, builds static musl binaries for amd64 and
   arm64 and publishes:
   - `wireguard_exporter-X.Y.Z.linux-<arch>.tar.gz` with `sha256sums.txt`;
   - image `ghcr.io/pessom/wireguard-exporter:X.Y.Z` (`X.Y`, `latest`);
   - chart `oci://ghcr.io/pessom/charts/wireguard-exporter` version `X.Y.Z`.

## Issue triage (maintainers)

| Label | Meaning |
|---|---|
| `needs-triage` | New, not reviewed yet (set by the issue templates) |
| `needs-info` | Waiting for the author; closed automatically after 14 + 7 days without activity |
| `bug`, `enhancement`, `documentation`, `question` | Type |
| `breaking-change` | Changes the metrics contract or flags incompatibly |
| `security` | Security-relevant change |
| `good first issue`, `help wanted` | Open for contributors |

Labels on merged PRs define the sections of the release notes
(`.github/release.yml`): `breaking-change`, `security`, `enhancement`, `bug`,
`documentation`, `dependencies`. PRs are merged with squash; the PR title
becomes the commit message.

## Reporting bugs

Use the [bug report template](https://github.com/pessom/wireguard-exporter/issues/new?template=bug_report.yml):
it asks for the exporter version, how it is run, kernel version, the command
line and the relevant part of `/metrics` or logs. Never post private keys.

# Repository rules

## Release: a new tag means updating the documentation

The documentation and examples contain installation commands pinned to a specific
GitHub release (release archives, the `ghcr.io/pessom/wireguard-exporter:<version>`
image, the Helm chart). A new `vX.Y.Z` tag must therefore never be pushed before
the version is updated in all of these places.

Steps:

1. Open a separate `Release X.Y.Z` PR (branch `chore/release-X.Y.Z`) that updates
   the version everywhere:
   - `Cargo.toml` and `Cargo.lock` (the `wireguard-exporter` package);
   - `charts/wireguard-exporter/Chart.yaml`: `version` and `appVersion`;
   - `contrib/kubernetes/kustomization.yaml`: `newTag`;
   - `contrib/docker-compose.yml`, `contrib/kubernetes/examples/sidecar.yaml`: image tag;
   - `README.md`, `docs/installation.md`: image tag, `VERSION=`, `--version`;
   - `docs/ansible.md`: `wireguard_exporter_version`;
   - `.github/ISSUE_TEMPLATE/bug_report.yml`: `placeholder`.
   Leave historical version mentions alone (e.g. "archives older than 0.3.1").
2. Make sure the old version is gone: `git grep -n "<old version>"` (ignoring
   `Cargo.lock`), and run `cargo test --locked`.
3. Wait for CI to pass, then merge the PR.
4. Only then create an annotated `vX.Y.Z` tag on `main` and push it.
   The `Release` workflow (`.github/workflows/release.yml`) checks that the version
   in Cargo.toml, the chart, kustomize, `contrib/`, `docs/` and `README.md` matches
   the tag, and fails if anything was not updated.

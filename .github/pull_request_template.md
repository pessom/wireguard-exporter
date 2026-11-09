## What and why

<!-- What does this change and why? Link the issue: "Closes #123". -->

## Checklist

- [ ] Tests added or updated (unit tests; `scripts/e2e.sh` for netlink/runtime behaviour)
- [ ] `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Docs updated (`docs/`, `README.md`) for new or changed flags, metrics or behaviour
- [ ] Metrics compatibility: no renamed or removed metrics/labels, no high-cardinality labels on per-peer series
- [ ] `contrib/` (Prometheus rules and tests, Grafana dashboard), Helm chart and manifests updated if affected

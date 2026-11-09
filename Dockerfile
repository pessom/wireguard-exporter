# Runtime image only: the static musl binary is built outside Docker
# (see .github/workflows/build.yml) and placed in dist/<arch>/.
#
#   cargo build --release --target x86_64-unknown-linux-musl
#   mkdir -p dist/amd64 && cp target/x86_64-unknown-linux-musl/release/wireguard_exporter dist/amd64/
#   docker build -t wireguard-exporter .
FROM scratch

ARG TARGETARCH
COPY --chmod=0755 dist/${TARGETARCH}/wireguard_exporter /wireguard_exporter

# UID 0 is intended: the process needs CAP_NET_ADMIN and nothing else. Run with
# --cap-drop ALL --cap-add NET_ADMIN --security-opt no-new-privileges --read-only.
# Non-root would require file capabilities, which no-new-privileges disables.
USER 0:0
EXPOSE 9586
ENTRYPOINT ["/wireguard_exporter"]

#!/usr/bin/env bash
# Build the .deb and .rpm packages for one architecture with nfpm.
#
# Usage: scripts/package.sh <version> <amd64|arm64> <binary> <outdir>
#
# The package ships the hardened unit from contrib/ with the binary path
# changed to /usr/bin (packages do not install into /usr/local).
set -euo pipefail

USAGE="usage: $0 <version> <amd64|arm64> <binary> <outdir>"
VERSION="${1:?$USAGE}"
ARCH="${2:?$USAGE}"
BIN="$(realpath "${3:?$USAGE}")"
OUT="${4:?$USAGE}"
case "$ARCH" in amd64 | arm64) ;; *) echo "$USAGE" >&2; exit 2 ;; esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$OUT"
OUT="$(realpath "$OUT")"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/packaging"
cp -R "$ROOT/packaging/scripts" "$ROOT/packaging/wireguard_exporter.default" "$STAGE/packaging/"
cp "$ROOT/LICENSE" "$ROOT/README.md" "$STAGE/"
install -m 0755 "$BIN" "$STAGE/wireguard_exporter"
sed 's#^ExecStart=/usr/local/bin/wireguard_exporter #ExecStart=/usr/bin/wireguard_exporter #' \
    "$ROOT/contrib/wireguard_exporter.service" > "$STAGE/wireguard_exporter.service"
grep -q '^ExecStart=/usr/bin/wireguard_exporter ' "$STAGE/wireguard_exporter.service" \
    || { echo "ExecStart not rewritten in the unit" >&2; exit 1; }
sed -e "s/@VERSION@/$VERSION/" -e "s/@ARCH@/$ARCH/" "$ROOT/packaging/nfpm.yaml" > "$STAGE/nfpm.yaml"

cd "$STAGE"
for fmt in deb rpm; do
    nfpm package --config nfpm.yaml --packager "$fmt" --target "$OUT/"
done
ls -l "$OUT"

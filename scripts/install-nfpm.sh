#!/usr/bin/env bash
# Install a pinned nfpm (Linux x86_64) into /usr/local/bin, verifying its
# checksum. Used by CI and the Release workflow.
set -euo pipefail

NFPM_VERSION=2.49.0
NFPM_SHA256=2afaa5ffb5e3493402e31e7c67a5a1fb42133e3f66fc315ca0051934125561c7
ASSET="nfpm_${NFPM_VERSION}_Linux_x86_64.tar.gz"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL -o "$TMP/$ASSET" \
    "https://github.com/goreleaser/nfpm/releases/download/v${NFPM_VERSION}/${ASSET}"
echo "${NFPM_SHA256}  $TMP/$ASSET" | sha256sum -c -
tar -xzf "$TMP/$ASSET" -C "$TMP" nfpm
sudo install -m 0755 "$TMP/nfpm" /usr/local/bin/nfpm
nfpm --version | grep -i version

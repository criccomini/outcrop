#!/bin/sh
# Outcrop installer: downloads the latest release binary for this platform,
# verifies its checksum, and installs it to ~/.local/bin (no sudo).
#
#   curl -fsSL https://raw.githubusercontent.com/criccomini/outcrop/main/install.sh | sh
#
# Environment:
#   OUTCROP_VERSION  release tag to install (default: latest)
#   OUTCROP_INSTALL  install directory (default: ~/.local/bin)
set -eu

REPO="criccomini/outcrop"
INSTALL_DIR="${OUTCROP_INSTALL:-$HOME/.local/bin}"

err() { echo "install.sh: $*" >&2; exit 1; }

OS=$(uname -s)
ARCH=$(uname -m)
case "$OS-$ARCH" in
  Linux-x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64 | Linux-arm64) TARGET="aarch64-unknown-linux-gnu" ;;
  Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
  Darwin-x86_64) err "no prebuilt binary for Intel Macs; build from source (see README)" ;;
  *) err "unsupported platform: $OS $ARCH (Windows: grab the zip from the releases page)" ;;
esac

VERSION="${OUTCROP_VERSION:-}"
if [ -z "$VERSION" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')
  [ -n "$VERSION" ] || err "could not determine the latest release"
fi

ASSET="outcrop-$VERSION-$TARGET.tar.gz"
BASE="https://github.com/$REPO/releases/download/$VERSION"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "downloading $ASSET ..."
curl -fsSL "$BASE/$ASSET" -o "$TMP/$ASSET"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS"

cd "$TMP"
if command -v sha256sum >/dev/null 2>&1; then
  grep "  $ASSET\$" SHA256SUMS | sha256sum -c - >/dev/null || err "checksum mismatch"
elif command -v shasum >/dev/null 2>&1; then
  grep "  $ASSET\$" SHA256SUMS | shasum -a 256 -c - >/dev/null || err "checksum mismatch"
else
  echo "warning: no sha256 tool found; skipping checksum verification" >&2
fi

tar xzf "$ASSET"
mkdir -p "$INSTALL_DIR"
install -m 755 "outcrop-$VERSION-$TARGET/outcrop" "$INSTALL_DIR/outcrop"

echo "installed $("$INSTALL_DIR/outcrop" --version) to $INSTALL_DIR/outcrop"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "note: $INSTALL_DIR is not on your PATH" >&2 ;;
esac

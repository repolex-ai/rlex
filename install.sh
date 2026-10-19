#!/bin/sh
# Install the latest rlex release.
#   curl -fsSL https://raw.githubusercontent.com/repolex-ai/rlex/main/install.sh | sh
# Installs to ~/.local/bin, or to $RLEX_INSTALL_DIR if set.
set -eu

REPO="repolex-ai/rlex"
DIR="${RLEX_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-gnu ;;
  *) echo "rlex: unsupported OS $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  arm64|aarch64) arch=aarch64 ;;
  x86_64|amd64) arch=x86_64 ;;
  *) echo "rlex: unsupported CPU $(uname -m)" >&2; exit 1 ;;
esac

url="https://github.com/${REPO}/releases/latest/download/rlex-${arch}-${os}.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $url"
curl -fsSL "$url" -o "$tmp/rlex.tar.gz"
tar -xzf "$tmp/rlex.tar.gz" -C "$tmp"
mkdir -p "$DIR"
mv "$tmp/rlex" "$DIR/rlex"
chmod +x "$DIR/rlex"
echo "Installed rlex to $DIR/rlex"

case ":$PATH:" in
  *":$DIR:"*) ;;
  *) echo "Add $DIR to your PATH, e.g.: echo 'export PATH=\"$DIR:\$PATH\"' >> ~/.zshrc  (or ~/.bashrc), then open a new terminal." ;;
esac

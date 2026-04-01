#!/bin/sh
# Install magic-code
# Usage: curl -fsSL https://raw.githubusercontent.com/kienbui1995/magic-code/main/install.sh | sh
set -e

REPO="kienbui1995/magic-code"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  PLATFORM="linux" ;;
  darwin) PLATFORM="macos" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

NAME="magic-code-${PLATFORM}-${ARCH}"

# Get latest release tag
if command -v curl >/dev/null 2>&1; then
  LATEST=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
elif command -v wget >/dev/null 2>&1; then
  LATEST=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | head -1 | cut -d'"' -f4)
else
  echo "Error: curl or wget required"; exit 1
fi

if [ -z "$LATEST" ]; then
  echo "Error: could not determine latest release"
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$LATEST/$NAME.tar.gz"

echo "Installing magic-code $LATEST ($PLATFORM/$ARCH)..."
echo "  From: $URL"
echo "  To:   $INSTALL_DIR/magic-code"

# Download and extract
mkdir -p "$INSTALL_DIR"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$TMPDIR/magic-code.tar.gz"
else
  wget -q "$URL" -O "$TMPDIR/magic-code.tar.gz"
fi

tar xzf "$TMPDIR/magic-code.tar.gz" -C "$TMPDIR"
mv "$TMPDIR/magic-code" "$INSTALL_DIR/magic-code"
chmod +x "$INSTALL_DIR/magic-code"

echo ""
echo "✓ magic-code installed to $INSTALL_DIR/magic-code"

# Check PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Add to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

echo ""
echo "Get started:"
echo "  magic-code --help"

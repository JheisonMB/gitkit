#!/bin/sh
# install.sh — download and install gitkit from GitHub Releases
# Usage: curl -fsSL https://raw.githubusercontent.com/UniverLab/gitkit/main/scripts/install.sh | sh
set -eu

REPO="UniverLab/gitkit"
BINARY="gitkit"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

info() { printf '  \033[1;34m%s\033[0m %s\n' "$1" "$2"; }
error() { printf '  \033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# --- detect OS ---
OS="$(uname -s)"
case "$OS" in
  Linux*)  OS_TARGET="unknown-linux-musl" ;;
  Darwin*) OS_TARGET="apple-darwin" ;;
  *)       error "Unsupported OS: $OS (only Linux and macOS are supported)" ;;
esac

# --- detect arch ---
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64)  ARCH_TARGET="x86_64" ;;
  arm64|aarch64) ARCH_TARGET="aarch64" ;;
  *)             error "Unsupported architecture: $ARCH" ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"
info "platform" "$TARGET"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

# --- resolve version ---
if [ -n "${VERSION:-}" ]; then
  TAG="v$VERSION"
  info "version" "$TAG (pinned)"
else
  TAG=$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" | rev | cut -d'/' -f1 | rev)
  [ -z "$TAG" ] && error "Could not resolve latest release tag"
  info "version" "$TAG (latest)"
fi

# --- download ---
ARCHIVE="${BINARY}-${TAG}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/${TAG}/${ARCHIVE}"

info "download" "$URL"
HTTP_CODE=$(curl -fSL -w '%{http_code}' -o "$TMPDIR/$ARCHIVE" "$URL" 2>/dev/null) || true
[ "$HTTP_CODE" = "200" ] || error "Download failed (HTTP $HTTP_CODE). Check that $TAG exists for $TARGET at:\n  $URL"

# --- verify checksum ---
# Match against SHA256SUMS.txt from the same release. Missing sums file (older
# releases) or no sha256 tool → skip; a present-but-mismatched checksum is fatal.
SUMS_URL="https://github.com/$REPO/releases/download/${TAG}/SHA256SUMS.txt"
if curl -fsSL -o "$TMPDIR/SHA256SUMS.txt" "$SUMS_URL" 2>/dev/null; then
  EXPECTED=$(awk -v f="$ARCHIVE" '$2 == f { print $1 }' "$TMPDIR/SHA256SUMS.txt" | head -1)
  [ -n "$EXPECTED" ] || error "No checksum listed for $ARCHIVE in SHA256SUMS.txt"
  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "$TMPDIR/$ARCHIVE" | awk '{ print $1 }')
  elif command -v shasum >/dev/null 2>&1; then
    ACTUAL=$(shasum -a 256 "$TMPDIR/$ARCHIVE" | awk '{ print $1 }')
  else
    ACTUAL=""
    info "checksum" "no sha256 tool found — skipping verification"
  fi
  if [ -n "$ACTUAL" ]; then
    [ "$ACTUAL" = "$EXPECTED" ] || error "Checksum mismatch for $ARCHIVE (expected $EXPECTED, got $ACTUAL)"
    info "checksum" "verified"
  fi
else
  info "checksum" "SHA256SUMS.txt not found for $TAG — skipping verification"
fi

# --- extract ---
tar xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR"
[ -f "$TMPDIR/$BINARY" ] || error "Binary not found in archive"

# --- install ---
mkdir -p "$INSTALL_DIR"
mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"
info "installed" "$INSTALL_DIR/$BINARY"

# --- ensure PATH ---
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    export PATH="$INSTALL_DIR:$PATH"
    for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
      if [ -f "$profile" ]; then
        if ! grep -q "export PATH=\"$INSTALL_DIR:\$PATH\"" "$profile" 2>/dev/null; then
          printf '\n# Added by gitkit installer\nexport PATH="%s:$PATH"\n' "$INSTALL_DIR" >> "$profile"
          info "updated" "$profile"
        fi
      fi
    done
    ;;
esac

# --- verify ---
info "done" "$($INSTALL_DIR/$BINARY --version 2>/dev/null || echo "$BINARY installed")"
echo ""
info "ready" "Run 'gitkit init' to get started!"

#!/bin/sh
set -eu

REPO="https://github.com/logancammish/smartextract"
INSTALL_DIR="${SMARTEXTRACT_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
fail() { say "SmartExtract installer: $*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || fail "Rust is required. Install it from https://rustup.rs and run this command again."
command -v 7zz >/dev/null 2>&1 || command -v 7z >/dev/null 2>&1 || command -v 7za >/dev/null 2>&1 || fail "7-Zip is required (for Ubuntu/Debian: sudo apt install p7zip-full)."
command -v unar >/dev/null 2>&1 || fail "unar is required for reliable RAR support (for Ubuntu/Debian: sudo apt install unar; for macOS: brew install unar)."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

say "Downloading SmartExtract…"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$REPO/archive/refs/heads/main.tar.gz" -o "$tmp/source.tar.gz"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$REPO/archive/refs/heads/main.tar.gz" -O "$tmp/source.tar.gz"
else
  fail "curl or wget is required."
fi

tar -xzf "$tmp/source.tar.gz" -C "$tmp"
say "Building an optimized binary…"
cargo build --release --manifest-path "$tmp/smartextract-main/Cargo.toml"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/smartextract-main/target/release/smartextract" "$INSTALL_DIR/smartextract"

say ""
say "✓ SmartExtract installed to $INSTALL_DIR/smartextract"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) say "Run: smartextract --help" ;;
  *) say "Add this to your shell profile, then restart your terminal:"
     say "  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac

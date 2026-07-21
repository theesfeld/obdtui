#!/bin/sh
# obdtui installer (source build path until release assets ship)
# Install from GitHub source into ~/.local/bin when cargo is available.
set -eu

REPO="theesfeld/obdtui"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
TMP="${TMPDIR:-/tmp}/obdtui-install-$$"

cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT

echo "obdtui install"
echo "Release binaries are not published yet (0.1.0-dev)."
echo "This script builds from source with cargo."

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found. Install Rust, then run again." >&2
  exit 1
fi

if ! command -v git >/dev/null 2>&1; then
  echo "error: git not found." >&2
  exit 1
fi

mkdir -p "$TMP"
git clone --depth 1 "https://github.com/${REPO}.git" "$TMP/src"
cd "$TMP/src"
cargo build --release -p obd-tui

mkdir -p "$INSTALL_DIR"
install -m 755 target/release/obdtui "$INSTALL_DIR/obdtui"

echo "Installed: $INSTALL_DIR/obdtui"
if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
  echo "Note: $INSTALL_DIR is not on PATH."
  echo "Add: export PATH=\"$INSTALL_DIR:\$PATH\""
fi

"$INSTALL_DIR/obdtui" --version || true

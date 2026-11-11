#!/usr/bin/env bash
# ============================================================================
#  Downloadification — build & install helper (Linux / macOS)
# ============================================================================
#  This is NOT the installer UI. It places the binaries on your PATH and adds
#  a desktop launcher, then opens the graphical control panel — which is where
#  you choose the folder and install the background service.
#
#  If prebuilt binaries are shipped next to this script (in ./bin), they are
#  used as-is and Rust is not required. Otherwise the source is built.
#
#  On Windows there is nothing to run by hand: download the release .zip and
#  launch `downloadification-gui.exe`. The GUI installs a Scheduled Task for
#  you (no PowerShell required).
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${DOWNLOADIFICATION_BIN_DIR:-$HOME/.local/bin}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="$CONFIG_HOME/downloadification"

LAUNCH=1
for arg in "$@"; do
    case "$arg" in
        --no-launch) LAUNCH=0 ;;
        -h|--help)
            echo "Usage: ./install.sh [--no-launch]"
            echo ""
            echo "  --no-launch   Install, but do not open the GUI."
            echo ""
            echo "Environment:"
            echo "  DOWNLOADIFICATION_BIN_DIR   Install dir (default: ~/.local/bin)"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg (try --help)" >&2
            exit 2
            ;;
    esac
done

echo "╔══════════════════════════════════════════════════════╗"
echo "║        Downloadification — build & install           ║"
echo "╚══════════════════════════════════════════════════════╝"
echo ""

# ── Locate binaries: prebuilt package first, then build from source ─────────
if [ -x "$SCRIPT_DIR/bin/downloadification" ] && [ -x "$SCRIPT_DIR/bin/downloadification-gui" ]; then
    BIN_SRC="$SCRIPT_DIR/bin"
    echo "[1/4] Using the prebuilt binaries in $BIN_SRC"
else
    if ! command -v cargo >/dev/null 2>&1; then
        echo "error: Rust and Cargo are required to build from source." >&2
        echo "       Install them from https://rustup.rs and re-run this script," >&2
        echo "       or download a release package that includes prebuilt binaries." >&2
        exit 1
    fi
    echo "[1/4] Building release binaries (this may take a few minutes)…"
    cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"
    BIN_SRC="$SCRIPT_DIR/target/release"
fi

echo "[2/4] Installing binaries to $BIN_DIR"
mkdir -p "$BIN_DIR"
install -m 0755 "$BIN_SRC/downloadification" "$BIN_DIR/downloadification"
install -m 0755 "$BIN_SRC/downloadification-gui" "$BIN_DIR/downloadification-gui"

echo "[3/4] Installing default config (existing config is preserved)"
mkdir -p "$CONFIG_DIR"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    install -m 0644 "$SCRIPT_DIR/config/config.toml" "$CONFIG_DIR/config.toml"
    echo "      created $CONFIG_DIR/config.toml"
else
    echo "      keeping $CONFIG_DIR/config.toml"
fi

echo "[4/4] Installing desktop launcher"
case "$(uname -s)" in
    Linux)
        APP_DIR="$DATA_HOME/applications"
        ICON_DIR="$DATA_HOME/icons/hicolor/scalable/apps"
        mkdir -p "$APP_DIR" "$ICON_DIR"
        install -m 0644 "$SCRIPT_DIR/assets/downloadification.desktop" "$APP_DIR/downloadification.desktop"
        install -m 0644 "$SCRIPT_DIR/assets/downloadification.svg" "$ICON_DIR/downloadification.svg"
        update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
        ;;
    Darwin)
        echo "      skipped (macOS: launch downloadification-gui from your terminal)"
        ;;
esac

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " Binaries installed:"
echo "   $BIN_DIR/downloadification"
echo "   $BIN_DIR/downloadification-gui"
echo ""
echo " Next: the graphical control panel opens now. Choose your folder and"
echo " click “Install service”. Or use the CLI:"
echo ""
echo "   downloadification status"
echo "   downloadification run"
echo "   downloadification install"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo ""
       echo "note: $BIN_DIR is not on your PATH. Add it to your shell profile." ;;
esac

if [ "$LAUNCH" -eq 1 ]; then
    if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ] || [ "$(uname -s)" = "Darwin" ]; then
        "$BIN_DIR/downloadification-gui" >/dev/null 2>&1 &
        echo ""
        echo "Launched the graphical control panel."
    else
        echo ""
        echo "No graphical session detected; start it later with: downloadification-gui"
    fi
fi
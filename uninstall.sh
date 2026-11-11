#!/usr/bin/env bash
# ============================================================================
#  Downloadification — uninstall helper (Linux / macOS)
# ============================================================================
set -euo pipefail

BIN_DIR="${DOWNLOADIFICATION_BIN_DIR:-$HOME/.local/bin}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
CONFIG_DIR="$CONFIG_HOME/downloadification"
LOG_DIR="$DATA_HOME/downloadification"

echo "╔══════════════════════════════════════════════════════╗"
echo "║             Downloadification — uninstall            ║"
echo "╚══════════════════════════════════════════════════════╝"
echo ""

echo "[1/3] Removing background service…"
if [ -x "$BIN_DIR/downloadification" ]; then
    "$BIN_DIR/downloadification" uninstall || true
else
    echo "      CLI not found; removing service files directly."
    rm -f "$CONFIG_HOME/systemd/user/downloadification.service"
    rm -f "$CONFIG_HOME/autostart/downloadification.desktop"
    rm -f "$HOME/Library/LaunchAgents/com.downloadification.agent.plist"
    command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload 2>/dev/null || true
fi

echo "[2/3] Removing binaries and launcher…"
rm -f "$BIN_DIR/downloadification" "$BIN_DIR/downloadification-gui"
rm -f "$DATA_HOME/applications/downloadification.desktop"
rm -f "$DATA_HOME/icons/hicolor/scalable/apps/downloadification.svg"
update-desktop-database "$DATA_HOME/applications" >/dev/null 2>&1 || true

echo "[3/3] Configuration and logs"
read -rp "  Remove config ($CONFIG_DIR)? [y/N] " answer
case "$answer" in
    [yY]*)
        rm -rf "$CONFIG_DIR"
        echo "      config removed"
        ;;
    *)
        echo "      config preserved"
        ;;
esac

read -rp "  Remove logs ($LOG_DIR)? [y/N] " answer
case "$answer" in
    [yY]*)
        rm -rf "$LOG_DIR"
        echo "      logs removed"
        ;;
    *)
        echo "      logs preserved"
        ;;
esac

echo ""
echo "Downloadification has been uninstalled."
#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILT_APP="$ROOT_DIR/dist/Monitor Switch.app"
INSTALL_DIR="$HOME/Applications"
INSTALLED_APP="$INSTALL_DIR/Monitor Switch.app"
LAUNCH_AGENTS_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$LAUNCH_AGENTS_DIR/app.monitorswitch.desktop.plist"
DOMAIN="gui/$(id -u)"

"$ROOT_DIR/scripts/build-macos-app.sh" >/dev/null
mkdir -p "$INSTALL_DIR"
launchctl bootout "$DOMAIN"/com.hailong.monitor-switch 2>/dev/null || true
launchctl bootout "$DOMAIN"/app.monitorswitch.desktop 2>/dev/null || true
pkill -x monitor-switch-app 2>/dev/null || true
rm -rf "$INSTALLED_APP"
ditto "$BUILT_APP" "$INSTALLED_APP"
rm -rf "$BUILT_APP"

# Remove any login item left by older installs; the app does not start at login.
rm -f "$PLIST_PATH" "$LAUNCH_AGENTS_DIR/com.hailong.monitor-switch.plist"

printf 'Monitor Switch installed to: %s\n' "$INSTALLED_APP"
printf 'Open it from Finder, Launchpad, or Spotlight. It does not start at login.\n'

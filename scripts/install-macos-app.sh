#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILT_APP="$ROOT_DIR/dist/Monitor Switch.app"
INSTALL_DIR="$HOME/Applications"
INSTALLED_APP="$INSTALL_DIR/Monitor Switch.app"
LAUNCH_AGENTS_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$LAUNCH_AGENTS_DIR/app.monitorswitch.desktop.plist"
EXECUTABLE_PATH="$INSTALLED_APP/Contents/MacOS/monitor-switch-app"
DOMAIN="gui/$(id -u)"

"$ROOT_DIR/scripts/build-macos-app.sh" >/dev/null
mkdir -p "$INSTALL_DIR" "$LAUNCH_AGENTS_DIR"
launchctl bootout "$DOMAIN"/com.hailong.monitor-switch 2>/dev/null || true
launchctl bootout "$DOMAIN"/app.monitorswitch.desktop 2>/dev/null || true
pkill -x monitor-switch-app 2>/dev/null || true
rm -rf "$INSTALLED_APP"
ditto "$BUILT_APP" "$INSTALLED_APP"
rm -rf "$BUILT_APP"

cat > "$PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>app.monitorswitch.desktop</string>
    <key>ProgramArguments</key>
    <array>
        <string>$EXECUTABLE_PATH</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
PLIST

plutil -lint "$PLIST_PATH" >/dev/null
rm -f "$LAUNCH_AGENTS_DIR/com.hailong.monitor-switch.plist"
launchctl bootstrap "$DOMAIN" "$PLIST_PATH"
launchctl kickstart -k "$DOMAIN"/app.monitorswitch.desktop

printf 'Monitor Switch installed to: %s\n' "$INSTALLED_APP"
printf 'Launch at login enabled; running in the menu bar.\n'

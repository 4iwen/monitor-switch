#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/Monitor Switch.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
ICONSET_DIR="$ROOT_DIR/dist/AppIcon.iconset"

cd "$ROOT_DIR"
cargo build --release --bin monitor-switch-app --bin monitor-switch

mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$ICONSET_DIR"
ditto "$ROOT_DIR/target/release/monitor-switch-app" "$MACOS_DIR/monitor-switch-app"
ditto "$ROOT_DIR/target/release/monitor-switch" "$RESOURCES_DIR/monitor-switch"
ditto "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"

sips -s format png "$ROOT_DIR/packaging/macos/AppIcon.svg" --out "$ROOT_DIR/dist/AppIcon-1024.png" >/dev/null
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ROOT_DIR/dist/AppIcon-1024.png" --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null
    retina=$((size * 2))
    sips -z "$retina" "$retina" "$ROOT_DIR/dist/AppIcon-1024.png" --out "$ICONSET_DIR/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/AppIcon.icns"

codesign --force --deep --sign - "$APP_DIR" >/dev/null
printf '%s\n' "$APP_DIR"

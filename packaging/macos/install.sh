#!/usr/bin/env bash
# Build Kugel.app, wire up the .kugel file association + icon, and install it.
#
#   ./packaging/macos/install.sh
#
# cargo-bundle drops the `document_type` metadata from Cargo.toml, so we
# overwrite the generated Info.plist with our canonical one that declares the
# .kugel document type and its custom UTI (icon).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

APP_SRC="target/release/bundle/osx/Kugel.app"
APP_DST="/Applications/Kugel.app"

echo "==> Building bundle"
cargo bundle --release

echo "==> Injecting .kugel document type into Info.plist"
cp "packaging/macos/Info.plist" "$APP_SRC/Contents/Info.plist"

echo "==> Installing to $APP_DST"
rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"

echo "==> Registering with Launch Services"
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister \
  -f "$APP_DST"

echo "==> Refreshing icon cache"
touch "$APP_DST"
killall Finder 2>/dev/null || true

echo "Done. .kugel files now open with Kugel and show its icon."

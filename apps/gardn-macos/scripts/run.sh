#!/bin/sh
set -eu
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
MACOS_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(CDPATH= cd -- "$MACOS_DIR/../.." && pwd)"
cd "$MACOS_DIR"
pkill -f GardnMenu.app || true
cargo build --package gardn --locked --manifest-path "$REPO_DIR/Cargo.toml"
xcodebuild \
  -project GardnMenu.xcodeproj \
  -scheme GardnMenu \
  -configuration Debug \
  -derivedDataPath "$PWD/.build/DerivedData" \
  CONFIGURATION_BUILD_DIR="$PWD/.build/Xcode" \
  build
APP="$PWD/.build/Xcode/GardnMenu.app"
cp "$REPO_DIR/target/debug/gardn" "$APP/Contents/MacOS/gardn"
chmod +x "$APP/Contents/MacOS/gardn"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" >/dev/null
open "$APP"

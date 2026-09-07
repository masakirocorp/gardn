#!/bin/sh
set -eu
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
MACOS_DIR="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_DIR="$(CDPATH= cd -- "$MACOS_DIR/../.." && pwd)"
cd "$MACOS_DIR"
pkill -f '/Gardn.app/' || true
VERSION="$(grep '^version' "$REPO_DIR/apps/gardn/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
BUNDLE_VERSION="$(python3 "$REPO_DIR/scripts/macos_app_version.py" "$VERSION")"
cargo build --package gardn --locked --manifest-path "$REPO_DIR/Cargo.toml"
export GARDN_BUNDLE_BIN="$REPO_DIR/target/debug/gardn"
xcodebuild \
  -project GardnMenu.xcodeproj \
  -scheme GardnMenu \
  -configuration Debug \
  -derivedDataPath "$PWD/.build/DerivedData" \
  CONFIGURATION_BUILD_DIR="$PWD/.build/Xcode" \
  MARKETING_VERSION="$VERSION" \
  CURRENT_PROJECT_VERSION="$BUNDLE_VERSION" \
  build
APP="$PWD/.build/Xcode/Gardn.app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" >/dev/null
open "$APP"

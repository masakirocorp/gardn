#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MACOS_DIR="$ROOT_DIR/apps/gardn-macos"
BUILD_DIR="$ROOT_DIR/build"
APP_NAME="Gardn"
VERSION="$(grep '^version' "$ROOT_DIR/apps/gardn/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')"
SIGN_IDENTITY="${SIGN_IDENTITY:-}"
NOTARIZE="${NOTARIZE:-false}"
BUNDLE_BIN="${GARDN_BUNDLE_BIN:-}"

echo "Building $APP_NAME v$VERSION..."

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

if [[ -z "$BUNDLE_BIN" ]]; then
  cargo build --package gardn --locked --release --manifest-path "$ROOT_DIR/Cargo.toml"
  BUNDLE_BIN="$ROOT_DIR/target/release/gardn"
fi

if [[ ! -x "$BUNDLE_BIN" ]]; then
  echo "error: bundled gardn binary not found: $BUNDLE_BIN" >&2
  exit 1
fi

if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "Code signing with: $SIGN_IDENTITY"
  xcodebuild -project "$MACOS_DIR/GardnMenu.xcodeproj" \
    -scheme GardnMenu \
    -configuration Release \
    -derivedDataPath "$BUILD_DIR/derived" \
    MARKETING_VERSION="$VERSION" \
    CURRENT_PROJECT_VERSION="$VERSION" \
    CODE_SIGN_IDENTITY="$SIGN_IDENTITY" \
    CODE_SIGN_STYLE=Manual \
    DEVELOPMENT_TEAM="${APPLE_TEAM_ID:-}" \
    OTHER_CODE_SIGN_FLAGS="--options=runtime"
else
  echo "Building unsigned (local)"
  xcodebuild -project "$MACOS_DIR/GardnMenu.xcodeproj" \
    -scheme GardnMenu \
    -configuration Release \
    -derivedDataPath "$BUILD_DIR/derived" \
    MARKETING_VERSION="$VERSION" \
    CURRENT_PROJECT_VERSION="$VERSION" \
    CODE_SIGN_IDENTITY="-" \
    CODE_SIGNING_REQUIRED=NO
fi

APP_PATH="$BUILD_DIR/derived/Build/Products/Release/$APP_NAME.app"
if [[ ! -d "$APP_PATH" ]]; then
  echo "error: $APP_PATH not found" >&2
  exit 1
fi

cp "$BUNDLE_BIN" "$APP_PATH/Contents/MacOS/gardn"
chmod +x "$APP_PATH/Contents/MacOS/gardn"

if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "Signing bundled gardn..."
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$APP_PATH/Contents/MacOS/gardn"
  echo "Signing app..."
  codesign --force --options runtime --entitlements "$MACOS_DIR/Gardn.entitlements" --sign "$SIGN_IDENTITY" "$APP_PATH"
fi

DMG_NAME="Gardn-$VERSION.dmg"
DMG_PATH="$BUILD_DIR/$DMG_NAME"
STAGING_DIR="$BUILD_DIR/dmg-staging"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
cp -R "$APP_PATH" "$STAGING_DIR/"

osascript <<EOF
tell application "Finder"
	make new alias file to POSIX file "/Applications" at POSIX file "$STAGING_DIR" with properties {name:"Applications"}
end tell
EOF

APPS_ICON="/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/ApplicationsFolderIcon.icns"
fileicon set "$STAGING_DIR/Applications" "$APPS_ICON"

rm -f "$DMG_PATH"
create-dmg \
  --volname "Gardn" \
  --window-pos 200 120 \
  --window-size 540 380 \
  --icon-size 100 \
  --icon "Gardn.app" 130 170 \
  --hide-extension "Gardn.app" \
  --icon "Applications" 410 170 \
  --no-internet-enable \
  "$DMG_PATH" \
  "$STAGING_DIR"

if [[ -n "$SIGN_IDENTITY" ]]; then
  echo "Signing DMG..."
  codesign --force --sign "$SIGN_IDENTITY" "$DMG_PATH"
fi

if [[ "$NOTARIZE" == "true" ]]; then
  NOTARY_ARGS=(--key "$NOTARY_KEY_PATH" --key-id "$APPLE_NOTARYTOOL_KEY_ID" --issuer "$APPLE_NOTARYTOOL_ISSUER_ID")
  echo "Notarizing DMG..."
  if ! xcrun notarytool submit "$DMG_PATH" "${NOTARY_ARGS[@]}" --wait 2>&1 | tee /tmp/notary-dmg.log; then
    SUBMISSION_ID=$(grep -o 'id: [a-f0-9-]*' /tmp/notary-dmg.log | head -1 | cut -d' ' -f2)
    if [[ -n "$SUBMISSION_ID" ]]; then
      xcrun notarytool log "$SUBMISSION_ID" "${NOTARY_ARGS[@]}" || true
    fi
    exit 1
  fi
  echo "Stapling DMG..."
  xcrun stapler staple "$DMG_PATH"
fi

echo "Done: $DMG_PATH"

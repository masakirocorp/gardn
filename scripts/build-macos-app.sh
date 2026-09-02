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
SPARKLE_TOOLS_VERSION="2.9.6"
SPARKLE_TOOLS_SHA256="52bf9e88cdd972fc0c81501377a880e90d47031bd8ca5462488f843e2609e192"

if [[ "$NOTARIZE" == "true" ]]; then
  : "${SIGN_IDENTITY:?SIGN_IDENTITY is required to notarize}"
  : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required to notarize}"
  : "${NOTARY_KEY_PATH:?NOTARY_KEY_PATH is required to notarize}"
  : "${APPLE_NOTARYTOOL_KEY_ID:?APPLE_NOTARYTOOL_KEY_ID is required to notarize}"
  : "${APPLE_NOTARYTOOL_ISSUER_ID:?APPLE_NOTARYTOOL_ISSUER_ID is required to notarize}"
fi

echo "Building $APP_NAME v$VERSION..."

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

if [[ -z "$BUNDLE_BIN" ]]; then
  cargo build --package gardn --locked --release --manifest-path "$ROOT_DIR/Cargo.toml"
  BUNDLE_BIN="$ROOT_DIR/target/release/gardn"
fi

if [[ ! -f "$BUNDLE_BIN" ]]; then
  echo "error: bundled gardn binary not found: $BUNDLE_BIN" >&2
  exit 1
fi
chmod +x "$BUNDLE_BIN"
export GARDN_BUNDLE_BIN="$BUNDLE_BIN"

XCODEBUILD=(
  xcodebuild
  -project "$MACOS_DIR/GardnMenu.xcodeproj"
  -scheme GardnMenu
  -configuration Release
  -derivedDataPath "$BUILD_DIR/derived"
  MARKETING_VERSION="$VERSION"
  CURRENT_PROJECT_VERSION="$VERSION"
)

if [[ -n "$SIGN_IDENTITY" ]]; then
  : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required to sign}"
  echo "Code signing with: $SIGN_IDENTITY"
  "${XCODEBUILD[@]}" \
    CODE_SIGN_IDENTITY="$SIGN_IDENTITY" \
    CODE_SIGN_STYLE=Manual \
    DEVELOPMENT_TEAM="$APPLE_TEAM_ID" \
    OTHER_CODE_SIGN_FLAGS="--options=runtime"
else
  echo "Building unsigned"
  "${XCODEBUILD[@]}" \
    CODE_SIGN_IDENTITY="-" \
    CODE_SIGNING_REQUIRED=NO
fi

APP_PATH="$BUILD_DIR/derived/Build/Products/Release/$APP_NAME.app"
if [[ ! -d "$APP_PATH" ]]; then
  echo "error: $APP_PATH not found" >&2
  exit 1
fi
EXTRA_BIN="$APP_PATH/Contents/MacOS/Gardn"
CLI_BIN="$APP_PATH/Contents/MacOS/gardn-cli"
if [[ ! -x "$EXTRA_BIN" ]]; then
  echo "error: $APP_PATH is missing Contents/MacOS/Gardn" >&2
  exit 1
fi
if [[ ! -x "$CLI_BIN" ]]; then
  echo "error: $APP_PATH is missing Contents/MacOS/gardn-cli" >&2
  exit 1
fi
if [[ "$EXTRA_BIN" -ef "$CLI_BIN" ]]; then
  echo "error: extra and bundled CLI collapsed to one file at $EXTRA_BIN" >&2
  exit 1
fi


if [[ -n "$SIGN_IDENTITY" ]]; then
  SPARKLE_FRAMEWORK="$APP_PATH/Contents/Frameworks/Sparkle.framework"
  if [[ ! -d "$SPARKLE_FRAMEWORK" ]]; then
    echo "error: Sparkle.framework missing from $APP_PATH" >&2
    exit 1
  fi
  echo "Signing Sparkle..."
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/B/XPCServices/Downloader.xpc"
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/B/XPCServices/Installer.xpc"
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/B/Autoupdate"
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/B/Updater.app"
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$SPARKLE_FRAMEWORK"
  echo "Signing bundled gardn..."
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$CLI_BIN"
  echo "Signing app..."
  codesign --force --options runtime --sign "$SIGN_IDENTITY" "$APP_PATH"
fi

ZIP_NAME="Gardn-$VERSION.zip"
ZIP_PATH="$BUILD_DIR/$ZIP_NAME"
ditto -c -k --keepParent "$APP_PATH" "$ZIP_PATH"

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
  echo "Notarizing ZIP..."
  if ! xcrun notarytool submit "$ZIP_PATH" "${NOTARY_ARGS[@]}" --wait 2>&1 | tee /tmp/notary-zip.log; then
    SUBMISSION_ID=$(grep -o 'id: [a-f0-9-]*' /tmp/notary-zip.log | head -1 | cut -d' ' -f2)
    if [[ -n "$SUBMISSION_ID" ]]; then
      xcrun notarytool log "$SUBMISSION_ID" "${NOTARY_ARGS[@]}" || true
    fi
    exit 1
  fi
fi

if [[ -n "${SPARKLE_KEY:-}" ]]; then
  : "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required to write appcast.xml}"
  : "${GITHUB_REF_NAME:?GITHUB_REF_NAME is required to write appcast.xml}"
  SPARKLE_DOWNLOAD_URL="https://github.com/${GITHUB_REPOSITORY}/releases/download/${GITHUB_REF_NAME}/Gardn-${VERSION}.zip"
  TOOLS_DIR="$BUILD_DIR/sparkle-tools"
  ARCHIVE="$BUILD_DIR/Sparkle-$SPARKLE_TOOLS_VERSION.tar.xz"
  mkdir -p "$TOOLS_DIR"
  curl -fsSL "https://github.com/sparkle-project/Sparkle/releases/download/$SPARKLE_TOOLS_VERSION/Sparkle-$SPARKLE_TOOLS_VERSION.tar.xz" -o "$ARCHIVE"
  ACTUAL_SHA="$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')"
  if [[ "$ACTUAL_SHA" != "$SPARKLE_TOOLS_SHA256" ]]; then
    echo "error: Sparkle tools sha256 mismatch: $ACTUAL_SHA" >&2
    exit 1
  fi
  tar xf "$ARCHIVE" -C "$TOOLS_DIR"
  KEY_FILE="$BUILD_DIR/sparkle.key"
  printf '%s\n' "$SPARKLE_KEY" > "$KEY_FILE"
  SIGN_OUTPUT="$("$TOOLS_DIR/bin/sign_update" "$ZIP_PATH" --ed-key-file "$KEY_FILE" 2>&1)"
  rm -f "$KEY_FILE"
  SIGNATURE="$(printf '%s\n' "$SIGN_OUTPUT" | grep -o 'sparkle:edSignature="[^"]*"' | cut -d'"' -f2)"
  if [[ -z "$SIGNATURE" ]]; then
    echo "error: Sparkle signature missing" >&2
    echo "$SIGN_OUTPUT" >&2
    exit 1
  fi
  ZIP_SIZE="$(stat -f%z "$ZIP_PATH")"
  cat > "$BUILD_DIR/appcast.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Gardn Updates</title>
    <item>
      <title>Version $VERSION</title>
      <sparkle:version>$VERSION</sparkle:version>
      <sparkle:shortVersionString>$VERSION</sparkle:shortVersionString>
      <pubDate>$(date -R)</pubDate>
      <enclosure
        url="$SPARKLE_DOWNLOAD_URL"
        length="$ZIP_SIZE"
        type="application/octet-stream"
        sparkle:edSignature="$SIGNATURE" />
    </item>
  </channel>
</rss>
EOF
fi

echo "Done: $DMG_PATH $ZIP_PATH"

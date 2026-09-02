#!/bin/sh
set -eu
APP="${BUILT_PRODUCTS_DIR:?}/${FULL_PRODUCT_NAME:?}"
DEST="$APP/Contents/MacOS/gardn-cli"
EXTRA="$APP/Contents/MacOS/Gardn"
DEST_NAME="$(basename "$DEST")"
EXTRA_NAME="$(basename "$EXTRA")"
if [ "$(printf '%s' "$DEST_NAME" | tr '[:upper:]' '[:lower:]')" = "$(printf '%s' "$EXTRA_NAME" | tr '[:upper:]' '[:lower:]')" ]; then
  echo "error: bundled CLI path $DEST collides with extra $EXTRA on a case-insensitive filesystem" >&2
  exit 1
fi

SRC="${GARDN_BUNDLE_BIN:-}"
ROOT="${SRCROOT:?}/../.."

if [ -z "$SRC" ] || [ ! -f "$SRC" ]; then
  if [ "${CONFIGURATION:-}" = "Release" ] && [ -f "$ROOT/target/release/gardn" ]; then
    SRC="$ROOT/target/release/gardn"
  elif [ -f "$ROOT/target/debug/gardn" ]; then
    SRC="$ROOT/target/debug/gardn"
  else
    echo "error: set GARDN_BUNDLE_BIN or build gardn first" >&2
    exit 1
  fi
fi

mkdir -p "$APP/Contents/MacOS"
cp "$SRC" "$DEST"
chmod +x "$DEST"

IDENTITY="${EXPANDED_CODE_SIGN_IDENTITY:-${CODE_SIGN_IDENTITY:-}}"
if [ -n "$IDENTITY" ] && [ "$IDENTITY" != "-" ]; then
  codesign --force --options runtime --sign "$IDENTITY" "$DEST"
fi


#!/bin/sh
set -eu
APP="${BUILT_PRODUCTS_DIR:?}/${FULL_PRODUCT_NAME:?}"
DEST="$APP/Contents/MacOS/gardn"
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

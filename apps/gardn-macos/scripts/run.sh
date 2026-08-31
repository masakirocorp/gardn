#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
swift build -c debug --product GardnMenu
BIN="$(swift build -c debug --show-bin-path)/GardnMenu"
APP="$PWD/.build/GardnMenu.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/GardnMenu"
cp Info.plist "$APP/Contents/Info.plist"
cp Assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
open "$APP"

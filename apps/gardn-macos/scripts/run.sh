#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
pkill -f GardnMenu.app || true
swift build -c debug --product GardnMenu
BIN="$(swift build -c debug --show-bin-path)/GardnMenu"
APP="$PWD/.build/GardnMenu.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/GardnMenu"
cp Info.plist "$APP/Contents/Info.plist"
python3 scripts/compile_app_icon.py "$APP/Contents/Resources"
rm -rf "$APP/Contents/Resources/AppIcon.icon"
ditto Assets/AppIcon.icon "$APP/Contents/Resources/AppIcon.icon"
codesign --force --sign - "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" >/dev/null
open "$APP"



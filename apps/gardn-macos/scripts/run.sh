#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
pkill -f GardnMenu.app || true
xcodebuild \
  -project GardnMenu.xcodeproj \
  -scheme GardnMenu \
  -configuration Debug \
  -derivedDataPath "$PWD/.build/DerivedData" \
  CONFIGURATION_BUILD_DIR="$PWD/.build/Xcode" \
  build
APP="$PWD/.build/Xcode/GardnMenu.app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP" >/dev/null
open "$APP"

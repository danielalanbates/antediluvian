#!/bin/bash
# Build "Antediluvia Launcher.app" and install to /Applications.
# Same iCloud-xattr staging dance as make_app.sh: sign an xattr-free copy.
set -euo pipefail
cd "$(dirname "$0")"

APP="../../dist/Antediluvia Launcher.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp Info.plist "$APP/Contents/"
cp ../app/AppIcon.icns "$APP/Contents/Resources/"
swiftc -O -parse-as-library -o "$APP/Contents/MacOS/AntediluviaLauncher" Launcher.swift

STAGE=$(mktemp -d)/"Antediluvia Launcher.app"
ditto --norsrc --noextattr "$APP" "$STAGE"
codesign --force --deep -s - "$STAGE"
rm -rf "/Applications/Antediluvia Launcher.app"
ditto "$STAGE" "/Applications/Antediluvia Launcher.app"
rm -rf "$(dirname "$STAGE")"
codesign -v "/Applications/Antediluvia Launcher.app"
echo "Installed /Applications/Antediluvia Launcher.app"

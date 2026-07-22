#!/bin/bash
# Build Antediluvia.app and install it to /Applications.
#
# The bundle carries the release server + Bevy client + assets. Its launcher
# (scripts/app/launcher.sh) starts a local server (DB in ~/Library/Application
# Support/Antediluvia) unless one is already listening on 8787, runs the
# client as $USER, and stops the server it started when the client exits.
#
# Signing gotcha: ~/Documents is iCloud-synced, so the bundle keeps acquiring
# FinderInfo/fileprovider xattrs that make codesign fail with "detritus not
# allowed". We therefore stage an xattr-free copy in a temp dir and sign there.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"

cargo build --release -j 4 -p antediluvia-server -p antediluvia-client-bevy

APP=dist/Antediluvia.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp scripts/app/Info.plist "$APP/Contents/"
cp scripts/app/launcher.sh "$APP/Contents/MacOS/Antediluvia"
chmod +x "$APP/Contents/MacOS/Antediluvia"
cp scripts/app/AppIcon.icns "$APP/Contents/Resources/"
cp target/release/antediluvia-server target/release/antediluvia-client-bevy \
   "$APP/Contents/Resources/"

# Sign in with Apple helper (real SIWA when the bundle is provisioned with the
# applesignin entitlement; stable local UUID fallback otherwise).
swiftc -O -o "$APP/Contents/Resources/apple-signin" scripts/app/AppleSignIn.swift
cp -R assets "$APP/Contents/Resources/assets"

STAGE=$(mktemp -d)/Antediluvia.app
ditto --norsrc --noextattr "$APP" "$STAGE"
codesign --force --deep -s - "$STAGE"
rm -rf /Applications/Antediluvia.app
ditto "$STAGE" /Applications/Antediluvia.app
rm -rf "$(dirname "$STAGE")"
codesign -v /Applications/Antediluvia.app
echo "Installed /Applications/Antediluvia.app"

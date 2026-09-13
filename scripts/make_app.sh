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
# Bundle assets, but exclude the heavy gathered CC0 sets not yet referenced
# (photoscan models + HDRIs) so the app stays small. The grass PBR textures
# (textures/pbr) ARE used by the terrain, so they're kept.
rsync -a --exclude 'models/polyhaven' --exclude 'hdri' assets/ "$APP/Contents/Resources/assets/"

# NO_INSTALL=1 builds + signs dist/Antediluvia.app only (e.g. while the game
# is running from /Applications). Otherwise the installed copy is archived to
# ~/Downloads/Antediluvia-archived/ before being replaced.
if [ "${NO_INSTALL:-}" = "1" ]; then
  find "$APP" -exec xattr -c {} +  2>/dev/null || true
  codesign --force --deep -s - "$APP"
  echo "Built $APP (not installed)"
  exit 0
fi

STAGE=$(mktemp -d)/Antediluvia.app
ditto --norsrc --noextattr "$APP" "$STAGE"
codesign --force --deep -s - "$STAGE"
if [ -d /Applications/Antediluvia.app ]; then
  OLD=$(defaults read /Applications/Antediluvia.app/Contents/Info.plist CFBundleShortVersionString 2>/dev/null || echo old)
  mkdir -p "$HOME/Downloads/Antediluvia-archived"
  mv /Applications/Antediluvia.app "$HOME/Downloads/Antediluvia-archived/Antediluvia-$OLD-$(date +%Y%m%d%H%M).app"
fi
ditto "$STAGE" /Applications/Antediluvia.app
rm -rf "$(dirname "$STAGE")"
codesign -v /Applications/Antediluvia.app
echo "Installed /Applications/Antediluvia.app"

#!/usr/bin/env bash
# Build CleanMeBro.app (and a zip of it) from this repo. macOS only.
#
#   scripts/bundle-mac.sh              # native arch
#   UNIVERSAL=1 scripts/bundle-mac.sh  # Apple silicon + Intel in one binary
#   SIGN_ID=... APP_NAME=... BUNDLE_ID=... PROFILE=debug override the defaults
#
# Output: target/bundle/CleanMeBro.app and target/bundle/CleanMeBro-<version>-macos.zip
set -euo pipefail

cd "$(dirname "$0")/.."
APP_NAME="${APP_NAME:-CleanMeBro}"
BIN="cleanmebro"
BUNDLE_ID="${BUNDLE_ID:-com.romanfurman6.cleanmebro}"
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
OUT="target/bundle"
APP="$OUT/$APP_NAME.app"

if [[ "${UNIVERSAL:-0}" == "1" ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/release
  lipo -create -output "target/release/$BIN" \
    "target/aarch64-apple-darwin/release/$BIN" \
    "target/x86_64-apple-darwin/release/$BIN"
elif [[ "${PROFILE:-release}" == "debug" ]]; then
  cargo build
else
  cargo build --release
fi
BUILD_DIR="target/${PROFILE:-release}"
[[ "${UNIVERSAL:-0}" == "1" ]] && BUILD_DIR="target/release"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BUILD_DIR/$BIN" "$APP/Contents/MacOS/$BIN"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <!-- Menu bar app: no Dock icon until the full window opens. -->
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>CleanMeBro asks Finder to pick a folder for saved captures, and System Events for the apps that open at login.</string>
  <key>NSMicrophoneUsageDescription</key><string>CleanMeBro records your voice in screen recordings when you turn the microphone on.</string>
  <key>NSCameraUsageDescription</key><string>CleanMeBro shows your camera in screen recordings when you turn the camera on.</string>
</dict>
</plist>
PLIST

# Ad-hoc signature by default so Apple silicon runs it; SIGN_ID signs with a
# real identity instead. Not notarized.
codesign --force --deep --sign "${SIGN_ID:--}" "$APP"

ZIP="$OUT/$APP_NAME-$VERSION-macos.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
echo "Built $APP"
echo "Zipped $ZIP"

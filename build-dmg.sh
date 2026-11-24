#!/bin/bash
# Builds "Whatsapp by TagLinc.app" and packages it into dist/Whatsapp-by-TagLinc.dmg
set -euo pipefail
cd "$(dirname "$0")"

APP_NAME="Whatsapp by TagLinc"
DMG_NAME="Whatsapp-by-TagLinc.dmg"
# Keep this bundle id: WebKit stores every account's login under it.
BIN="multi-whatsapp"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
DIST="dist"
APP="$DIST/$APP_NAME.app"

# Universal binary (Apple Silicon + Intel) via rustup's stable toolchain
RUSTUP="$(command -v rustup || echo "$(brew --prefix rustup)/bin/rustup")"
CARGO_BIN="$("$RUSTUP" which --toolchain stable cargo)"
export RUSTC="$("$RUSTUP" which --toolchain stable rustc)"
export MACOSX_DEPLOYMENT_TARGET=14.0
for T in aarch64-apple-darwin x86_64-apple-darwin; do
  "$CARGO_BIN" build --release --target "$T"
done

rm -rf "$DIST" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/$BIN" \
  "target/aarch64-apple-darwin/release/$BIN" "target/x86_64-apple-darwin/release/$BIN"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIdentifier</key><string>local.multiwhatsapp.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Voice messages and calls in WhatsApp.</string>
  <key>NSCameraUsageDescription</key><string>Video calls and photos in WhatsApp.</string>
</dict></plist>
EOF

# Simple generated icon (green rounded square) — skipped silently if tools are missing
if [ -f assets/AppIcon.icns ]; then
  cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
fi

codesign --force --deep --sign - "$APP"

STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp "INSTALL.txt" "$STAGE/How to install.txt"
cp assets/DmgIcon.icns "$STAGE/.VolumeIcon.icns"

# Build writable image, flag the volume as having a custom icon, then compress.
RW="$(mktemp -d)/rw.dmg"
MNT="$(mktemp -d)"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDRW "$RW" >/dev/null
hdiutil attach -nobrowse -noautoopen -mountpoint "$MNT" "$RW" >/dev/null
SetFile -a C "$MNT"
hdiutil detach "$MNT" >/dev/null
hdiutil convert "$RW" -format UDZO -ov -o "$DIST/$DMG_NAME" >/dev/null
rm -rf "$STAGE" "$(dirname "$RW")"

# Icon on the .dmg file itself (Finder shows it; some upload/chat services strip it)
swift scripts/set-icon.swift assets/DmgIcon.icns "$DIST/$DMG_NAME"

echo "Done: $DIST/$DMG_NAME ($(du -h "$DIST/$DMG_NAME" | cut -f1))"

#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION="1.0.0"
APP="dist/NotOffice.app"
rm -rf "$APP"
cargo build --release
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/notoffice "$APP/Contents/MacOS/notoffice"
chmod +x "$APP/Contents/MacOS/notoffice"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>NotOffice</string>
  <key>CFBundleDisplayName</key><string>NotOffice</string>
  <key>CFBundleIdentifier</key><string>local.notoffice</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>notoffice</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP" || true
fi
if command -v hdiutil >/dev/null 2>&1; then
  hdiutil create -volname "NotOffice" -srcfolder "$APP" \
    -ov -format UDZO "dist/NotOffice-${VERSION}.dmg"
fi
echo "Built $APP"

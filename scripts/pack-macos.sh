#!/usr/bin/env bash
# 打包 macOS .app bundle —— 仅用于本地测试(非正式交付物)
# .app 内含 CFBundleURLTypes,用于验证 x-notify:// 协议拉起流程
# 用法: scripts/pack-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="x-notify-service"
BUNDLE_ID="com.hexinfo.x-notify-service"
OUT="dist/$APP_NAME.app"

echo "==> cargo build --release"
cargo build --release

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "target/release/x-notify-service" "$OUT/Contents/MacOS/"
cp assets/icons/icon.icns "$OUT/Contents/Resources/AppIcon.icns"

cat > "$OUT/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>x-notify-service</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>$BUNDLE_ID</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>x-notify</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

echo "==> 测试 bundle: $OUT"
echo "    注册协议:   $OUT/Contents/MacOS/x-notify-service protocol register"
echo "    直接运行:   $OUT/Contents/MacOS/x-notify-service install"
echo "    验证拉起:   open 'x-notify://launch'"

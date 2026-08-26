#!/usr/bin/env bash
# 打包 Windows 交付物(正式):NSIS 安装器 setup.exe(per-user 免 UAC、可选安装目录、LZMA 压缩)
# 用法: scripts/pack-windows.sh [target]
#   默认 target: x86_64-pc-windows-gnu(交叉编译,需 mingw-w64;macOS: brew install mingw-w64 nsis)
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${1:-x86_64-pc-windows-gnu}"
VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/version = "\(.*\)"/\1/')"
STAGE="dist/.stage-windows"

echo "==> cargo build --release --target $TARGET"
cargo build --release --target "$TARGET"

# 构建 JSSDK 与演示页(安装器内一并分发)
echo "==> pnpm build sdk"
(cd sdk/js && pnpm install --silent && pnpm -F @hexinfo/x-notify-service-sdk build)

rm -rf "$STAGE"
mkdir -p "$STAGE" dist
cp "target/$TARGET/release/x-notify-service.exe" "$STAGE/"
cp scripts/templates/config.toml "$STAGE/config.toml"
cp sdk/js/packages/sdk/dist/x-notify-service-sdk.js "$STAGE/sdk.js"
cp assets/sdk-使用手册.md "$STAGE/sdk-manual.md"
cp assets/demo.html "$STAGE/demo.html"

echo "==> makensis"
makensis -NOCD -DSTAGE="$STAGE" -DVERSION="$VERSION" scripts/pack-windows.nsi
rm -rf "$STAGE"
echo "==> 产出: dist/x-notify-service-setup-$VERSION.exe"
ls -lh "dist/x-notify-service-setup-$VERSION.exe" | awk '{print $5, $9}'

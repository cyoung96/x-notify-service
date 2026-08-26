#!/usr/bin/env bash
# 从 assets/icon.svg 单源再生全部图标位图:hicolor PNG(Linux)/ICO(Windows)/ICNS(macOS)
#
# 用法: scripts/gen-icons.sh
# 依赖: rsvg-convert(brew librsvg)、iconutil(macOS 自带)、python3
set -euo pipefail
cd "$(dirname "$0")/.."

SVG=assets/icon.svg
HICOLOR=assets/icons/hicolor
ICO=assets/icons/x-notify-service.ico
ICNS=assets/icons/icon.icns
WORK=$(mktemp -d /tmp/x-notify-icons.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

render() { # $1=size $2=output
    rsvg-convert -w "$1" -h "$1" "$SVG" -o "$2"
}

# Linux hicolor 全档(128/48/32/16 另被 build.rs 烘进 _NET_WM_ICON)
for size in 16 32 48 64 128 256 512; do
    render "$size" "$HICOLOR/x-notify-service-$size.png"
done

# Windows ICO:16-256 全档 PNG 条目(24 等非 hicolor 档进临时目录)
ICO_SIZES=(16 24 32 48 64 128 256)
for size in "${ICO_SIZES[@]}"; do
    render "$size" "$WORK/ico-$size.png"
done
python3 - "$WORK" "${ICO_SIZES[@]}" <<'EOF'
# ICO 组装:ICONDIR + ICONDIRENTRY×N + PNG 负载(宽度字节 0 表示 256)
import pathlib
import struct
import sys

work = pathlib.Path(sys.argv[1])
sizes = [int(s) for s in sys.argv[2:]]
payloads = [work.joinpath(f"ico-{s}.png").read_bytes() for s in sizes]
header = struct.pack("<HHH", 0, 1, len(sizes))
entries = bytearray()
offset = 6 + 16 * len(sizes)
for size, payload in zip(sizes, payloads):
    side = 0 if size >= 256 else size
    entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(payload), offset)
    offset += len(payload)
out = work.joinpath("assembled.ico")
out.write_bytes(header + bytes(entries) + b"".join(payloads))
EOF
mv "$WORK/assembled.ico" "$ICO"

# macOS ICNS:iconset 命名约定(@2x 为双倍档;目录须带 .iconset 后缀)
ICONSET="$WORK/app.iconset"
mkdir -p "$ICONSET"
for spec in "16:icon_16x16" "32:icon_16x16@2x" "32:icon_32x32" "64:icon_32x32@2x" \
    "128:icon_128x128" "256:icon_128x128@2x" "256:icon_256x256" "512:icon_256x256@2x" \
    "512:icon_512x512" "1024:icon_512x512@2x"; do
    size="${spec%%:*}"
    render "$size" "$ICONSET/${spec#*:}.png"
done
iconutil -c icns "$ICONSET" -o "$ICNS"

ls -l "$HICOLOR" "$ICO" "$ICNS"

#!/bin/sh
# x-notify-service Linux(UOS/麒麟)安装脚本 —— 用户级安装,无需 root
# 注册逻辑由二进制自身完成(x-notify-service install),本脚本只做文件放置
set -e

SRC_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$HOME/.local/bin"
CONF_DIR="$HOME/.config/x-notify-service"

mkdir -p "$BIN_DIR" "$CONF_DIR"

cp "$SRC_DIR/bin/x-notify-service" "$BIN_DIR/x-notify-service"
chmod +x "$BIN_DIR/x-notify-service"

if [ ! -f "$CONF_DIR/config.toml" ]; then
    cp "$SRC_DIR/config/config.toml" "$CONF_DIR/config.toml"
    echo "已生成配置: $CONF_DIR/config.toml"
fi

# 图标安装到用户级 hicolor(供 desktop 文件引用)
if [ -d "$SRC_DIR/icons/hicolor" ]; then
    for png in "$SRC_DIR"/icons/hicolor/x-notify-service-*.png; do
        [ -f "$png" ] || continue
        size="$(basename "$png" .png | sed 's/.*-//')"
        mkdir -p "$HOME/.local/share/icons/hicolor/${size}x${size}/apps"
        cp "$png" "$HOME/.local/share/icons/hicolor/${size}x${size}/apps/x-notify-service.png"
    done
fi
# 演示页与 SDK 放置到用户数据目录
mkdir -p "$HOME/.local/share/x-notify-service"
cp "$SRC_DIR/demo.html" "$SRC_DIR/sdk.js" "$HOME/.local/share/x-notify-service/"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "提示: 建议将 $BIN_DIR 加入 PATH" ;;
esac

"$BIN_DIR/x-notify-service" install

echo "安装完成:开机自启 + x-notify:// 协议已注册,服务已启动"
echo "卸载: $BIN_DIR/x-notify-service uninstall"

#!/usr/bin/env bash
# Xvfb 无头 GUI 冒烟:真实创建/显示/关闭弹窗(Slint+winit+软件渲染+fontconfig 全栈)。
# 带 Xfwm4:真实 WM 会参与窗口摆放与 _NET_WORKAREA 维护,
# 断言弹窗最终落在工作区右下角(对抗 WM 重摆的复校机制是否生效)。
set -euo pipefail
cd "$(dirname "$0")/../.."

BIN=./target/release/x-notify-service
API=http://127.0.0.1:17320
DISPLAY_ID=:99

Xvfb "$DISPLAY_ID" -screen 0 1280x800x24 &
XVFB_PID=$!
trap 'kill $XVFB_PID 2>/dev/null; pkill -f x-notify-service 2>/dev/null || true' EXIT
sleep 1
# 服务/WM/xdotool 统一连接该 DISPLAY(不 export 的话 xdotool 会连默认 :0 而查不到窗口)
export DISPLAY="$DISPLAY_ID"

# 真实 WM:Xfwm4 会把新窗口按自己的摆放策略放置(如级联/左上角),
# 并维护 _NET_WORKAREA(无面板时 = 0,0,1280,800)
if command -v xfwm4 >/dev/null 2>&1; then
    DISPLAY="$DISPLAY_ID" xfwm4 >/dev/null 2>&1 &
    sleep 1
    echo "PASS: Xfwm4 已启动(真实 WM 环境)"
else
    echo "SKIP: 无 xfwm4,退化为无 WM 冒烟(定位断言跳过)"
fi

DISPLAY="$DISPLAY_ID" "$BIN" >/tmp/xns-gui.log 2>&1 &
for _ in $(seq 1 50); do curl -sf "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done

VIA=$(curl -s -X POST "$API/notify" -d '{"title":"GUI 冒烟","body":"Xvfb 真实渲染弹窗"}' | jq -r .via)
[ "$VIA" = "popup" ] || { echo "FAIL: 应 via=popup 实际 $VIA"; cat /tmp/xns-gui.log; exit 1; }
echo "PASS: via=popup(GUI 栈端到端可用)"

if grep -q "弹窗显示失败" /tmp/xns-gui.log 2>/dev/null; then
    echo "FAIL: 服务日志存在弹窗失败记录"; exit 1
fi

# 窗口层断言(xdotool 按窗口标题查询;标题不匹配则降级为仅 via 断言)
sleep 1
if command -v xdotool >/dev/null 2>&1; then
    N=$(xdotool search --name x-notify-service 2>/dev/null | wc -l | tr -d ' ' || true)
    if [ "$N" -ge 1 ]; then
        echo "PASS: 弹窗窗口已映射($N)"

        # 定位断言:无面板时工作区=全屏,期望右下角 (1280-367-14, 800-206-14)=(899,580)
        if pgrep -x xfwm4 >/dev/null 2>&1; then
            WIN=$(xdotool search --name x-notify-service | head -1)
            read -r WX WY < <(xdotool getwindowgeometry --shell "$WIN" | awk '/^X=/{x=$2} /^Y=/{y=$2} END{print x, y}')
            DX=$((WX - 899)); if [ $DX -lt 0 ]; then DX=$((-DX)); fi
            DY=$((WY - 580)); if [ $DY -lt 0 ]; then DY=$((-DY)); fi
            echo "弹窗实际位置: ($WX,$WY),期望: (899,580),偏差: (${DX},${DY})"
            if [ "$DX" -le 6 ] && [ "$DY" -le 6 ]; then
                echo "PASS: 弹窗落在工作区右下角(WM 重摆复校生效)"
            else
                echo "FAIL: 弹窗未在右下角,复校机制未生效"; cat /tmp/xns-gui.log; exit 1
            fi
        fi

        curl -s -X POST "$API/close" >/dev/null
        sleep 1
        N2=$(xdotool search --name x-notify-service 2>/dev/null | wc -l | tr -d ' ' || true)
        [ "$N2" = "0" ] && echo "PASS: /close 后窗口已隐藏" || { echo "FAIL: close 后窗口仍在($N2)"; exit 1; }
        # 第二条通知顶替路径
        curl -s -X POST "$API/notify" -d '{"title":"第二条","body":"顶替"}' >/dev/null
        sleep 1
        N3=$(xdotool search --name x-notify-service 2>/dev/null | wc -l | tr -d ' ' || true)
        [ "$N3" = "1" ] && echo "PASS: 新通知顶替后仍恰一个窗口" || { echo "FAIL: 顶替后窗口数 $N3"; exit 1; }
    else
        echo "SKIP: 按标题未查到弹窗窗口,仅保留 via=popup 断言"
    fi
fi

echo "== GUI 冒烟通过 =="

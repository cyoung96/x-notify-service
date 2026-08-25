#!/usr/bin/env bash
# 集成测试:HTTP API(身份/校验/CORS/close)+ 进程语义(单例静默退出/端口向后探测)
# 以 --no-popup 启动(无桌面环境可跑;弹窗渲染属 GUI,不在本脚本范围)
set -euo pipefail
cd "$(dirname "$0")/../.."

BIN=./target/release/x-notify-service
[ -x "$BIN" ] || BIN=./target/debug/x-notify-service
API=http://127.0.0.1:17320

# jq 断言;断言失败时打印上下文
assert() { # $1=描述 $2=实际 $3=期望
    if [ "$2" = "$3" ]; then
        echo "PASS: $1"
    else
        echo "FAIL: $1(实际: $2, 期望: $3)"; exit 1
    fi
}

cleanup() { pkill -f 'x-notify-service' 2>/dev/null || true; }
trap cleanup EXIT
cleanup; sleep 0.5

# ---------- 场景1:常规启动 ----------
"$BIN" --no-popup >/dev/null 2>&1 &
for _ in $(seq 1 50); do curl -sf "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done

H=$(curl -s "$API/health")
assert "health 应用身份" "$(echo "$H" | jq -r .app)" "x-notify-service"

V=$(curl -s -X POST "$API/notify" -d '{"title":"集成","body":"内容"}' | jq -r .via)
assert "无桌面时降级 via=system" "$V" "system"

S=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$API/notify" -d '{"title":"  "}')
assert "空标题 422" "$S" "422"
S=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$API/notify" -d '{bad')
assert "坏 JSON 400" "$S" "400"
S=$(curl -s -o /dev/null -w '%{http_code}' "$API/nope")
assert "未知路径 404" "$S" "404"
S=$(curl -s -o /dev/null -w '%{http_code}' -X OPTIONS "$API/notify")
assert "OPTIONS 预检 204" "$S" "204"

CAO=$(curl -s -i "$API/health" | grep -i '^access-control-allow-origin' | tr -d '\r' | awk '{print $2}')
assert "CORS Allow-Origin" "$CAO" "*"
CPN=$(curl -s -I -X OPTIONS "$API/notify" | grep -i '^access-control-allow-private-network' | tr -d '\r' | awk '{print $2}')
assert "CORS Private-Network" "$CPN" "true"

S=$(curl -s -X POST "$API/close")
assert "close 幂等 ok" "$(echo "$S" | jq -r .ok)" "true"

# ---------- 场景2:单例静默退出(第二实例退出后原实例仍应答)----------
"$BIN" >/dev/null 2>&1
RC=$?
assert "二次启动静默退出 exit=0" "$RC" "0"
ALIVE=$(curl -s -o /dev/null -w '%{http_code}' "$API/health")
assert "原实例仍在服务" "$ALIVE" "200"

# ---------- 场景3:端口向后探测 ----------
pkill -f 'x-notify-service'; sleep 0.5
python3 -c "
import socket, subprocess, time, json, urllib.request
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', 17320)); s.listen(1)   # 占住默认端口
p = subprocess.Popen(['$BIN', '--no-popup'], stdout=subprocess.DEVNULL)
for _ in range(50):
    time.sleep(0.2)
    try:
        h = json.loads(urllib.request.urlopen('http://127.0.0.1:17321/health', timeout=1).read())
        break
    except Exception:
        pass
assert h['port'] == 17321, f'应报实际端口 17321,实际 {h[\"port\"]}'
p.terminate(); p.wait()
print('PASS: 默认端口被占时落到 17321 且 /health 报实际端口')
"
pkill -f 'x-notify-service' 2>/dev/null || true

echo "== 集成测试全部通过 =="

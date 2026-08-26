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
"$BIN" --no-popup serve >/dev/null 2>&1 &
for _ in $(seq 1 50); do curl -sf "$API/health" >/dev/null 2>&1 && break; sleep 0.2; done

H=$(curl -s "$API/health")
DEMO=$(curl -s http://127.0.0.1:17320/ | head -c 200)
echo "$DEMO" | grep -q "<!doctype html>" && echo "PASS: 内嵌演示页可访问" || { echo "FAIL: 演示页缺失"; exit 1; }
SDK_BODY=$(curl -s http://127.0.0.1:17320/sdk.js)
echo "$SDK_BODY" | grep -q "createNotifyService" && echo "PASS: 内嵌 sdk.js 为真产物" || { echo "FAIL: 内嵌 sdk.js 是占位桩(打包顺序错误)"; exit 1; }
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
"$BIN" serve >/dev/null 2>&1
RC=$?
assert "二次启动静默退出 exit=0" "$RC" "0"
ALIVE=$(curl -s -o /dev/null -w '%{http_code}' "$API/health")
assert "原实例仍在服务" "$ALIVE" "200"

# ---------- 场景2.5:info 诊断命令(只读,headless 下应仍可运行)----------
INFO_OUT=$("$BIN" info 2>/dev/null)
assert "info 退出码 0" "$?" "0"
echo "$INFO_OUT" | grep -q "工作区" && echo "PASS: info 输出含工作区诊断" || { echo "FAIL: info 输出缺工作区"; exit 1; }

# ---------- 场景3:端口向后探测 ----------
pkill -f 'x-notify-service'; sleep 0.5
python3 -c "
import socket, subprocess, time, json, urllib.request
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(('127.0.0.1', 17320)); s.listen(1)   # 占住默认端口
p = subprocess.Popen(['$BIN', '--no-popup', 'serve'], stdout=subprocess.DEVNULL)
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

# ---------- 场景4:CLI 生命周期(start/stop/restart/close,均幂等)----------
"$BIN" stop >/dev/null 2>&1; assert "stop 未运行时幂等 exit=0" "$?" "0"
"$BIN" start >/dev/null 2>&1; assert "start exit=0" "$?" "0"
ALIVE=$(curl -s -o /dev/null -w '%{http_code}' "$API/health"); assert "start 后 health 200" "$ALIVE" "200"
"$BIN" start >/dev/null 2>&1; assert "start 重复执行幂等 exit=0" "$?" "0"
"$BIN" restart >/dev/null 2>&1; assert "restart exit=0" "$?" "0"
ALIVE=$(curl -s -o /dev/null -w '%{http_code}' "$API/health"); assert "restart 后 health 200" "$ALIVE" "200"
"$BIN" close >/dev/null 2>&1; assert "close 幂等 exit=0" "$?" "0"
"$BIN" stop >/dev/null 2>&1; assert "stop exit=0" "$?" "0"
ALIVE=$(curl -s -o /dev/null -w '%{http_code}' "$API/health" || true)
assert "stop 后 health 不通" "$ALIVE" "000"
"$BIN" start >/dev/null 2>&1 || true   # 收尾:留给后续场景的服务态

# ---------- 场景5:安全参数(token 鉴权 + CORS 白名单;默认不配置=全开放无鉴权)----------
pkill -f 'x-notify-service' 2>/dev/null || true; sleep 0.5
cat > /tmp/xns-sec.toml <<'SEC'
port = 17444
token = "s3cret"
cors_origins = ["http://good.example"]
SEC
"$BIN" --config /tmp/xns-sec.toml --no-popup serve >/dev/null 2>&1 &
SEC_SVC=$!
for _ in $(seq 1 30); do curl -sf http://127.0.0.1:17444/health >/dev/null 2>&1 && break; sleep 0.2; done
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:17444/notify -d '{"title":"t"}' || echo 000)
assert "无 token 被拒 401" "$CODE" "401"
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:17444/notify -H "X-Token: s3cret" -d '{"title":"t"}' || echo 000)
assert "带 token 放行 200" "$CODE" "200"
ORIGIN=$(curl -s -i -X OPTIONS http://127.0.0.1:17444/notify -H "Origin: http://good.example" \
    -H "Access-Control-Request-Method: POST" | grep -i '^access-control-allow-origin' | awk '{print $2}' | tr -d '\r')
assert "白名单 Origin 回显" "$ORIGIN" "http://good.example"
N=$(curl -s -i -X OPTIONS http://127.0.0.1:17444/notify -H "Origin: http://evil.example" \
    -H "Access-Control-Request-Method: POST" | grep -ci '^access-control-allow-origin' || true)
assert "陌生 Origin 无 Allow-Origin" "$N" "0"
kill $SEC_SVC 2>/dev/null || true

echo "== 集成测试全部通过 =="

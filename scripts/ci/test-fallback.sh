#!/usr/bin/env bash
# 系统通知兜底路径端到端测试(在 dbus-run-session 内执行):
# 1. 起 mock 通知守护进程占住 org.freedesktop.Notifications
# 2. 以 --no-popup 启动服务(强制兜底通道)
# 3. POST /notify → 断言响应 via=system 且 mock 守护进程真实收到内容
set -euo pipefail
cd "$(dirname "$0")/../.."

OUT=/tmp/notify-captured.jsonl
rm -f "$OUT"

python3 scripts/ci/mock-notify-daemon.py "$OUT" &
MOCK_PID=$!
sleep 1
# 预检:mock 必须已占住总线名,否则测试无意义
dbus-send --session --print-reply --dest=org.freedesktop.DBus / org.freedesktop.DBus.ListNames \
    | grep -q org.freedesktop.Notifications || { echo "mock 未占住总线名"; exit 1; }

./target/release/x-notify-service --no-popup serve &
SVC_PID=$!
trap 'kill $MOCK_PID $SVC_PID 2>/dev/null || true' EXIT

# 等服务就绪(默认端口 17320)
for _ in $(seq 1 50); do
    curl -sf http://127.0.0.1:17320/health >/dev/null 2>&1 && break
    sleep 0.2
done

VIA=$(curl -s -X POST http://127.0.0.1:17320/notify \
    -d '{"title":"兜底端到端","body":"<b>加粗</b>与<font color=\"#d93025\">红色</font>应剥为纯文本"}' \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["via"])')
echo "via=$VIA"

python3 - "$VIA" "$OUT" <<'EOF'
import json, sys

via, out = sys.argv[1], sys.argv[2]
assert via == "system", f"应走系统通知,实际 via={via}"
lines = [json.loads(x) for x in open(out, encoding="utf-8")]
assert lines, "mock 守护进程未收到任何通知"
n = lines[-1]
assert n["summary"] == "兜底端到端", n
assert "加粗" in n["body"] and "<b>" not in n["body"], f"HTML 未剥除: {n['body']}"
assert "红色" in n["body"], n
print("PASS: 系统通知兜底真实送达,HTML 已剥为纯文本")
EOF

# CLI notify -f:强制兜底,应真实送达 mock 守护进程且退出码 0
./target/release/x-notify-service notify -t "CLI 兜底手测" -f
RC=$?
[ "$RC" = "0" ] || { echo "FAIL: notify -f 退出码 $RC"; exit 1; }
tail -1 "$OUT" | grep -q "CLI 兜底手测" && echo "PASS: notify -f 经 CLI 直发系统通知" || { echo "FAIL: mock 未收到 CLI 通知"; exit 1; }

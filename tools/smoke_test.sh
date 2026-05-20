#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
SERVER_PORT=9090
PASS=0
FAIL=0

cleanup() {
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

check() {
    local desc="$1" expected="$2" actual="$3"
    if echo "$actual" | grep -q "$expected"; then
        echo "  PASS $desc"
        PASS=$((PASS + 1))
    else
        echo "  FAIL $desc"
        echo "    expected: '$expected'"
        echo "    got:      '$actual'"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== EVSE API Smoke Tests ==="

echo "[1/5] Building server..."
cd "$PROJECT_DIR"
cargo build --release 2>&1 | tail -1

echo "[2/5] Starting server on port $SERVER_PORT..."
EVSE_API_PORT=$SERVER_PORT "$PROJECT_DIR/target/release/evse-api-server" &
SERVER_PID=$!
sleep 2

for i in $(seq 1 15); do
    curl -s "http://localhost:$SERVER_PORT/api/v1/health" > /dev/null 2>&1 && break
    sleep 0.5
done

echo "[3/5] HTTP health checks..."
HEALTH=$(curl -s "http://localhost:$SERVER_PORT/api/v1/health" || echo "FAIL")
check "GET /api/v1/health" "ok" "$HEALTH"

STATUS=$(curl -s "http://localhost:$SERVER_PORT/api/v1/status" || echo "FAIL")
check "GET /api/v1/status" "sessions" "$STATUS"

NOTFOUND=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$SERVER_PORT/api/v1/nonexistent")
check "GET /api/v1/nonexistent" "404" "$NOTFOUND"

echo "[4/5] WebSocket smoke test..."
WS_OUT=$(mktemp)
timeout 5 websocat -t "ws://localhost:$SERVER_PORT/ws" < /dev/null > "$WS_OUT" 2>/dev/null || true
WS_RESULT=$(cat "$WS_OUT" 2>/dev/null || echo "EMPTY")
check "WebSocket connects" "status" "$WS_RESULT"
check "WebSocket session_id" "session_id" "$WS_RESULT"
rm -f "$WS_OUT"

echo "[5/5] Stopping server..."
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL SMOKE TESTS PASSED"
exit $FAIL

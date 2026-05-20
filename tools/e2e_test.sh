#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."
SERVER_PORT=9091
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

echo "=== E2E Integration Test ==="

echo "[1/6] Building..."
cd "$PROJECT_DIR"
cargo build --release 2>&1 | tail -1

echo "[2/6] Starting server on port $SERVER_PORT..."
EVSE_API_PORT=$SERVER_PORT "$PROJECT_DIR/target/release/evse-api-server" &
SERVER_PID=$!
sleep 2

for i in $(seq 1 15); do
    curl -s "http://localhost:$SERVER_PORT/api/v1/health" > /dev/null 2>&1 && break
    sleep 0.5
done

echo "[3/6] Health check..."
HEALTH=$(curl -s "http://localhost:$SERVER_PORT/api/v1/health" || echo "FAIL")
check "server healthy" "ok" "$HEALTH"

echo "[4/6] WebSocket connect + receive status..."
WS_OUT=$(mktemp)
timeout 5 websocat -t "ws://localhost:$SERVER_PORT/ws" < /dev/null > "$WS_OUT" 2>/dev/null || true
WS=$(cat "$WS_OUT" 2>/dev/null || echo "EMPTY")
check "WS status event" '"type":"status"' "$WS"
check "WS connected message" "connected" "$WS"
check "WS session_id present" "session_id" "$WS"
rm -f "$WS_OUT"

echo "[5/6] Control event round-trip..."
CTRL_OUT=$(mktemp)
(
    echo '{"type":"control_event","event":{"kind":"AuthorizationResponse","authorized":true}}'
    sleep 2
) | timeout 5 websocat -t "ws://localhost:$SERVER_PORT/ws" > "$CTRL_OUT" 2>/dev/null || true
CTRL=$(cat "$CTRL_OUT" 2>/dev/null || echo "EMPTY")
check "WS accepts control event" '"type":"status"' "$CTRL"
rm -f "$CTRL_OUT"

echo "[6/6] Stopping server..."
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true

echo ""
echo "E2E Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && echo "ALL E2E TESTS PASSED"
exit $FAIL

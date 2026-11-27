#!/bin/sh
# 친구와 함께 시험할 수 있게 터널과 서버를 켜고, 친구에게 보낼 https 링크를 출력합니다. Ctrl+C로 서버와 터널을 함께 끕니다.
#
# 순서: 도구 확인 -> 포트 확인 -> 웹 클라이언트와 서버를 릴리스로 빌드 -> 포트 확인 -> 터널 켜기(주소를 받음) -> 그 주소를 알려 주며 서버 켜기 -> 링크 출력
# 터널을 먼저 켜는 이유: cloudflared의 빠른 터널은 켜진 뒤에야 주소를 출력하고, 서버는 그 주소를 시작할 때 받아야 합니다.
# 터널을 켜기 전에 포트가 비어 있는지 확인하므로, 터널이 이 컴퓨터의 다른 프로그램을 공개하는 일이 없습니다.
#
# 실행: sh scripts/playtest.sh
# 환경 변수: PLAYTEST_PORT(서버의 포트, 기본 7777), CARGO_TARGET_DIR(빌드의 출력 폴더, 기본 target)
# 출력: <빌드의 출력 폴더>/playtest/web(웹 빌드), server.log, tunnel.log. 웹 빌드의 폴더는 빌드할 때마다 비워지므로 다른 파일을 두지 않습니다.
#
# 이 스크립트를 kill -9로 끝내면 터널과 서버를 끄지 못합니다. 그때는 cloudflared와 caravan-dedicated의 프로세스를 직접 끄십시오.

set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
PORT=${PLAYTEST_PORT:-7777}
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}
case $TARGET in
/*) ;;
*) TARGET=$ROOT/$TARGET ;;
esac
export CARGO_TARGET_DIR="$TARGET"
OUT=$TARGET/playtest
WEB=$OUT/web

SERVER_PID=
TUNNEL_PID=

# 프로세스가 살아 있는지 봅니다. 끝났지만 아직 거두지 않은 프로세스(Z)는 끝난 것으로 봅니다.
alive() {
    [ -n "$1" ] || return 1
    state=$(ps -o stat= -p "$1" 2>/dev/null) || return 1
    [ -n "$state" ] && [ "${state#Z}" = "$state" ]
}

# 나갈 때(정상, 오류, Ctrl+C) 서버와 터널을 모두 끕니다. 5초 안에 끝나지 않으면 강제로 끝냅니다.
# shellcheck disable=SC2329 # 아래의 trap이 부릅니다.
cleanup() {
    trap - EXIT INT TERM HUP
    for pid in "$SERVER_PID" "$TUNNEL_PID"; do
        if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
    done
    waited=0
    while [ "$waited" -lt 50 ] && { alive "$SERVER_PID" || alive "$TUNNEL_PID"; }; do
        sleep 0.1
        waited=$((waited + 1))
    done
    for pid in "$SERVER_PID" "$TUNNEL_PID"; do
        if [ -n "$pid" ]; then
            kill -9 "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
}
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP
trap cleanup EXIT

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "$1 is not installed. $2" >&2
        exit 1
    fi
}

# 포트에서 듣는 프로그램이 있으면 알리고 끝냅니다.
require_free_port() {
    if nc -z 127.0.0.1 "$PORT" >/dev/null 2>&1; then
        echo "port $PORT is already in use. Stop that program, or choose another port: PLAYTEST_PORT=7778 sh scripts/playtest.sh" >&2
        exit 1
    fi
}

# 아무것도 켜거나 빌드하기 전에 필요한 프로그램을 모두 확인합니다.
require cloudflared "Install it with: brew install cloudflared"
require trunk "Install it with: cargo install trunk --locked"
require cargo "Install Rust from https://rustup.rs"
require curl "Install curl with your package manager."
require nc "Install netcat with your package manager."
require_free_port

cd "$ROOT"
mkdir -p "$OUT"

echo "building the web client (release) ..."
(cd crates/client && trunk build --release --locked --dist "$WEB")
echo "building the server (release) ..."
cargo build --release --locked -p caravan_dedicated

# 빌드하는 동안 다른 프로그램이 포트를 쓰기 시작했을 수 있으므로, 터널을 켜기 직전에 다시 확인합니다.
require_free_port

echo "tunnel:  starting cloudflared ..."
cloudflared tunnel --no-autoupdate --url "http://127.0.0.1:$PORT" >"$OUT/tunnel.log" 2>&1 &
TUNNEL_PID=$!

# 터널이 출력하는 주소를 읽습니다. 오류 문구에 나오는 api.trycloudflare.com은 터널의 주소가 아닙니다.
URL=
tries=0
while [ -z "$URL" ]; do
    if ! alive "$TUNNEL_PID"; then
        echo "cloudflared stopped. Its output (more in $OUT/tunnel.log):" >&2
        tail -n 15 "$OUT/tunnel.log" >&2
        exit 1
    fi
    if [ "$tries" -ge 300 ]; then
        echo "cloudflared gave no address in 60 seconds. See $OUT/tunnel.log" >&2
        exit 1
    fi
    URL=$(grep -Eo 'https://[a-z0-9-]+\.trycloudflare\.com' "$OUT/tunnel.log" | grep -v '^https://api\.' | head -n 1)
    if [ -z "$URL" ]; then
        sleep 0.2
        tries=$((tries + 1))
    fi
done

"$TARGET/release/caravan-dedicated" --addr "127.0.0.1:$PORT" --web-dir "$WEB" --public-url "$URL" >"$OUT/server.log" 2>&1 &
SERVER_PID=$!

tries=0
until curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:$PORT/" 2>/dev/null; do
    if ! alive "$SERVER_PID" || [ "$tries" -ge 100 ]; then
        echo "the server did not start. Its output (more in $OUT/server.log):" >&2
        tail -n 15 "$OUT/server.log" >&2
        exit 1
    fi
    sleep 0.1
    tries=$((tries + 1))
done
echo "server:  http://127.0.0.1:$PORT/"

echo
echo "  Send this link to your friends (it may take a few seconds before it opens):"
echo
echo "    $URL/"
echo
echo "  Play yourself at http://127.0.0.1:$PORT/ . The invite you copy there is the link above."
echo "  Press Ctrl+C to stop the server and the tunnel."

while alive "$SERVER_PID" && alive "$TUNNEL_PID"; do
    sleep 1
done
echo "the server or the tunnel stopped. See $OUT/server.log and $OUT/tunnel.log" >&2
exit 1

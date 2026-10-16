#!/bin/sh
# One scenario against the real Rust backend on a throwaway root: no real device files
# (nothing on the LAN is touched), the built web app served by the backend. On localhost:
# passkeys need a secure context and a domain (never an IP). Accounts come from invitations
# made with `maison-backend invite`, as on the Pi (lib.ts `invitation`).
# Needs: `cargo build --manifest-path backend/Cargo.toml` and `cd web && bun run build` first.
set -eu
cd "$(dirname "$0")"
ROOT=$(cd .. && pwd)
DIR=$(mktemp -d)
PORT=${PORT:-3099}
cp -R "$ROOT/cache" "$DIR/cache"
export MAISON_SOURCE_ROOT="$DIR" PUBLIC_URL="http://localhost:$PORT" MAISON_BIN="$ROOT/backend/target/debug/maison-backend"
FRONTEND_DIST_DIR="$ROOT/web/build" PORT="$PORT" HOST=127.0.0.1 \
	JWT_SECRET=e2e-secret-0123456789abcdef0123456789 AUTH_COOKIE_SECURE=false DISABLE_BLUETOOTH=true \
	ZIGBEE_SERIAL_PORT=/dev/null-e2e MATTER_STATE_DIR="$DIR/matter" MATTER_TRUST_DIR="$ROOT/matter-trust" \
	RUST_LOG=warn "$ROOT/backend/target/debug/maison-backend" > "$DIR/server.log" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; rm -rf "$DIR"' EXIT
# up within 10 s, or the run stops here, saying why: never a scenario against nothing
up=false
for _ in $(seq 50); do
	if curl -sf "http://localhost:$PORT/health" >/dev/null; then
		up=true
		break
	fi
	kill -0 $SERVER 2>/dev/null || break
	sleep 0.2
done
if [ "$up" != true ]; then
	echo "e2e: the backend did not answer /health on port $PORT; its log:" >&2
	cat "$DIR/server.log" >&2
	exit 1
fi
mkdir -p shots
BASE="http://localhost:$PORT" SHOTS=shots bun "${1:-smoke.ts}"

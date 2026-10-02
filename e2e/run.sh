#!/bin/sh
# One scenario against the real Rust backend on a throwaway root: a test account, no real
# device files (nothing on the LAN is touched), the built web app served by the backend.
# Needs: `cargo build --manifest-path backend/Cargo.toml` and `cd web && bun run build` first.
set -e
cd "$(dirname "$0")"
ROOT=$(cd .. && pwd)
DIR=$(mktemp -d)
PORT=${PORT:-3099}
HASH=$("$ROOT/backend/target/debug/hash_password" 'e2e-password')
printf '[{"id":"1","username":"e2e","password_hash":"%s","role":"admin"}]\n' "$HASH" > "$DIR/users.json"
for f in devices.json meross-devices.json hue-lamps.json hue-lamps-blacklist.json zigbee-lamps.json \
	zigbee-lamps-blacklist.json broadlink-codes.json; do echo '[]' > "$DIR/$f"; done
echo '{}' > "$DIR/device-cache.json"
cp -R "$ROOT/cache" "$DIR/cache"
MAISON_SOURCE_ROOT="$DIR" FRONTEND_DIST_DIR="$ROOT/web/build" PORT="$PORT" HOST=127.0.0.1 \
	JWT_SECRET=e2e-secret-0123456789abcdef0123456789 AUTH_COOKIE_SECURE=false DISABLE_BLUETOOTH=true \
	ZIGBEE_SERIAL_PORT=/dev/null-e2e MATTER_STATE_DIR="$DIR/matter" MATTER_TRUST_DIR="$ROOT/matter-trust" \
	RUST_LOG=warn "$ROOT/backend/target/debug/maison-backend" > "$DIR/server.log" 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null; rm -rf "$DIR"' EXIT
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/health" >/dev/null && break; sleep 0.2; done
mkdir -p shots
BASE="http://127.0.0.1:$PORT" SHOTS=shots bun "${1:-smoke.ts}"

#!/usr/bin/env bash
# Maison onto a host, over SSH: one static binary, the web bundle, the settings. The host is
# anything that runs a Linux binary and has sh, tar and wget (BusyBox's do); what it must
# provide itself (the service's account, a supervisor, its logs) is docs/deploy.md's « The host ».
set -euo pipefail

ROOT_DIR="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

# This house's deploy settings, never pushed (deploy.env.example)
if [ -f "${ROOT_DIR}/deploy.env" ]; then
  # shellcheck source=/dev/null
  . "${ROOT_DIR}/deploy.env"
fi

DEPLOY_HOST="${DEPLOY_HOST:-}"
APP_DIR="${APP_DIR:-/opt/maison}"
SERVICE_USER="${SERVICE_USER:-maison}"
SERVICE_GROUP="${SERVICE_GROUP:-${SERVICE_USER}}"
ENV_FILE="${ENV_FILE:-${ROOT_DIR}/.env}"
# The backend for the host: BACKEND_BUILD is the command that makes BACKEND_BIN; without one,
# cargo-zigbuild for BACKEND_TARGET (a target rustup ships)
BACKEND_TARGET="${BACKEND_TARGET:-}"
BACKEND_BUILD="${BACKEND_BUILD:-}"
BACKEND_BIN="${BACKEND_BIN:-${ROOT_DIR}/backend/target/${BACKEND_TARGET}/release/maison-backend}"
# The edge's certificates for this host (`maison-backend edge init`'s first directory), when
# EDGE_LISTEN is set
EDGE_HOUSE_DIR="${EDGE_HOUSE_DIR:-}"

# Pushed from this machine on every deploy (the source of truth is local).
RUNTIME_FILES=(
  devices.json
  meross-devices.json
)

# Mutable state owned by the backend on the host: never pushed, never seeded (a missing file is
# the backend's default, an empty one an error), handed back to the service user when present.
# The backend creates them itself: APP_DIR is root:maison 1775 (group-writable, sticky like
# /tmp), so the service makes and renames its own files but can never replace root's (.env,
# devices.json…).
STATE_FILES=(
  device-cache.json
  broadlink-codes.json
  hue-lamps.json
  hue-lamps-blacklist.json
  zigbee-lamps.json
  zigbee-lamps-blacklist.json
  climate-state.json
  nabaztag.json
  refresh-tokens.json
  scenes.json
  tv.json
  androidtv.json
  adb-key
  atv-identity
)

# State directories handed recursively to the service user, "name|mode" (an empty mode leaves
# permissions alone). matter/ holds the fabric keys: the controller saves by temp file + rename,
# so it needs a private directory of its own rather than a pre-created file.
STATE_DIRS=(
  'cache|'
  'matter|700'
  'auth|700'
  'photos|700'
)

# The host's layout, relative to APP_DIR.
REMOTE_BACKEND_DIR=backend/target/release
REMOTE_WEB_DIR=web/build
REMOTE_LAYOUT=(
  "${REMOTE_BACKEND_DIR}"
  "${REMOTE_WEB_DIR}"
  deploy/mosquitto
  matter-trust
  cache/tempo
)

usage() {
  cat <<'EOF'
Usage:
  ./deploy.sh [all|build|push|start|stop|logs|status]

Commands:
  all      build, push, start
  build    Build the web bundle and the backend for the host
  push     Push the binary, the bundle and the settings to the host
  start    Restart the stack on the host and wait for /health
  stop     Stop the stack on the host until the next start
  logs     Follow the host's logs: stack (default), backend, mosquitto
  status   The host's own account of its services

Settings (deploy.env, or the environment):
  DEPLOY_HOST      SSH target, by key (root@<host>)
  BACKEND_TARGET   Rust target triple, built with cargo-zigbuild (RUSTFLAGS is passed on)
  BACKEND_BUILD    a command that builds the backend instead, run from the repository's root
  BACKEND_BIN      the binary it makes, default backend/target/<BACKEND_TARGET>/release/maison-backend
  EDGE_HOUSE_DIR   the edge's certificates for this host (maison-backend edge init)
  APP_DIR          the host's directory, default /opt/maison
  SERVICE_USER     the service's account on the host, default maison (SERVICE_GROUP alike)
  ENV_FILE         the env file pushed as the host's .env, default ./.env
EOF
}

log() {
  printf '\n==> %s\n' "$*"
}

warn() {
  printf 'Warning: %s\n' "$*" >&2
}

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

run_local() {
  log "$*"
  "$@"
}

on_host() {
  if [ -z "${DEPLOY_HOST}" ]; then
    fail 'Missing DEPLOY_HOST (deploy.env): the SSH target, e.g. root@<host>'
  fi
  # shellcheck disable=SC2029 # the command line is built here, on purpose
  ssh "${DEPLOY_HOST}" "$@"
}

# Run the script read on stdin on the host, as root, with APP_DIR, SERVICE_USER and
# SERVICE_GROUP set and "$@" as its remaining arguments. The host may run BusyBox sh: no
# bash-isms in a remote script, and lists arrive as positional arguments. ssh hands the remote
# shell a single command line, hence the quoting.
host_script() {
  local args
  args="$(printf ' %q' "${APP_DIR}" "${SERVICE_USER}" "${SERVICE_GROUP}" "$@")"
  {
    cat <<'EOF'
set -eu
# shellcheck disable=SC2034 # not every script uses all three
APP_DIR="$1" SERVICE_USER="$2" SERVICE_GROUP="$3"
shift 3
EOF
    cat
  } | on_host "sh -s --${args}"
}

# The one place that knows how the host's state is laid out: directories, ownership. Idempotent.
# Files come as root, so ownership is handed back after every push, otherwise the backend gets
# EACCES on its next save.
lay_out_state() {
  host_script "${REMOTE_LAYOUT[*]}" "${STATE_DIRS[*]}" "${STATE_FILES[@]}" <<'EOF'
LAYOUT="$1"
STATE_DIRS="$2"
shift 2

for dir in ${LAYOUT}; do
  mkdir -p "${APP_DIR}/${dir}"
done

# The service writes its state by temp file + rename next to it: the app dir is
# group-writable for it, sticky so nobody replaces a file they do not own.
chown "root:${SERVICE_GROUP}" "${APP_DIR}"
chmod 1775 "${APP_DIR}"

for entry in ${STATE_DIRS}; do
  dir="${APP_DIR}/${entry%%|*}"
  mode="${entry#*|}"
  mkdir -p "${dir}"
  chown -R "${SERVICE_USER}:${SERVICE_GROUP}" "${dir}"
  if [ -n "${mode}" ]; then
    chmod "${mode}" "${dir}"
  fi
done

for name in "$@"; do
  file="${APP_DIR}/${name}"
  if [ -e "${file}" ]; then
    chown "${SERVICE_USER}:${SERVICE_GROUP}" "${file}"
  fi
done
EOF
}

# A setting of the local env file (the last one wins), quotes around it dropped.
env_value() {
  local value
  value="$(grep -E "^$1=" "${ENV_FILE}" 2>/dev/null | tail -n1 | cut -d'=' -f2-)" || true
  value="${value%\"}"
  value="${value#\"}"
  value="${value%\'}"
  value="${value#\'}"
  printf '%s' "${value}"
}

# The Meross plugs' addresses (meross-devices.json), one IPv4 address a line: what the host's
# firewall lets reach Mosquitto's 8883. Anything but a private IPv4 address stops the deploy.
plug_addresses() {
  python3 -I - "${ROOT_DIR}/meross-devices.json" <<'PY'
import ipaddress, json, sys
for device in json.load(open(sys.argv[1])):
    address = ipaddress.ip_address(str(device["ip"]).strip())
    if address.version != 4 or not address.is_private:
        sys.exit(f"meross-devices.json: {address} is not a private IPv4 address")
    print(address)
PY
}

# put_file <local file> <remote path> <owner:group> <mode>: replaced whole (a temp file, renamed)
put_file() {
  on_host "set -eu; t=\"$2.new\"; mkdir -p \"\$(dirname \"$2\")\"; cat >\"\${t}\"; chown $3 \"\${t}\"; chmod $4 \"\${t}\"; mv \"\${t}\" \"$2\"" <"$1"
}

# put_tree <local dir> <remote dir>: the tree replaced whole (unpacked aside, then swapped)
put_tree() {
  COPYFILE_DISABLE=1 tar -C "$1" --no-xattrs -cf - . |
    on_host "set -eu; d=\"$2\"; rm -rf \"\${d}.new\"; mkdir -p \"\${d}.new\"; tar -C \"\${d}.new\" -xf -; rm -rf \"\${d}.old\"; if [ -e \"\${d}\" ]; then mv \"\${d}\" \"\${d}.old\"; fi; mv \"\${d}.new\" \"\${d}\"; rm -rf \"\${d}.old\"; chown -R root:root \"\${d}\""
}

build() {
  run_local bun install --cwd "${ROOT_DIR}/web" --frozen-lockfile
  run_local bun run --cwd "${ROOT_DIR}/web" build
  if [ -n "${BACKEND_BUILD}" ]; then
    log "${BACKEND_BUILD}"
    (cd "${ROOT_DIR}" && bash -c "${BACKEND_BUILD}")
  elif [ -n "${BACKEND_TARGET}" ]; then
    # --no-default-features: no Bluetooth (BlueZ's libraries are not cross-built)
    run_local cargo zigbuild --release --locked --no-default-features \
      --manifest-path "${ROOT_DIR}/backend/Cargo.toml" --target "${BACKEND_TARGET}" --bin maison-backend
  else
    fail 'Missing BACKEND_TARGET or BACKEND_BUILD (deploy.env): what the host runs'
  fi
}

push() {
  if [ ! -f "${BACKEND_BIN}" ]; then
    fail "Missing backend artifact: ${BACKEND_BIN}. Run ./deploy.sh build first."
  fi
  if [ ! -d "${ROOT_DIR}/${REMOTE_WEB_DIR}" ]; then
    fail "Missing ${REMOTE_WEB_DIR}. Run ./deploy.sh build first."
  fi

  log "Preparing ${APP_DIR}"
  lay_out_state

  log "Pushing backend artifact"
  put_file "${BACKEND_BIN}" "${APP_DIR}/${REMOTE_BACKEND_DIR}/maison-backend" root:root 755
  log "Pushing web bundle"
  put_tree "${ROOT_DIR}/${REMOTE_WEB_DIR}" "${APP_DIR}/${REMOTE_WEB_DIR}"

  # Secrets (.env holds JWT_SECRET; devices.json the Tuya local keys, meross-devices.json the
  # Meross key): root's, readable by the service's group only
  if [ -f "${ENV_FILE}" ]; then
    log "Pushing env file"
    put_file "${ENV_FILE}" "${APP_DIR}/.env" "root:${SERVICE_GROUP}" 640
  else
    warn "Env file not found at ${ENV_FILE}; keeping the host's .env untouched"
  fi
  local relative_path
  for relative_path in "${RUNTIME_FILES[@]}"; do
    if [ -f "${ROOT_DIR}/${relative_path}" ]; then
      log "Pushing ${relative_path}"
      put_file "${ROOT_DIR}/${relative_path}" "${APP_DIR}/${relative_path}" "root:${SERVICE_GROUP}" 640
    fi
  done

  # The edge's certificates (edge.rs): the service's own directory, its key 0600
  if [ -n "${EDGE_HOUSE_DIR}" ]; then
    log "Pushing the edge's certificates"
    local f
    for f in ca.pem cert.pem; do
      put_file "${EDGE_HOUSE_DIR}/${f}" "${APP_DIR}/edge/${f}" "${SERVICE_USER}:${SERVICE_GROUP}" 644
    done
    put_file "${EDGE_HOUSE_DIR}/key.pem" "${APP_DIR}/edge/key.pem" "${SERVICE_USER}:${SERVICE_GROUP}" 600
  elif [ -n "$(env_value EDGE_LISTEN)" ]; then
    warn "EDGE_LISTEN is set but EDGE_HOUSE_DIR is not: the host keeps the edge certificates it has"
  fi

  # Meross plugs only: their firmware wants a broker to boot
  if [ -f "${ROOT_DIR}/meross-devices.json" ]; then
    log "Pushing Mosquitto's configuration, certificates and the plugs' addresses (its firewall)"
    local scratch
    scratch="$(mktemp -d)"
    # shellcheck disable=SC2064 # the directory is known now
    trap "rm -rf '${scratch}'" EXIT
    put_file "${ROOT_DIR}/deploy/mosquitto/maison.conf" /etc/mosquitto/maison.conf root:root 644
    if [ -d "${ROOT_DIR}/mosquitto/certs" ]; then
      # The CA private key never needs to leave this machine.
      for f in ca.pem server.pem; do
        put_file "${ROOT_DIR}/mosquitto/certs/${f}" "/etc/mosquitto/certs/maison/${f}" mosquitto:mosquitto 644
      done
      put_file "${ROOT_DIR}/mosquitto/certs/server-key.pem" /etc/mosquitto/certs/maison/server-key.pem mosquitto:mosquitto 600
    else
      warn "mosquitto/certs is missing locally; the TLS listener cannot start"
    fi
    plug_addresses >"${scratch}/plugs.allow"
    put_file "${scratch}/plugs.allow" "${APP_DIR}/deploy/mosquitto/plugs.allow" root:root 644
  fi

  log "Pushing Matter attestation roots"
  put_tree "${ROOT_DIR}/matter-trust" "${APP_DIR}/matter-trust"

  if [ -d "${ROOT_DIR}/cache/tempo" ]; then
    log "Pushing the Tempo cache"
    # What the host has learnt since (seasons, netload.json, weather.json) is newer and stays;
    # model.json is fitted here (fit_tempo) and versioned: always this machine's.
    COPYFILE_DISABLE=1 tar -C "${ROOT_DIR}/cache/tempo" --no-xattrs -cf - . |
      on_host "set -eu; d=${APP_DIR}/cache/tempo; s=\$(mktemp -d); tar -C \"\${s}\" -xf -; mkdir -p \"\${d}\"; for f in \"\${s}\"/*; do n=\"\${d}/\${f##*/}\"; if [ ! -e \"\${n}\" ] || [ \"\${f##*/}\" = model.json ] || [ \"\${f}\" -nt \"\${n}\" ]; then cp \"\${f}\" \"\${n}\"; fi; done; rm -rf \"\${s}\""
  fi

  log "Restoring mutable-state ownership"
  lay_out_state
}

start() {
  log "Restarting the stack on the host"
  host_script <<'EOF'
rm -f "${APP_DIR}/stopped"
# the host's supervisor starts them again at once, from the files just pushed
killall maison-backend mosquitto 2>/dev/null || true
port="$(grep -E '^PORT=' "${APP_DIR}/.env" | tail -n1 | cut -d'=' -f2-)" || true
n=0
until wget -q -O - "http://127.0.0.1:${port:-3033}/health" >/dev/null 2>&1; do
  n=$((n + 1))
  if [ "${n}" -ge 60 ]; then
    printf '%s\n' 'maison: no /health after 60 s (./deploy.sh logs)' >&2
    exit 1
  fi
  sleep 1
done
printf 'maison: healthy after %s s\n' "${n}"
EOF
}

stop() {
  log "Stopping the stack on the host"
  host_script <<'EOF'
# the host's supervisor starts nothing while this file exists
touch "${APP_DIR}/stopped"
killall maison-backend mosquitto 2>/dev/null || true
printf '%s\n' 'maison, mosquitto: stopped (./deploy.sh start to restart)'
EOF
}

COMMAND="${1:-all}"

case "${COMMAND}" in
  all)
    build
    push
    start
    ;;
  build) build ;;
  push) push ;;
  start) start ;;
  stop) stop ;;
  logs)
    log "Following ${2:-stack} logs on the host"
    on_host "maison-logs $(printf '%q' "${2:-stack}")"
    ;;
  status) on_host maison-status ;;
  help | -h | --help) usage ;;
  *)
    printf 'Unknown command: %s\n\n' "${COMMAND}" >&2
    usage >&2
    exit 1
    ;;
esac

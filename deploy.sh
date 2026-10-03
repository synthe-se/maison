#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PI_HOST="${PI_HOST:-}"
PI_PASSWORD="${PI_PASSWORD:-}"
PI_APP_DIR="${PI_APP_DIR:-/opt/maison}"
PI_SERVICE_USER="${PI_SERVICE_USER:-maison}"
PI_SERVICE_GROUP="${PI_SERVICE_GROUP:-${PI_SERVICE_USER}}"
PI_ENV_FILE="${PI_ENV_FILE:-${ROOT_DIR}/.env}"
BACKEND_TARGET="${BACKEND_TARGET:-arm-unknown-linux-musleabihf}"
BACKEND_BIN="${ROOT_DIR}/backend/target/${BACKEND_TARGET}/release/maison-backend"
CLOUDFLARED_BIN="${CLOUDFLARED_BIN:-${ROOT_DIR}/cloudflared-arm}"

# Files pushed from this machine on every deploy (the source of truth is local).
RUNTIME_FILES=(
  devices.json
  meross-devices.json
)

# Mutable state owned by the backend on the Pi: never pushed, never seeded (a
# missing file is the backend's default, an empty one an error), handed back to
# the service user when present. The backend creates them itself: APP_DIR is
# root:maison 1775 (group-writable, sticky like /tmp), so the service makes and
# renames its own files but can never replace root's (.env, devices.json…).
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
  ir-keymap.json
  tv.json
  androidtv.json
  adb-key
  atv-identity
)

# State directories handed recursively to the service user. Format:
# "name|mode"; an empty mode leaves permissions alone. matter/ holds the
# fabric keys: the controller saves by temp-file + rename, so it needs a
# private directory of its own rather than a pre-created file.
STATE_DIRS=(
  'cache|'
  'matter|700'
  'auth|700'
)

# Remote layout, relative to PI_APP_DIR.
REMOTE_BACKEND_DIR=backend/target/release
REMOTE_WEB_DIR=web/build
REMOTE_LAYOUT=(
  "${REMOTE_BACKEND_DIR}"
  "${REMOTE_WEB_DIR}"
  deploy/openrc
  deploy/mosquitto
  mosquitto/certs
  matter-trust
  cache/tempo
)

usage() {
  cat <<'EOF'
Usage:
  PI_HOST=pi@raspberrypi ./deploy.sh [all|build|push|upgrade|start|stop|logs|status]

Commands:
  all      Build locally, push to the Pi, upgrade host services, restart everything
  build    Build the web frontend and cross-build the backend only
  push     Push artifacts and configs to the Pi only
  upgrade  Install or upgrade host-native dependencies on the Pi
  start    Install service definitions and restart the stack on the Pi
  stop     Stop the running stack on the Pi
  logs     Follow logs for one service or the full stack
  status   Show service status, dependency hints, and final URLs

Environment:
  PI_HOST          SSH target, required for push/upgrade/start/all/logs/status
  PI_PASSWORD      Optional SSH password used via sshpass when installed locally
  PI_APP_DIR       Remote app directory, default /opt/maison
  PI_SERVICE_USER  Service user on the Pi, default maison
  PI_SERVICE_GROUP Service group on the Pi, default maison
  PI_ENV_FILE      Local env file to deploy, default ./.env
  BACKEND_TARGET   Rust target triple, default arm-unknown-linux-musleabihf
  CLOUDFLARED_BIN  Path to cross-compiled cloudflared binary, default ./cloudflared-arm

Examples:
  PI_HOST=pi@192.168.1.50 ./deploy.sh all
  PI_HOST=pi@192.168.1.50 ./deploy.sh push
  PI_HOST=pi@192.168.1.50 ./deploy.sh logs backend
  PI_HOST=pi@192.168.1.50 ./deploy.sh status
EOF
}

log() {
  printf '\n==> %s\n' "$*"
}

warn() {
  printf 'Warning: %s\n' "$*" >&2
}

require_host() {
  if [ -z "${PI_HOST}" ]; then
    printf '%s\n' 'Missing PI_HOST. Example: PI_HOST=pi@192.168.1.50 ./deploy.sh all' >&2
    exit 1
  fi
}

ssh_base_cmd() {
  if [ -n "${PI_PASSWORD}" ]; then
    if ! command -v sshpass >/dev/null 2>&1; then
      printf '%s\n' 'PI_PASSWORD is set but sshpass is not installed. Install it first, for example: brew install hudochenkov/sshpass/sshpass' >&2
      exit 1
    fi
    SSHPASS="${PI_PASSWORD}" sshpass -e "$@"
  else
    "$@"
  fi
}

run_local() {
  log "$*"
  "$@"
}

ssh_pi() {
  ssh_base_cmd ssh "${PI_HOST}" "$@"
}

rsync_pi() {
  ssh_base_cmd rsync "$@"
}

# Prologue of every remote script. The Pi runs BusyBox sh: no bash-isms
# below this line, and lists arrive as positional arguments.
remote_prologue() {
  cat <<'EOF'
set -eu
# shellcheck disable=SC2034 # not every script uses all three
APP_DIR="$1" SERVICE_USER="$2" SERVICE_GROUP="$3"
shift 3
EOF
}

# Run the script read on stdin on the Pi, as root, with APP_DIR,
# SERVICE_USER and SERVICE_GROUP set and "$@" as its remaining arguments.
# ssh hands the remote shell a single command line, hence the quoting.
ssh_pi_script() {
  require_host
  local args
  args="$(printf ' %q' "${PI_APP_DIR}" "${PI_SERVICE_USER}" "${PI_SERVICE_GROUP}" "$@")"
  { remote_prologue; cat; } | ssh_pi "sh -s --${args}"
}

# The one place that knows how the Pi's state is laid out: service account,
# directory layout, seeded state files, ownership. Idempotent.
# Arguments: LAYOUT, STATE_DIRS (space-separated), then one "name|seed" per
# state file.
remote_state_script() {
  cat <<'EOF'
LAYOUT="$1"
STATE_DIRS="$2"
shift 2

if ! getent group "${SERVICE_GROUP}" >/dev/null 2>&1; then
  addgroup -S "${SERVICE_GROUP}"
fi
if ! id -u "${SERVICE_USER}" >/dev/null 2>&1; then
  adduser -S -D -H -h "${APP_DIR}" -G "${SERVICE_GROUP}" -s /sbin/nologin "${SERVICE_USER}"
fi
# The backend drives the Zigbee dongle over /dev/ttyUSB0 (root:dialout).
if ! id -Gn "${SERVICE_USER}" | grep -qw dialout; then
  addgroup "${SERVICE_USER}" dialout
fi

for dir in ${LAYOUT}; do
  mkdir -p "${APP_DIR}/${dir}"
done

# The service writes its state by temp file + rename next to it: the app dir is
# group-writable for it, sticky so nobody replaces a file they do not own.
chown "root:${SERVICE_GROUP}" "${APP_DIR}"
chmod 1775 "${APP_DIR}"

# rsync runs as root, so ownership is handed back after every push —
# otherwise the backend gets EACCES on its next persist.
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

# Run the state step on the Pi, followed by the script read on stdin (if any).
ssh_pi_with_state() {
  { remote_state_script; cat; } |
    ssh_pi_script "${REMOTE_LAYOUT[*]}" "${STATE_DIRS[*]}" "${STATE_FILES[@]}"
}

build_local() {
  run_local bun install --cwd "${ROOT_DIR}/web" --frozen-lockfile
  run_local bun run --cwd "${ROOT_DIR}/web" build
  run_local env TARGET="${BACKEND_TARGET}" bash "${ROOT_DIR}/scripts/build-rpi1-backend.sh"
}

prepare_remote_push() {
  log "Preparing remote host for file sync"
  ssh_pi_with_state <<'EOF'
apk add --no-cache rsync
# The React frontend (frontend/dist) was replaced by the SvelteKit bundle in
# web/build: drop the stale copy so the Pi carries a single frontend.
rm -rf "${APP_DIR}/frontend"
EOF
}

fix_state_ownership() {
  log "Restoring mutable-state ownership"
  ssh_pi_with_state </dev/null
}

push_to_pi() {
  require_host

  if [ ! -f "${BACKEND_BIN}" ]; then
    printf 'Missing backend artifact: %s\n' "${BACKEND_BIN}" >&2
    printf '%s\n' 'Run ./deploy.sh build first.' >&2
    exit 1
  fi

  if [ ! -d "${ROOT_DIR}/${REMOTE_WEB_DIR}" ]; then
    printf 'Missing %s. Run ./deploy.sh build first.\n' "${REMOTE_WEB_DIR}" >&2
    exit 1
  fi

  prepare_remote_push

  log "Pushing backend artifact"
  rsync_pi -avz "${BACKEND_BIN}" "${PI_HOST}:${PI_APP_DIR}/${REMOTE_BACKEND_DIR}/"

  log "Pushing web bundle"
  rsync_pi -avz "${ROOT_DIR}/${REMOTE_WEB_DIR}/" "${PI_HOST}:${PI_APP_DIR}/${REMOTE_WEB_DIR}/"

  # Secrets (.env holds JWT_SECRET and the tunnel token; devices.json the Tuya local keys,
  # meross-devices.json the Meross key): root's, readable by the service's group only.
  local secret=(--chown="root:${PI_SERVICE_GROUP}" --chmod=F640)

  if [ -f "${PI_ENV_FILE}" ]; then
    log "Pushing env file"
    rsync_pi -avz "${secret[@]}" "${PI_ENV_FILE}" "${PI_HOST}:${PI_APP_DIR}/.env"
  else
    warn "Env file not found at ${PI_ENV_FILE}; keeping remote .env untouched"
  fi

  log "Pushing service templates and configs"
  rsync_pi -avz "${ROOT_DIR}/deploy/openrc/maison" "${PI_HOST}:${PI_APP_DIR}/deploy/openrc/"
  rsync_pi -avz "${ROOT_DIR}/deploy/openrc/cloudflared-maison" "${PI_HOST}:${PI_APP_DIR}/deploy/openrc/"
  rsync_pi -avz "${ROOT_DIR}/deploy/mosquitto/maison.conf" "${PI_HOST}:${PI_APP_DIR}/deploy/mosquitto/"
  rsync_pi -avz --chown=root:root "${ROOT_DIR}/deploy/logrotate/maison" "${PI_HOST}:/etc/logrotate.d/maison"

  if [ -d "${ROOT_DIR}/mosquitto/certs" ]; then
    log "Pushing Mosquitto certificates"
    # The CA private key never needs to leave this machine.
    rsync_pi -avz --exclude ca-key.pem "${ROOT_DIR}/mosquitto/certs/" "${PI_HOST}:${PI_APP_DIR}/mosquitto/certs/"
  else
    warn "mosquitto/certs is missing locally; TLS listener deployment may fail"
  fi

  log "Pushing Matter attestation roots"
  rsync_pi -avz --delete "${ROOT_DIR}/matter-trust/" "${PI_HOST}:${PI_APP_DIR}/matter-trust/"

  if [ -f "${CLOUDFLARED_BIN}" ]; then
    log "Pushing cloudflared binary to /usr/local/bin/cloudflared"
    rsync_pi -avz "${CLOUDFLARED_BIN}" "${PI_HOST}:/usr/local/bin/cloudflared"
    ssh_pi chmod +x /usr/local/bin/cloudflared
  else
    warn "cloudflared-arm binary not found at ${CLOUDFLARED_BIN}; skipping"
    warn "Build it with: ./scripts/build-cloudflared-armv6.sh"
  fi

  local relative_path entry
  for relative_path in "${RUNTIME_FILES[@]}"; do
    if [ -f "${ROOT_DIR}/${relative_path}" ]; then
      log "Pushing ${relative_path}"
      rsync_pi -avz "${secret[@]}" "${ROOT_DIR}/${relative_path}" "${PI_HOST}:${PI_APP_DIR}/"
    fi
  done

  for relative_path in "${STATE_FILES[@]}"; do
    if [ -f "${ROOT_DIR}/${relative_path}" ]; then
      warn "Skipping push of mutable runtime file ${relative_path}; keeping remote state"
      ## Enable on first deploy
      #log "Pushing ${relative_path}"
      #rsync_pi -avz "${ROOT_DIR}/${relative_path}" "${PI_HOST}:${PI_APP_DIR}/"
    fi
  done

  if [ -d "${ROOT_DIR}/cache" ]; then
    log "Pushing cache directory"
    # --update: what the Pi has learnt since (Tempo seasons, netload.json, weather.json) is
    # newer than this machine's copy and stays. model.json is fitted here (fit_tempo) and
    # versioned: always this machine's.
    rsync_pi -avz --update "${ROOT_DIR}/cache/" "${PI_HOST}:${PI_APP_DIR}/cache/"
    rsync_pi -avz "${ROOT_DIR}/cache/tempo/model.json" "${PI_HOST}:${PI_APP_DIR}/cache/tempo/"
  fi

  fix_state_ownership
}

upgrade_pi() {
  log "Upgrading host-native services on the Pi"
  ssh_pi_with_state <<'EOF'
apk update
apk add --no-cache bash ca-certificates curl git logrotate mosquitto rsync

mkdir -p /etc/mosquitto/conf.d /etc/mosquitto/certs/maison /var/log/mosquitto /var/log

if [ -f "${APP_DIR}/deploy/mosquitto/maison.conf" ]; then
  cp "${APP_DIR}/deploy/mosquitto/maison.conf" /etc/mosquitto/conf.d/maison.conf
fi

if [ -f "${APP_DIR}/mosquitto/certs/ca.pem" ]; then
  cp "${APP_DIR}/mosquitto/certs/ca.pem" /etc/mosquitto/certs/maison/ca.pem
fi
if [ -f "${APP_DIR}/mosquitto/certs/server.pem" ]; then
  cp "${APP_DIR}/mosquitto/certs/server.pem" /etc/mosquitto/certs/maison/server.pem
fi
if [ -f "${APP_DIR}/mosquitto/certs/server-key.pem" ]; then
  cp "${APP_DIR}/mosquitto/certs/server-key.pem" /etc/mosquitto/certs/maison/server-key.pem
  chmod 600 /etc/mosquitto/certs/maison/server-key.pem
fi

chown -R mosquitto:mosquitto /etc/mosquitto/certs/maison /var/log/mosquitto 2>/dev/null || true
touch /var/log/maison.log /var/log/cloudflared-maison.log
chown "${SERVICE_USER}:${SERVICE_GROUP}" /var/log/maison.log /var/log/cloudflared-maison.log
chmod 644 /var/log/maison.log /var/log/cloudflared-maison.log

if ! command -v cloudflared >/dev/null 2>&1; then
  printf '%s\n' 'Warning: cloudflared is not installed on the Pi.' >&2
  printf '%s\n' 'Push it with: ./scripts/build-cloudflared-armv6.sh && ./deploy.sh push' >&2
fi
EOF
}

start_pi() {
  log "Installing service definitions and restarting the stack"
  ssh_pi_with_state <<'EOF'
for service in maison cloudflared-maison; do
  sed \
    -e "s#@@APP_DIR@@#${APP_DIR}#g" \
    -e "s#@@SERVICE_USER@@#${SERVICE_USER}#g" \
    -e "s#@@SERVICE_GROUP@@#${SERVICE_GROUP}#g" \
    "${APP_DIR}/deploy/openrc/${service}" | tee "/etc/init.d/${service}" >/dev/null
  chmod +x "/etc/init.d/${service}"
done

rc-update add mosquitto default >/dev/null 2>&1 || true
rc-update add maison default >/dev/null 2>&1 || true
# --nodeps: maison `need`s mosquitto, so a plain mosquitto restart would also stop maison and
# start it again in the background, racing the restart below (« maison stopped by something
# else », flock errors). maison only needs the broker for the plugs' boot; restart it once.
rc-service --nodeps mosquitto restart || rc-service mosquitto start
rc-service maison restart || rc-service maison start

if command -v cloudflared >/dev/null 2>&1 && grep -q '^CLOUDFLARE_TUNNEL_TOKEN=.' "${APP_DIR}/.env"; then
  rc-update add cloudflared-maison default >/dev/null 2>&1 || true
  rc-service cloudflared-maison restart || rc-service cloudflared-maison start
else
  if ! command -v cloudflared >/dev/null 2>&1; then
    printf '%s\n' 'Skipping cloudflared service start: cloudflared is not installed on the Pi.' >&2
  elif ! grep -q '^CLOUDFLARE_TUNNEL_TOKEN=.' "${APP_DIR}/.env"; then
    printf '%s\n' 'Skipping cloudflared service start: CLOUDFLARE_TUNNEL_TOKEN is missing from the remote .env.' >&2
  fi
fi

sleep 2

for service in mosquitto maison cloudflared-maison; do
  if rc-service "${service}" status >/dev/null 2>&1; then
    printf '%s: active\n' "${service}"
  else
    printf '%s: inactive\n' "${service}"
  fi
done
EOF
}

stop_pi() {
  log "Stopping the stack on the Pi"
  ssh_pi_script <<'EOF'
for service in cloudflared-maison maison mosquitto; do
  if [ -x "/etc/init.d/${service}" ]; then
    rc-service "${service}" stop >/dev/null 2>&1 || true
  fi
done

for service in mosquitto maison cloudflared-maison; do
  if [ -x "/etc/init.d/${service}" ] && rc-service "${service}" status >/dev/null 2>&1; then
    printf '%s: active\n' "${service}"
  elif [ -x "/etc/init.d/${service}" ]; then
    printf '%s: inactive\n' "${service}"
  fi
done
EOF
}

logs_pi() {
  local target="${1:-stack}"
  log "Following ${target} logs on the Pi"
  ssh_pi_script "${target}" <<'EOF'
TARGET="$1"

case "${TARGET}" in
  stack)
    touch /var/log/mosquitto/mosquitto.log /var/log/maison.log /var/log/cloudflared-maison.log
    exec tail -f /var/log/mosquitto/mosquitto.log /var/log/maison.log /var/log/cloudflared-maison.log
    ;;
  mosquitto)
    touch /var/log/mosquitto/mosquitto.log
    exec tail -f /var/log/mosquitto/mosquitto.log
    ;;
  backend|maison)
    touch /var/log/maison.log
    exec tail -f /var/log/maison.log
    ;;
  cloudflared|tunnel)
    touch /var/log/cloudflared-maison.log
    exec tail -f /var/log/cloudflared-maison.log
    ;;
  *)
    printf 'Unknown log target: %s\n' "${TARGET}" >&2
    printf '%s\n' 'Valid targets: stack, mosquitto, backend, cloudflared' >&2
    exit 1
    ;;
esac
EOF
}

status_pi() {
  log "Collecting deployment status from the Pi"
  ssh_pi_script "${REMOTE_BACKEND_DIR}" "${REMOTE_WEB_DIR}" <<'EOF'
BACKEND_DIR="$1"
WEB_DIR="$2"

service_state_openrc() {
  if rc-service "$1" status >/dev/null 2>&1; then
    printf 'active'
  elif [ -x "/etc/init.d/$1" ]; then
    printf 'inactive'
  else
    printf 'not-installed'
  fi
}

public_hostname=''
if [ -f "${APP_DIR}/.env" ]; then
  public_hostname="$(grep -E '^CLOUDFLARE_PUBLIC_HOSTNAME=' "${APP_DIR}/.env" | tail -n1 | cut -d'=' -f2-)"
fi

tunnel_token_present='no'
if [ -f "${APP_DIR}/.env" ] && grep -q '^CLOUDFLARE_TUNNEL_TOKEN=.' "${APP_DIR}/.env"; then
  tunnel_token_present='yes'
fi

printf 'Platform:\n'
printf '  os: %s\n' "$(. /etc/os-release && printf '%s' "${PRETTY_NAME:-unknown}")"

printf '\nServices:\n'
printf '  mosquitto: %s\n' "$(service_state_openrc mosquitto)"
printf '  maison: %s\n' "$(service_state_openrc maison)"
printf '  cloudflared: %s\n' "$(service_state_openrc cloudflared-maison)"

printf '\nRuntime:\n'
if command -v cloudflared >/dev/null 2>&1; then
  printf '  cloudflared: installed (%s)\n' "$(cloudflared --version 2>/dev/null | head -n1)"
else
  printf '%s\n' '  cloudflared: missing'
fi

printf '\nPaths:\n'
printf '  app: %s\n' "${APP_DIR}"
printf '  backend: %s\n' "${APP_DIR}/${BACKEND_DIR}/maison-backend"
printf '  web: %s\n' "${APP_DIR}/${WEB_DIR}"

printf '\nAccess:\n'
printf '  local: http://%s:3033\n' "$(hostname -i 2>/dev/null | awk '{print $1}' || hostname)"
if [ -n "${public_hostname}" ]; then
  printf '  public: https://%s\n' "${public_hostname}"
else
  printf '%s\n' '  public: not configured'
fi

printf '\nHints:\n'
if ! command -v cloudflared >/dev/null 2>&1; then
  printf '%s\n' '  - Cloudflare Tunnel cannot start because cloudflared is not installed.'
elif [ "${tunnel_token_present}" != 'yes' ]; then
  printf '%s\n' '  - Cloudflare Tunnel cannot start because CLOUDFLARE_TUNNEL_TOKEN is missing in the remote .env.'
fi
EOF
}

COMMAND="${1:-all}"
LOG_TARGET="${2:-stack}"

case "${COMMAND}" in
  all)
    build_local
    push_to_pi
    upgrade_pi
    start_pi
    ;;
  build)
    build_local
    ;;
  push)
    push_to_pi
    ;;
  upgrade)
    upgrade_pi
    ;;
  start)
    start_pi
    ;;
  stop)
    stop_pi
    ;;
  logs)
    logs_pi "${LOG_TARGET}"
    ;;
  status)
    status_pi
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    printf 'Unknown command: %s\n\n' "${COMMAND}" >&2
    usage >&2
    exit 1
    ;;
esac

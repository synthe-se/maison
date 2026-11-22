# Deploying

Maison on a host is one static binary, the web bundle it serves, an `.env` and its state files,
under `/opt/maison`. Nothing is built or installed on the host: the build machine cross-builds,
`deploy.sh` pushes over SSH with `sh`, `tar` and `cat`, and the host's own supervisor runs it.
No container, no runtime, no database.

It has run on a Raspberry Pi 1 (ARMv6, Alpine) and runs on a former ISP set-top box (MIPS32,
512 MB, a 28 MB flash partition for the app and its state): a 13.7 MB binary, 18 MB of memory
in use.

## The host

Any Linux that runs the binary. `deploy.sh` expects of it:

- SSH as root by key; `sh`, `tar`, `wget` and `killall` (BusyBox's do).
- The service's account, `maison`, in the group of the Zigbee dongle's serial port (`dialout`).
- A supervisor (inittab `respawn`, a systemd unit with `Restart=always`, OpenRC's
  `supervise-daemon`…) that runs `/opt/maison/backend/target/release/maison-backend` as that
  account, from `/opt/maison`, with `MAISON_SOURCE_ROOT=/opt/maison`, starts it again when it
  exits, and starts nothing while `/opt/maison/stopped` exists. The backend reads `.env`
  itself. `deploy.sh start` kills the process and waits for `/health`; `stop` writes `stopped`
  first.
- `maison-logs [stack|backend|mosquitto]` and `maison-status`: the host's own account of its
  logs and services (`deploy.sh logs`, `status`).
- A CA bundle (Tempo, the weather), IPv6 (Matter), and a clock: on a host without one, Maison
  waits for the kernel to say the clock is synced before it mints Matter certificates or moves
  shutters.
- With Meross plugs only: Mosquitto on `/etc/mosquitto/maison.conf` (TLS on 8883, pushed with
  its certificates to `/etc/mosquitto/certs/maison`), behind a firewall that lets only the
  addresses of `/opt/maison/deploy/mosquitto/plugs.allow` reach 8883, never listening without
  it, and stopped by `stopped` too. The plugs' firmware needs a broker to boot; the backend
  itself talks to them over HTTP.

The set-top box's image ([uplg/BCM7231B2](https://github.com/uplg/BCM7231B2), its Maison role)
is such a host, on BusyBox init: `maison-run`, `mosquitto-run` (an nftables table from
`plugs.allow`), `maison-logs`, `maison-status`.

## Build

`deploy.env` (`cp deploy.env.example deploy.env`, never pushed, sourced by `deploy.sh`) says
where the host is and what it runs:

| Key | |
|---|---|
| `DEPLOY_HOST` | the SSH target (`root@<host>`) |
| `BACKEND_TARGET` | a target rustup ships (`aarch64-unknown-linux-musl`, `arm-unknown-linux-musleabihf`, `x86_64-unknown-linux-musl`…), built by `cargo zigbuild --release --locked --no-default-features` (no Bluetooth); `RUSTFLAGS` is passed on (`-C target-cpu=arm1176jzf-s` for a Raspberry Pi 1) |
| `BACKEND_BUILD`, `BACKEND_BIN` | instead: a command that builds the backend for a target rustup does not ship, and the binary it makes (the set-top box: its image's `make maison`, nightly `build-std` for MIPS32) |
| `EDGE_HOUSE_DIR` | with `EDGE_LISTEN`: the directory `maison-backend edge init` made for this host; its certificates are pushed to `EDGE_DIR` |
| `APP_DIR`, `SERVICE_USER`, `SERVICE_GROUP`, `ENV_FILE` | defaults: `/opt/maison`, `maison`, `maison`, `./.env` |

For a `BACKEND_TARGET` build, once:

```bash
rustup target add <target>
cargo install cargo-zigbuild
brew install zig
```

OpenSSL (linked by webauthn-rs) is vendored and compiled by zig too. `make deny` checks what
the backend depends on (`backend/deny.toml`: no GPL, crates.io and the pinned uplg forks only,
RustSec advisories).

## `.env`

`cp .env.example .env`. The same file is pushed to the host on every deploy, so nothing
dev-only goes in it (pass `PUBLIC_URL=http://localhost:5173` in the shell when developing).

| Key | |
|---|---|
| `JWT_SECRET` | at least 32 random bytes (`openssl rand -hex 32`); the backend refuses a shorter one or a placeholder |
| `PUBLIC_URL` | where Maison is reached, `https://<host name>`. Its host is the passkeys' RP ID: changing it loses every passkey |
| `EDGE_LISTEN` | the edge listener's address (`backend/src/edge.rs`): the public address through a reverse proxy elsewhere, mutual TLS; its certificates in `EDGE_DIR` (default `edge/`) |
| `AUTH_COOKIE_SECURE` | `true` (the public address is HTTPS) |
| `DISABLE_BLUETOOTH` | `true` on a host built without it |
| `ALLOW_LAN_TOKENS` | optional, off: lets an app's service token reach the device routes over the LAN's plain HTTP (Tailscale is always allowed) |
| `SCREEN_IDLE_DAYS` | optional, default 90: a wall screen unseen this long is revoked (0: never) |
| `MAISON_LOG_FILE` | optional, default `/var/log/maison.log`: where the CLI (`invite`, `token`) adds its security events, so the service's log tells everything that opened the house |
| `ZIGBEE_SERIAL_PORT`, `ZIGBEE_ADAPTER=ember`, `ZIGBEE_EZSP_PROTOCOL_VERSION=13` | the Sonoff Dongle Lite MG21 (`/dev/ttyUSB0` on the host) |
| `NABAZTAG_HOST` | the rabbit's address, optional |
| `RTE_CLIENT_ID`, `RTE_CLIENT_SECRET` | optional: RTE's official Tempo API (free account on data.rte-france.com), else open data |
| `SCENES_JSON_PATH` | optional: where the scenes are kept (default `scenes.json`); every state file has such a `*_JSON_PATH` |
| `PHOTOS_DIR` | optional: where the wall frame's photos are kept (default `photos/`, 0700, made by the first one) |
| `PORT` | optional, default 3033 (`API_PORT` is read too); a value that does not parse is logged and the default used, like every numeric setting |

Device addresses live in their own files: `devices.json` (Tuya, with local keys) and
`meross-devices.json` are pushed from the build machine; the rest (the TV, the box, the
blaster's codes, the scenes) is the host's own state, edited from the web app.

## Deploy

```bash
make deploy              # build the web bundle and the backend, push, restart, wait for /health
make deploy-status       # the host's services
make deploy-logs         # LOG_TARGET=stack|backend|mosquitto
make deploy-push         # or ./deploy.sh build|push|start|stop
```

Each push also writes `deploy/mosquitto/plugs.allow` from `meross-devices.json` (anything but a
private IPv4 address stops the deploy). The logs name people, addresses and refusals: every 401
and 403 is logged with its code, the path, who asked and the client's address. Keep them
readable by root and the service alone, and long enough for an incident noticed a week late.

What a push does to the cache: of `cache/tempo/`, only what the host does not have or what is
newer goes (what the host learnt since stays), except `model.json`, which is always the build
machine's. Mutable state files are never pushed.

## Reaching it

Passkeys need a domain and HTTPS, so the house is reached at `PUBLIC_URL`, one of two ways:

- **The edge listener** (`EDGE_LISTEN`): a reverse proxy elsewhere holds the public name and its
  certificate, and reaches the host over a private network. TLS both ways: the proxy presents a
  client certificate (`maison-backend edge init`), and only its `X-Forwarded-For` is believed.
  A wall frame on the LAN comes the same way with its own certificate (`edge client`,
  [frame.md](frame.md)).
- **A tunnel on the host**, arriving on loopback (cloudflared): the client's address is
  `CF-Connecting-IP`, believed from loopback alone. Run it under its own account, so that it
  reads neither `.env` nor `auth/` nor `matter/`. With Cloudflare, set the zone's Browser Cache
  TTL to Respect Existing Headers: otherwise it overrides the backend's `Cache-Control`
  (`no-store` for `/api/`, a year for `/_app/immutable/`, `no-cache` for the rest) and a new
  page or icon stays hidden for hours.

From the LAN, `http://<host>:3033` answers only `/health` and, for an app's service token, the
device routes (over Tailscale; over the plain LAN only with `ALLOW_LAN_TOKENS`,
[passkeys.md](passkeys.md#service-tokens-machines)).

## First person

Entry is by invitation only. The first one (or a way back in) comes from the host:

```bash
ssh <host> 'cd /opt/maison && ./backend/target/release/maison-backend invite me --name Me --admin'
```

It prints a one-time link (`<PUBLIC_URL>/invite#<token>`; 72 hours for an admin, 7 days
otherwise). Open it on the device that will hold the passkey. After that, admins invite from
their account page. A machine (a companion app) gets a service token instead:
`maison-backend token create <name>` ([passkeys.md](passkeys.md#service-tokens-machines)).

## Layout and permissions

| Path under `/opt/maison` | Owner, mode | |
|---|---|---|
| `.` | `root:maison 1775` | group-writable and sticky: the service creates and renames its own files, never root's |
| `.env`, `devices.json`, `meross-devices.json` | `root:maison 0640` | secrets, pushed |
| `deploy/mosquitto/plugs.allow` | root, `0644` | the plugs' IPv4 addresses, the firewall's list |
| `auth/`, `matter/`, `photos/` | `maison`, `0700` | people and passkeys; Matter fabric keys and shutters; the frame's photos |
| `edge/` | `maison`, its key `0600` | the edge listener's certificates |
| `cache/`, state files (`*.json`, `adb-key`, `atv-identity`) | `maison` | written by the backend (temp file + rename, 0600 when private), never pushed |
| `backend/target/release/maison-backend`, `web/build/` | root | the binary and the bundle it serves |
| `matter-trust/` | root | CSA attestation roots (tracked; `scripts/update-matter-trust.sh` refreshes them) |

`deploy.sh` restores that ownership after every push.

## Chores

- **Tempo, once a year** (or when RTE changes its rules), on the build machine only:
  `cargo run --release --manifest-path backend/Cargo.toml --bin fit_tempo`, commit
  `cache/tempo/`, then `make deploy`. See [tempo.md](tempo.md).
- **README screenshot**: `scripts/screenshot.sh` renders an invented house
  (`e2e/readme.ts`) into `screenshots/maison-<hash>.jpg`, named after its content (GitHub's
  image redirect drops a `?v=` and caches the address), and points the README at it.
- **Icons**: `scripts/icons.sh` regenerates every icon and `favicon.ico` from
  `web/static/brand.svg` and `web/brand/maskable.svg`.
- **Matter roots**: `scripts/update-matter-trust.sh` when a new device fails attestation.
- **A Meross plug**: `scripts/meross-provision.py` provisions one without its cloud, or
  repoints it when the broker moved.

# Deploying to the Raspberry Pi 1

Production is a Raspberry Pi 1 Model B (ARMv6, 512 MB) on Alpine Linux in sys mode, with
OpenRC. No Docker, nothing built on the Pi: the Mac cross-builds a static musl binary and
the web bundle, and `deploy.sh` pushes them.

The Pi runs three services: `mosquitto` (TLS on :8883, only because the Meross plugs'
firmware needs a broker to boot; the backend itself talks to them over HTTP), `maison` (the
backend, which also serves the web app) and `cloudflared-maison` (the tunnel).

## On the Mac, once

```bash
rustup target add arm-unknown-linux-musleabihf
cargo install cargo-zigbuild
brew install zig librsvg          # librsvg: scripts/icons.sh
git clone https://github.com/cloudflare/cloudflared.git ../cloudflared   # needs Go
```

`make build-pi` (run by `make deploy`) is `scripts/build-rpi1-backend.sh`: `cargo zigbuild
--release --no-default-features` (no Bluetooth on the Pi) for `arm-unknown-linux-musleabihf`
with `-C target-cpu=arm1176jzf-s`. OpenSSL (linked by webauthn-rs) is vendored and compiled
by zig too.

## A fresh Pi

Flash Alpine (`alpine-rpi-3.24.1-armhf.img.xz`) with the headless bootstrap, from macOS:

```bash
scripts/flash-alpine-headless-macos.sh --image ~/Downloads/alpine-rpi-3.24.1-armhf.img.xz --disk disk4
```

The script erases the card (it asks for confirmation unless `--no-confirm`), adds the
[headless bootstrap](https://github.com/macmpi/alpine-linux-headless-bootstrap) overlay,
and copies what `bootstrap/` holds: `unattended.sh` (installs Alpine to the card in sys
mode, hostname `maison`, chrony, Mosquitto, the `maison` service user, `/opt/maison`; root
logs in over SSH by key only), and optionally `interfaces`, `wpa_supplicant.conf`,
`authorized_keys` (default: your first `~/.ssh/id_*.pub`) and `ssh-host-keys/`. Flags
(`--hostname`, `--interfaces`, `--wpa-supplicant`, `--unattended`, `--authorized-keys`,
`--ssh-host-keys`, `--keep-mounted`) override them. Once the Pi has rebooted, deploy.

The Pi has no real-time clock: it boots on the time swclock saved at shutdown, and NTP then
steps the clock, possibly weeks ahead. Maison waits for a synced clock before it mints
Matter certificates or moves shutters.

## `.env`

`cp .env.example .env`. The same file is pushed to the Pi on every deploy, so nothing
dev-only goes in it (pass `PUBLIC_URL=http://localhost:5173` in the shell when developing).

| Key | |
|---|---|
| `PI_HOST` | SSH target for `deploy.sh` and every `make deploy-*` (e.g. `root@<pi>`) |
| `JWT_SECRET` | at least 32 random bytes (`openssl rand -hex 32`); the backend refuses a shorter one or a placeholder |
| `CLOUDFLARE_PUBLIC_HOSTNAME` | the public host; `PUBLIC_URL` defaults to `https://` + it. Its host is the passkeys' RP ID: changing it loses every passkey |
| `CLOUDFLARE_TUNNEL_TOKEN` | the tunnel's token; without it `cloudflared-maison` is not started. Never on a command line |
| `AUTH_COOKIE_SECURE` | `true` (the tunnel is HTTPS) |
| `DISABLE_BLUETOOTH` | `true` on the Pi |
| `IR_API_TOKEN` | the set-top box's token for `POST /api/ir/key` (same value in its `kird.conf`); empty disables the route |
| `ZIGBEE_SERIAL_PORT`, `ZIGBEE_ADAPTER=ember`, `ZIGBEE_EZSP_PROTOCOL_VERSION=13` | the Sonoff Dongle Lite MG21 (`/dev/ttyUSB0` on the Pi; the service user is put in `dialout`) |
| `NABAZTAG_HOST` | the rabbit's address, optional |
| `RTE_CLIENT_ID`, `RTE_CLIENT_SECRET` | optional: RTE's official Tempo API (free account on data.rte-france.com), else open data |
| `SCENES_JSON_PATH` | optional: where the scenes are kept (default `scenes.json`); every state file has such a `*_JSON_PATH` |
| `PORT` | optional, default 3033 (`API_PORT` is read too); a value that does not parse is logged and the default used, like every numeric setting |

Device addresses live in their own files: `devices.json` (Tuya, with local keys) and
`meross-devices.json` are pushed from the Mac; `tv.json`, `androidtv.json` (from their
`.template`) and the rest are the Pi's own state, edited from the web app.

## Deploy

```bash
make deploy              # build web + ARMv6 backend, push, install host packages, restart services
make deploy-status       # services, versions, URLs
make deploy-logs         # LOG_TARGET=stack|backend|mosquitto|cloudflared
make deploy-push         # or ./deploy.sh build|push|upgrade|start|stop
```

`PI_PASSWORD` uses `sshpass` instead of a key. Logs: `/var/log/maison.log`,
`/var/log/cloudflared-maison.log`, `/var/log/mosquitto/mosquitto.log` (logrotate).

What a push does to the cache: `cache/tempo/` goes with `--update` (what the Pi learnt since
stays), except `model.json`, which is always the Mac's. Mutable state files are never pushed.

## First person

Entry is by invitation only. The first one (or a way back in) comes from the Pi:

```bash
ssh <pi> 'cd /opt/maison && ./backend/target/release/maison-backend invite leonard --name Léonard --admin'
```

It prints a one-time link (7 days). Open it on the device that will hold the passkey.
After that, admins invite from « Mon compte ».

## Layout and permissions on the Pi

| Path | Owner, mode | |
|---|---|---|
| `/opt/maison` | `root:maison 1775` | group-writable and sticky: the service creates and renames its own files, never root's |
| `.env`, `devices.json`, `meross-devices.json` | `root:maison 0640` | secrets, pushed |
| `auth/`, `matter/` | `maison`, `0700` | people and passkeys; Matter fabric keys and shutters |
| `cache/`, state files (`*.json`, `adb-key`, `atv-identity`) | `maison` | written by the backend (temp file + rename, 0600 when private) |
| `scenes.json`, `ir-keymap.json` | `maison` | the scenes and the remote's keys, edited from the web app, never pushed |
| `backend/target/release/maison-backend`, `web/build/` | root | the binary and the bundle it serves |
| `matter-trust/` | root | CSA attestation roots (tracked; `scripts/update-matter-trust.sh` refreshes them) |

`deploy.sh` restores that ownership after every push (rsync runs as root). From the LAN,
`http://<pi>:3033` answers only `/health` and `/api/ir/key`; everything else goes through
the tunnel, since passkeys need a domain and HTTPS. Mosquitto's plaintext listener is bound
to 127.0.0.1.

## Cloudflare

- `make cloudflared-upgrade` builds the latest cloudflared release tag from `../cloudflared`
  for ARMv6 (`GOARM=6`; `CLOUDFLARED_VERSION=<tag>` pins one, `SKIP_PULL=1` builds what is
  checked out), then swaps it on the Pi: stop, keep `cloudflared.bak`, move in, restart,
  print the version.
- Zone setting: **Caching → Browser Cache TTL: Respect Existing Headers**. Otherwise
  Cloudflare overrides the backend's `Cache-Control` (`no-store` for `/api/`, a year for
  `/_app/immutable/`, `no-cache` for the rest) and a new page or icon stays hidden for hours.

## Chores

- **Tempo, once a year** (or when RTE changes its rules), on the Mac only:
  `cargo run --release --manifest-path backend/Cargo.toml --bin fit_tempo`, commit
  `cache/tempo/`, then `make deploy`. See [tempo.md](tempo.md).
- **README screenshot**: `scripts/screenshot.sh` renders an invented house
  (`e2e/readme.ts`) into `screenshots/maison-<hash>.jpg`, named after its content (GitHub's
  image redirect drops a `?v=` and caches the address), and points the README at it.
- **Icons**: `scripts/icons.sh` regenerates every icon and `favicon.ico` from
  `web/static/brand.svg` and `web/brand/maskable.svg`.
- **Matter roots**: `scripts/update-matter-trust.sh` when a new device fails attestation.

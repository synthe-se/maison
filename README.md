# Maison

## What it can do

- Monitor and control local home devices from a single interface.
- Manage Tuya-based devices such as feeders, fountains, and litter boxes.
- Track energy and status data for Meross plugs.
- Control Philips Hue lamps over Bluetooth and Zigbee.
- Handle Hue dimmer switch (v1 at least), global handling, On/off change power state for every connected zigbee device, dim up/down same.
- Query Tempo data, predictions, history, and calibration helpers.
- Mirror the daily Tempo colors on a Nabaztag running the garenne firmware (belly LED = today, ears = tomorrow).
- Turn a set-top-box remote control into a house remote: an AirTies AIR 7310T (custom firmware) decodes its Ruwido IR remote and forwards every key press to maison, which maps buttons to actions. See "IR remote" below.
- Drive the living-room Philips TV (55PUS6753, Saphi) over its JointSPACE API: volume, Ambilight and remote keys, with power going over infrared — the one channel that reaches the set in deep standby. See "Television" below.
- Drive the Android TV box (MECOOL LEAP-S1) from a proper on-screen remote — D-pad, media, volume, app shortcuts and APK sideloading — over two native clients: the Remote v2 protocol a physical remote speaks (fast, needs pairing) with ADB as the fallback and for what Remote v2 cannot do. See "Android TV box" below.
- Drive roller shutters (Sonoff Orb-RBS) over Matter, fully local: open, stop, close and percentage positions. Maison is its own Matter commissioner. See "Roller shutters (Matter)" below.
- Keep access private with local authentication and secure session cookies.

![Maison](/screenshots/maison.jpg?v=1787694963)

Two app components, plus their shared texts and tests:

- `web/`: the web app — SvelteKit 3 (Svelte 5 runes), a pure SPA served by the backend,
  Bits UI for every interactive widget, Paraglide for the texts, no CSS framework
- `backend/`: the Rust backend (Axum)
- `i18n/`: the only source of the texts (`messages/fr.json`, `en.json`, inlang format)
- `e2e/`: end-to-end scenarios (Playwright + axe) against the real backend
- `docs/ux/`, `docs/dependances/`: the research the interface and the stack follow, sources cited

## The web app

Same design rules as [Ariane](../ariane): the synthe.se tokens (warm ground, ink, one petrol
accent; Fraunces + Cal Sans, self-hosted), one page container and a 4 px spacing scale
(`docs/ux/mise-en-page.md` in Ariane), WCAG 2.2 AA as a floor. What is specific to a house
dashboard — a tile per device whose icon is its gesture, a command in flight (optimistic, « Allumage… »
after 1 s, « Pas de réponse » at the device family's limit), sliders, live state, Tempo colours with
shapes — is in `docs/ux/tableau-de-bord.md`. The stack and what SvelteKit 3 / Vitest 5 changed
since Ariane's notes: `docs/dependances/web.md`.

Rules the code keeps (and the checks enforce):

- **One logic, one place.** A gesture goes through `Gesture` (`#lib/gesture.svelte.ts`), an
  on/off through `Command`, server state through `live()`, every slider is `Range`, every
  choice `Select`, every page head `PageHead`; dates and numbers come from `#lib/i18n.svelte.ts`.
  Each Bits UI primitive is wrapped once in `web/src/lib/components/`.
- **Texts only from `i18n/messages`**, French typography in French (narrow no-break spaces).
  `bun run check` refuses a key missing in a locale, unused, or saying the same as another
  (`web/scripts/i18n.check.ts`; legitimate homonyms in `i18n.allow.json` with their reason).
- **Tokens only** for corners and control heights (`web/scripts/css.check.ts`).
- **Strict CSP**: the backend allows the built page's inline scripts by their SHA-256, never
  `'unsafe-inline'` (`content_security_policy` in `backend/src/lib.rs`).

## Tests

```bash
cargo test --manifest-path backend/Cargo.toml   # backend: unit + integration
cd web && bun run check                         # types (svelte-check), messages, CSS tokens
cd web && bun run test                          # Vitest: components and rune modules in Chromium, the rest in Node
cd web && bun run coverage                      # same, with v8 coverage
cd e2e && bun install && bun run all            # after `cargo build` and `bun --cwd web run build`
```

`e2e/run.sh scenario.ts` starts the real backend on a throwaway root (a test account, no device
file: nothing on the LAN is touched) serving `web/build`; `e2e/house.ts` simulates the devices a
scenario needs. Scenarios: `smoke` (sign-in, navigation, sign-out), `passkeys` (invitation, passkeys added and removed, an admin invites, sign-in again; a Chromium virtual authenticator), `dashboard` (tiles, gestures,
a device that does not answer), `layout` (measured at 390, 1024, 1440 and 2560 px), `a11y` (axe,
light and dark, desktop and phone).

## Runtime files kept in place

The Rust backend reads these files directly from the repo root:

- `devices.json`
- `device-cache.json`
- `auth/auth.json` (people, their passkeys, pending invitations; written by the backend)
- `meross-devices.json`
- `hue-lamps.json`
- `hue-lamps-blacklist.json`
- `zigbee-lamps.json`
- `zigbee-lamps-blacklist.json`
- `climate-state.json`
- `ir-keymap.json`
- `tv.json`
- `androidtv.json`
- `adb-key`
- `atv-identity`
- `matter/` (fabric keys + paired shutters, mode 0700)
- `matter-trust/` (Matter attestation roots, tracked in git)
- `mosquitto/`

Tempo cache and calibration files now live in `cache/tempo/`.

Tempo recalibration workflow is documented in `docs/tempo-calibration.md`.

## Television

The 55PUS6753 runs Saphi, not Android TV, which is what makes it controllable
at all: the JointSPACE API answers plain HTTP on port 1925 with
`pairing_type: "none"` — no pairing, no auth, no certificate. Copy
`tv.json.template` to `tv.json` and fill in the set's address, the address of
the Broadlink blaster that fronts it and, optionally, the Android box address.

Two things are worth knowing before poking at it:

- **The endpoint whitelist is not advisory.** Saphi answers `Forbidden` or
  `Not Found` on what it does not implement (`/6/sources`, `/6/applications`,
  `/6/activities/*`, anything under `/5/`), and hitting those repeatedly kills
  the embedded server *persistently* — neither a standby cycle nor the API's
  own `Standby` brings it back, only unplugging the set from the mains for
  ~30 s. `tv.rs` therefore models the reachable surface as an enum and spaces
  every request through a single gate. Do not widen it without verifying.
- **Power does not go over the API.** Light standby still answers on 1925
  (`powerstate: "Standby"`), but deep standby drops the network stack entirely
  and nothing on the network gets it back: Wake-on-LAN fails despite the SSDP
  `WAKEUP` header, even as raw layer-2 magic packets that bypass IP routing
  altogether, and CEC fails too because the set leaves the bus. Infrared is the
  only channel that survives, its receiver staying powered at every depth — and
  it is also the only one that still works once the JointSPACE server has
  crashed. So `power_on` and `power_off` both fire a Philips RC5 code through
  the blaster, discrete rather than a toggle (`0x3F` on, `0x3D` off, address
  `0x00`), which makes them safe to send without reading the current state
  back. See `philips_ir.rs`; `cargo run --bin send_ir` fires a raw packet by
  hand.

Source switching is the one thing JointSPACE cannot do on Saphi. Powering on
with `switchToBox` instead nudges the Android box awake over DIAL, which makes
it assert CEC One Touch Play — that both powers the set and routes it to the
box's HDMI input. This is the fix for "the TV came up on the wrong input".

## IR remote debounce

Presses of the same key closer than 1.2 s are treated as phantom doubles:
marginal IR reception splits one hold into several presses, which is fatal for
toggles — the action cancels itself. That default is wrong for navigation,
where the second press of a D-pad is deliberate and arrives well inside the
window. Bindings therefore take an optional `debounce_ms`; the shipped keymap
uses 150 ms for the D-pad, media and volume keys and leaves toggles (climate,
plugs, lamps) on the 1.2 s default. The value is clamped at 50 ms so a typo
cannot disable the filter for the bindings that depend on it.

Worth knowing when a key feels slow: `input keyevent` starts a JVM on the box,
which costs ~150 ms awake but around **10 s when the box is asleep** — the same
delay the official `adb` binary shows, so it is the box, not this code.

## Android TV box

Two channels reach the box, and which one carries a key matters.

**Remote v2** (`atvremote.rs`) is the protocol Google's own remote app speaks:
mutual TLS on port 6466, protobuf messages, one long-lived session. It is what
the fast path is built on, and the reason is measurable — sending a key over
ADB costs ~150 ms, of which only 65 ms is ADB itself; the rest is `input`
booting a JVM on *every single press*. Remote v2 has no such cost, and because
the session is owned by a background task the call returns to the HTTP handler
in microseconds instead of blocking it.

It needs pairing, once per host: the dashboard's "Pair" button opens a session,
the TV shows six hex digits, and typing them back proves the screen was read
(the client checks the code's first byte against its own hash before bothering
the TV, so a typo fails locally). The TLS client certificate must be RSA — the
pairing secret hashes both certificates' moduli — so the key comes from the
`rsa` crate and rcgen only wraps it. The identity lives in `atv-identity`
(0600) and is per host: pairing the Pi does not pair your laptop.

Keys and app launches prefer Remote v2 and fall back to ADB when unpaired,
because a slower remote beats no remote. App launching goes through
`RemoteAppLinkLaunchRequest` — the mechanism behind the Netflix and Prime
buttons on a physical remote.

**ADB** keeps everything with no equivalent in Remote v2: CEC (hence the TV's
input), detailed box state, and sideloading an APK.

### The ADB channel

The box runs plain Android 14 with network debugging enabled, so Maison talks
to it over **ADB, implemented natively in `adb.rs`** — no `adb` binary is
shipped to the Pi. The existing Rust crates all drive the local `adb` *server*
(a second daemon on :5037), which is exactly the dependency worth avoiding on
an ARMv6 Alpine box.

Authentication is the part worth knowing about. On connect the box sends a
20-byte token; the client signs it with RSA-2048 and answers. **The token is
already a SHA-1 digest, so it must be signed pre-hashed** — hashing it again
yields a signature the box silently rejects, after which every connection
falls back to re-sending the public key and re-prompting on screen.

The signing key is generated on first use and stored in `adb-key` (0600).
Generating 2048 bits takes tens of seconds on a Pi 1, so it happens off the
async runtime, and the television will show one "Allow USB debugging?" prompt
the first time the backend connects. Accept it once — with *always allow* — and
it never comes back.

Beyond the remote, the box is also how the television gets powered and routed:
it runs with `power_control_mode=broadcast` and `tv_wake_on_one_touch_play=1`,
so waking it asserts CEC One Touch Play (TV on, input switched) and sleeping it
broadcasts a CEC standby.

APKs can be sideloaded from the dashboard: the file goes over ADB's `sync:`
service to the box's temp directory, then through `pm install -r`. Uploads are
capped at 96 MB — the Pi has 512 MB and holds the payload in memory.

## IR remote (AirTies STB)

An AirTies AIR 7310T set-top box running its custom firmware
([uplg/BCM7231B2](https://github.com/uplg/BCM7231B2)) decodes the Ruwido IR
remote in hardware; its
`kird` daemon POSTs every key event to `POST /api/ir/key`, authenticated with
the `IR_API_TOKEN` machine token (constant-time compare, independent from the
JWT session auth; set the same token in the STB's `device/kird.conf`).

Buttons are configured from the web app: Télécommande. The page
draws the physical remote (mapped keys highlighted), captures a key by asking
you to press it (it polls `GET /api/ir/recent`), and each binding holds an
ordered list of actions. One button can drive several devices, and a Test
button dry-runs the actions without saving.

Available actions, all "reversible" where it makes sense (`on` / `off` /
`toggle`, toggle reads the device's current state and flips it):

- Nabaztag command (garenne grammar: `dance`, `chor /vl/config/chor/taichi.chor`, `ears 8 8`, …)
- Zigbee lamp power (on/off/toggle) and brightness (0-254, `repeat` fires while the button is held)
- Meross plug power (on/off/toggle)
- Broadlink saved IR code
- Mitsubishi AC toggle: turns the AC on with structured settings
  (mode/temperature/fan/vane pickers) or off if the last commanded state was
  on. The blast is delayed ~1.2 s so the remote's own IR repeats cannot
  collide with the RM4 transmission at the AC's receiver.

The keymap persists in `ir-keymap.json` (server-side, survives reboots and
deployments). Phantom double-presses from marginal IR reception are debounced
server-side (1.2 s per key); unmapped keys return 200 and are logged
(`unmapped IR key`) so new buttons are easy to discover.

## Roller shutters (Matter)

The Sonoff Orb-RBS is a Wi-Fi Matter switch (ESP32) exposing a standard
Window Covering cluster. `matter.rs` makes Maison a Matter **commissioner** in
pure Rust ([`matter-controller`](https://crates.io/crates/matter-controller)):
it holds its own fabric, adds the switch to it from a pairing code, then
drives it over encrypted CASE sessions on the LAN. No eWeLink account, no
cloud, no Home Assistant.

Pairing goes **over IP**: the Pi build has no Bluetooth, so the switch must
already be on the Wi-Fi. The standard Matter "multi-admin" route does that:

1. Wire the switch (neutral required), hold its button 5 s to enter pairing
   mode, and add it with a phone ecosystem (Google Home, Apple Home, or the
   eWeLink app) using the Matter QR code on the device or in the quick guide.
   This is only what puts it on the Wi-Fi.
2. Calibrate the travel (hold 10 s, LED breathing): percentage positions need
   it, open/stop/close do not.
3. In that app, open the switch's "share with another Matter app / pairing
   mode" screen (Google: *Linked Matter apps & services → Link apps & services*;
   Apple: *Turn On Pairing Mode*). It shows an 11-digit code for a few minutes.
4. Dashboard → Volets → Ajouter: paste the code, pick a name. Maison
   discovers the switch over mDNS, verifies its attestation (DAC chain against
   the CSA production roots in `matter-trust/`), installs its own operational
   certificate and finds the Window Covering endpoint.

The phone ecosystem can stay as a second admin or be removed afterwards;
Maison does not depend on it. Removing a shutter in Maison sends
`RemoveFabric`, so the switch frees that slot.

**Wrong way round, or the position is off?** Swap the two motor wires if Open closes, then
let the switch learn its travel again (hold its button about 10 s until the LED breathes, then
let it run a full open and close): the position Maison shows is the switch's own estimate.

**Follow the sun.** Each shutter can open at sunrise and close at sunset, each shifted by up
to ± 3 h (closing defaults to an hour after sunset). The house's town is looked up once by name
(Open-Meteo geocoding, the only network call); sunrise and sunset are then computed on the Pi
(NOAA equations, `sun.rs`), so the shutters still move when the internet is down. A look every
30 s sends the latest open or close that fell due since the previous look, one command per
shutter, looking back 5 min at most: nothing is caught up after a restart or when NTP steps the
clock (the Pi boots on the time it last saw), and nothing moves while the clock is unset. The tile shows the next move
(« Fermeture 20:24 »). State: `matter/covers.json` (schedules), `matter/place.json`.

Positions follow the cluster but are flipped for the UI: Matter counts
*closure* in hundredths of a percent (0 = open), while the API and dashboard
show `openPercent` (100 = open).

Notes:

- Fabric private keys live in `matter/controller.bin` (0600, directory 0700).
  `deploy.sh` creates the directory for the service user. The controller
  saves by temp-file + rename, so a pre-created file in the root-owned app
  directory would not do.
- The fabric is created on the first pairing, not at boot: the Pi has no RTC,
  and certificates minted before NTP has set the clock would be dated 1970.
- If a newly bought device fails attestation, refresh the roots with
  `scripts/update-matter-trust.sh` (mirror of the CSA ledger kept in
  connectedhomeip).
- Live round trip (pair → close → 40 % → open → rename → reload → remove)
  against a real switch or a simulator:
  `MATTER_LIVE_CODE=<code> cargo test --features live-runtime-tests --test matter_live -- --nocapture`.
  For a matter.js / chip example device add `MATTER_TEST_ROOTS=1` (CSA test
  DAC). On macOS the simulator's `node` must be allowed through the
  application firewall, or the device never sees PASE.

## Prerequisites

- `bun` for the web app (and Node, which SvelteKit's tools run on)
- Rust and `cargo` for the backend (`cargo-zigbuild` for the Pi cross-build)
- a compatible Zigbee USB dongle for Zigbee support, for example a Sonoff Dongle Lite MG21 (`adapter: ember`)

## Raspberry Pi 1

For Raspberry Pi 1 deployments, the intended setup is fully host-native:

- run Mosquitto directly on the Pi
- plug in the Zigbee USB coordinator (Sonoff MG21-based dongle); the backend drives it natively over EZSP
- build the web app once, then let the Rust backend serve `web/build`
- run the cross-built musl release binary (no Bluetooth: `--no-default-features`)
- set `DISABLE_BLUETOOTH=true`
- set `AUTH_COOKIE_SECURE=false` if the Pi is exposed only over plain HTTP on the LAN

Deployment notes and host-native service files are in `docs/raspberry-pi-1.md`, `deploy/openrc/maison`, `deploy/openrc/cloudflared-maison`, and `deploy/mosquitto/maison.conf`.

There is also a one-shot deployment helper for the Pi: `deploy.sh`.
It supports `all`, `build`, `push`, `upgrade`, `start`, `stop`, `status`, and `logs`.
It also accepts `PI_PASSWORD` for password-based SSH when `sshpass` is installed locally.

The Raspberry Pi 1 target now assumes Alpine Linux with OpenRC and a musl backend build.

For first boot without screen or keyboard, use `scripts/flash-alpine-headless-macos.sh` and `docs/alpine-headless-flash-macos.md`.

Zigbee is driven natively by the Rust backend over EZSP (serial dongle); there is no Zigbee2MQTT or Node.js layer.

## Environment

```bash
cp .env.example .env
```

Main settings:

- `PORT` / `API_PORT`: Rust backend port, default `3033`
- `JWT_SECRET`: auth signing secret
- `FRONTEND_DIST_DIR`: the built web app served by the backend when `index.html` exists, default `web/build`
- `DISABLE_BLUETOOTH`: set `true` to disable Hue BLE support
- `ZIGBEE_SERIAL_PORT`: serial path of the Zigbee USB dongle
- `ZIGBEE_ADAPTER`: adapter type, `ember` for MG21/EZSP dongles such as the Sonoff Dongle Lite MG21
- `IR_API_TOKEN`: machine token for the STB IR bridge (empty/unset disables `/api/ir/key`)
- `MATTER_STATE_DIR`: Matter fabric + paired shutters directory, default `matter/`
- `MATTER_TRUST_DIR`: attestation roots (`paa/`, `cd/`), default `matter-trust/`
- `MATTER_TEST_ROOTS`: `true` to accept CSA test devices (simulators) instead of production ones; development only
- `AUTH_COOKIE_NAME`: session cookie name
- `AUTH_COOKIE_SECURE`: keep `true` when the app is exposed through HTTPS/Cloudflare
- `PUBLIC_URL`: where Maison is reached, the passkeys' origin (its host is their RP ID); defaults to `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`. Locally: `http://localhost:5173`
- `CLOUDFLARE_TUNNEL_TOKEN`: optional token for the Cloudflare tunnel profile
- `CLOUDFLARED_PROTOCOL`: Cloudflare transport protocol, default `http2` for better compatibility behind NAT
- `CLOUDFLARE_PUBLIC_HOSTNAME`: optional stable public hostname, for example `home.example.com`

## Security notes

- `JWT_SECRET` must be set to a strong unique value; the backend now refuses to start with the default secret.
- No passwords: signing in is by passkey only (WebAuthn, Ariane's implementation; notes in
  `docs/dependances/passkeys.md`). One button, no name to type: the device offers its passkey
  (Face ID, Touch ID, a PIN) and the server checks the signature.
- Passkeys need HTTPS and a domain: open Maison at `https://home.kahn.studio` (the tunnel),
  also at home. `http://192.168.1.103:3033` cannot sign in.
- Entry is by invitation: a one-time link, valid 7 days, only its hash kept. An admin makes
  one from « Mon compte → Inviter quelqu’un »; the very first one (or a way back in) comes
  from the Pi:

  ```bash
  ssh root@192.168.1.103 'cd /opt/maison && ./backend/target/release/maison-backend invite leonard --name Léonard --admin'
  ```

  An invitation for someone who exists already adds a passkey (lost phone): same person,
  same role. Admins invite; members do everything else.
- « Mon compte » lists my passkeys (rename, remove all but the last, add one from this
  device) and signs me out everywhere.
- Sessions: an `HttpOnly` access cookie (15 min) and a rotating refresh cookie (7 days); a
  refresh reads the person again.
- Failed passkey attempts are limited per address (10 a minute), ceremonies started signed
  out too (30 a minute); behind cloudflared, the address is `CF-Connecting-IP`.

## Development (this machine)

```bash
make backend         # cargo run, backend on :3033
make web             # SvelteKit dev server, proxies /api to :3033
make test            # backend tests + web check + Vitest
```

## Deployment (Raspberry Pi 1)

Everything goes through `deploy.sh`, wrapped by Make targets. `PI_HOST` is
read from `.env`.

```bash
make deploy              # build (web app + ARMv6 musl backend) + push + restart services
make deploy-status       # service states, versions, URLs
make deploy-logs         # follow logs (LOG_TARGET=stack|backend|mosquitto|cloudflared)
make cloudflared-upgrade # rebuild latest cloudflared for ARMv6 and swap it on the Pi
```

The Pi runs three OpenRC services: `mosquitto` (TLS :8883 for the Meross
plugs), `maison` (the backend, which also serves the web app), and
`cloudflared-maison` (the tunnel, when `CLOUDFLARE_TUNNEL_TOKEN` is set in
`.env`; set `CLOUDFLARE_PUBLIC_HOSTNAME` for the public URL).

Full instructions are in `docs/raspberry-pi-1.md`.

## Validation

- Web build: `bun --cwd web run build`
- Backend tests: `cargo test --manifest-path backend/Cargo.toml`
- Minimal Pi-oriented backend check: `cargo check --manifest-path backend/Cargo.toml --no-default-features`

### Planned

- Matter bridge (but will not handle cats-related devices such as litter as it's not yet in the specification.)

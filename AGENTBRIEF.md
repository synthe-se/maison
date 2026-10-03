# Maison — Architecture Brief

## 1. Project identity

- **Name**: "Maison" (crate `maison-backend`, service `maison`)
- **Purpose**: self-hosted home-automation dashboard — cat devices (feeder,
  fountain, litter box), lamps (Hue BLE, Zigbee), smart plugs (Meross),
  roller shutters (Matter, Sonoff Orb-RBS), IR
  climate control (Broadlink → Mitsubishi AC), and French Tempo electricity
  tariff tracking/prediction.
- **Production target**: Raspberry Pi 1 (Alpine Linux, OpenRC, musl,
  `--no-default-features` build without Bluetooth). Development on macOS.

## 2. Repository layout

```
maison/
├── backend/                 # Rust backend (Axum 0.8, tokio)
│   ├── src/
│   │   ├── main.rs          # entrypoint, dotenvy, graceful shutdown;
│   │   │                    #   `maison-backend invite <id> --name X [--admin]`
│   │   ├── lib.rs           # AppState, router assembly, layers (CORS,
│   │   │                    #   security headers, 180s TimeoutLayer, trace)
│   │   ├── config.rs        # env-driven Config (paths, auth, zigbee)
│   │   ├── error.rs         # AppError (thiserror + IntoResponse)
│   │   ├── auth.rs          # JWT extractors (signed in / admin / machine), refresh store
│   │   ├── passkey.rs       # WebAuthn (webauthn-rs): ceremonies, limits, RP from PUBLIC_URL
│   │   ├── people.rs        # auth/auth.json: people, passkeys, invitations (no passwords)
│   │   ├── tuya.rs          # Tuya local TCP (feeder/fountain/litter box)
│   │   ├── meross.rs        # Meross plugs via LOCAL HTTP (reqwest) — the
│   │   │                    #   backend does NOT speak MQTT; the plugs'
│   │   │                    #   firmware needs a reachable TLS broker on
│   │   │                    #   :8883 to boot, hence the Mosquitto service
│   │   ├── hue.rs/hue_stub.rs # Philips Hue BLE (btleplug, feature "bluetooth")
│   │   ├── broadlink.rs     # Broadlink IR manager + persisted climate state
│   │   ├── mitsubishi_ir.rs # Mitsubishi AC IR frame encoder (see §5)
│   │   ├── matter.rs        # Matter commissioner + window coverings
│   │   ├── zigbee.rs        # Zigbee lamp manager (native EZSP only)
│   │   ├── zigbee_native.rs # EZSP/EmberZNet driver (see §4)
│   │   ├── tempo/           # RTE colours, EDF quotas, prices, RTE-rule forecast (docs/tempo.md)
│   │   └── routes/          # one module per domain, all JWT-authenticated
│   │                        #   except /health
│   └── tests/               # integration tests + fixtures
├── web/                     # SvelteKit 3 SPA (Svelte 5 runes, Bits UI, Paraglide, Bun)
│   ├── src/lib/components/  # the shared bricks: DeviceTile, Range, Select, Tabs, Sheet,
│   │                        #   ConfirmDialog, PageHead, Toggle, Icon, Header…
│   ├── src/lib/devices/     # one folder per device family (lamps/, cats/, tv/, tempo/…)
│   ├── src/lib/*.svelte.ts  # live (server state), gesture, command, session, ui, i18n
│   ├── src/styles/          # tokens.css (light-dark()) + app.css
│   └── scripts/             # i18n.check, css.check, imports.fix
├── i18n/                    # messages/{fr,en}.json (inlang), the only source of texts
├── e2e/                     # Playwright + axe scenarios against the real backend
├── cache/tempo/             # Tempo seasons, model.json (fit_tempo), netload.json, weather.json
├── deploy/                  # OpenRC units, mosquitto conf
├── docs/                    # Pi setup, Tempo (docs/tempo.md), flashing
├── mosquitto/               # broker config + certs (Meross TLS :8883)
├── scripts/                 # Pi cross-build (zigbuild), IR capture, flash
├── Makefile                 # dev targets + Pi deploy wrappers + cloudflared upgrade
├── deploy.sh                # one-shot Pi deployment helper
└── *.json                   # runtime state (devices, lamps, users,
                             #   broadlink-codes, climate-state, …)
```

There is **no zigbee2mqtt integration** (removed 2026-08): no MQTT client in
the backend, no `rumqttc`, no Z2M config. Mosquitto stays solely because
Meross plug firmware requires a reachable TLS broker.

## 3. Zigbee stack (native EZSP)

Dependency chain (all pinned in `backend/Cargo.toml` + lockfile):

- `ashv2` — **uplg fork, branch `main`** = upstream
  (PaulmannLighting) v13 + robustness fixes not yet upstream (old v12 lineage in `legacy-v12`): transmitter
  self-requeue deadlock removed (local pending queue + housekeeping tick),
  frame-number reset after RST/RST-ACK, receiver exit on fatal serial errors,
  active retransmission of timed-out DATA frames, duplicate-retransmission
  payload dedupe. Transport-agnostic (AsyncRead/AsyncWrite); the backend
  opens the port with `tokio-serial`. The `ezsp` cargo feature provides the
  `Transmit`/`Receive` adapters.
- `ezsp` 17 — **uplg fork, branch `main`** (old 15 lineage in `legacy-v15`), wired through
  `[patch.crates-io]` so both the direct dep and ashv2's internal dep
  resolve to it. Two fork patches: `importTransientKey` drops the
  SecManContext prefix when the negotiated protocol is < v14 (the EZSP ≤ v13 wire
  format; v14+ NCPs get upstream's layout), and the receiver never blocks response
  routing on a full callback channel (upstream can deadlock there) because the Sonoff
  Dongle Lite MG21 firmware line is EmberZNet 7.4.x = EZSP v13. If the
  dongle ever runs EZSP ≥ v14 firmware, drop this patch and the
  `[patch.crates-io]` entry to run vanilla crates.io ezsp.
- `silizium` 3 — crates.io (security manager types).

Driver design (`zigbee_native.rs`):

- One driver task owns the pipeline: tokio-serial stream → `ashv2::start`
  (2 spawned actor futures) → `ezsp::Client::run` (2 more futures) →
  `Connection` + bounded callback channel. `PipelineTasks` tracks the four
  `JoinHandle`s for liveness (`is_alive`) and bounded teardown (abort+join).
- EZSP protocol version: desired from `ZIGBEE_EZSP_PROTOCOL_VERSION`
  (default 13); on `ProtocolVersionMismatch` the pipeline is rebuilt once
  with the version the NCP announced (firmware upgrades need no config).
- HTTP handlers reach the driver via a bounded command channel; both the
  enqueue and the reply wait are bounded (`COMMAND_REPLY_TIMEOUT`) so no
  request can hang. Touchlink scans get a longer per-command timeout.
- **The driver never dies permanently**: reconnection retries forever with
  capped exponential backoff; past `MAX_RECONNECT_ATTEMPTS` the lifecycle
  reports `Failed` (API fails fast with the reason) while retries continue
  in the background. Watchdog (`WATCHDOG_TIMEOUT`) plus per-command,
  per-callback, and network-bring-up timeouts guard the event loop; any
  breach tears down and rebuilds the pipeline.
- `zigbee.rs` layers lamp persistence (`zigbee-lamps.json`, blacklist) and
  the HTTP-facing views on top of the driver snapshots.

## 4. Climate (Broadlink → Mitsubishi AC)

- `mitsubishi_ir.rs` encodes 18-byte Mitsubishi frames from a command
  grammar: `state-<mode>-<temp>-fan-<fan>-vane-<vane>[-wide-…][-econo-…]
  [-stop-HH-MM][-stopin-<minutes>][-timer-off]` or `state-off`.
- The unit's clock byte is set from the backend's local time on every send
  (the frame resets the AC clock, so absolute timers only work when the
  clock is transmitted). `stopin-<minutes>` is the relative sleep timer the
  UI uses ("turn off in 1h/3h", 10-minute granularity, wraps midnight).
- The last commanded state is persisted (`climate-state.json`, config
  `CLIMATE_STATE_JSON_PATH`) and exposed at `GET /api/broadlink/mitsubishi/
  state`; the frontend form restores it once on mount (`lastOnCommand`
  parsed back into form state, one-shot hydration).

## 5. Other integrations

| Domain | Transport | Notes |
|---|---|---|
| Tuya (feeder/fountain/litter) | local TCP (rust-async-tuyapi fork) | device cache persisted |
| Meross plugs | local HTTP | broker on :8883 only for plug firmware boot |
| Hue lamps | BLE (btleplug), feature-gated | stub on Pi builds |
| Broadlink | UDP discovery + IR send/learn (rbroadlink) | codes in broadlink-codes.json |
| Tempo | RTE (open data, or the OAuth API with `RTE_CLIENT_ID`), api-couleur-tempo fallback, EDF quotas, data.gouv prices, ODRE éCO2mix, Open-Meteo | RTE's published rule + Monte Carlo to J+7; `model.json` refit yearly on the Mac (`fit_tempo`), never on the Pi |
| IR remote (AirTies STB) | HTTP in: the STB's `kird` daemon POSTs Ruwido key events to `/api/ir/key`, bearer-authed with `IR_API_TOKEN` (machine token, `MachineClient` extractor, deliberately not chained with JWT auth) | keycode→action map in `ir-keymap.json` (mutable server-side state, edited via the /remote configurator, never pushed by deploy.sh). Presses are debounced 1.2 s per key (phantom doubles from marginal IR); climate_toggle waits 1.2 s before blasting (remote/RM4 IR collision). Unmapped keys return 200 and land in `/api/ir/recent` for capture |
| Philips TV (Saphi) | JointSPACE over plain HTTP :1925 (`pairing_type: "none"`, no auth) + Wake-on-LAN + DIAL on the Android box | `tv.rs`. **The endpoint whitelist is load-bearing**: unimplemented paths (`/6/sources`, `/6/applications`, `/6/activities/*`, `/5/*`) kill the single-threaded server *persistently* — only a mains power cycle revives it — so endpoints are an enum, not strings, and every call goes through one gate holding `MIN_REQUEST_GAP`. Two sleep depths: light standby answers on 1925, deep standby needs WoL first (~20 s, revives the network only; the panel needs a following `powerstate: On`). Saphi cannot switch sources, so `switchToBox` wakes the box over DIAL :8008 and lets CEC One Touch Play route the input. Config in `tv.json`; live tests behind `live-runtime-tests` |
| Android TV box — keys & app launches | **Remote v2** over mutual TLS :6466 (`atvremote.rs`), ADB as fallback | The fast path. ADB costs ~150 ms per key, of which only 65 ms is ADB — the rest is `input` booting a JVM per press. Remote v2 sends a protobuf on an open session, and a background task owns that session so the handler returns in µs instead of blocking. Framing is varint-length + protobuf, hand-encoded (no prost/protoc for two small schemas). **Pairing is per host and the client cert must be RSA** — the secret hashes both certs' moduli — so the key comes from `rsa` and rcgen only wraps it (ring cannot generate RSA). The TV's cert is accepted unverified on purpose: self-signed, no verifiable name, and auth runs the other way. `protocol_version` must be 2 and `service_name` `atvremote`, or the TV answers STATUS_ERROR. Identity in `atv-identity` (0600); pairing routes at `/androidtv/pair/{start,finish}` |
| Android TV box (MECOOL LEAP-S1) | native ADB over TCP :5555 (`adb.rs`, no `adb` binary) | `androidtv.rs`. Auth: the box's 20-byte token **is** a SHA-1 digest and must be signed pre-hashed (`sign_prehash`, not `Signer::try_sign`) — hashing twice makes the box reject the signature and fall back to re-sending the public key, re-prompting on screen every connection. Key generated on first use into `adb-key` (0600, `spawn_blocking` — tens of seconds on a Pi 1), one on-screen authorisation ever. Connection is kept and reused, one stream at a time; keys are an enum and package names are validated, so nothing reaches the shell unchecked. APK sideloading goes through the `sync:` service then `pm install -r`, capped at 96 MB. CEC lives here too: waking the box does One Touch Play (TV on + input), sleeping it broadcasts standby. Config in `androidtv.json`; live tests behind `live-runtime-tests` |
| Matter window coverings (Sonoff Orb-RBS) | Matter over IP (`matter-controller` 0.16, pure Rust on `ring`, own `mdns-sd` responder; IPv6 socket, needs IPv6 on the Pi) | `matter.rs`. Maison is the commissioner: one fabric, created lazily on the first pairing (no RTC on the Pi: minting certs before NTP would date them 1970). Pairing takes a multi-admin code from a phone ecosystem, since the Pi has no BLE to put a device on the Wi-Fi. Attestation is checked against the CSA production PAA/CD roots vendored in `matter-trust/` (`scripts/update-matter-trust.sh`); `MATTER_TEST_ROOTS=1` swaps in the CSA test roots for simulators. State in `matter/` (dir 0700: `controller.bin` keys saved by temp+rename, `covers.json` names/endpoints). The controller is built on first use, not at boot. Positions are flipped: the cluster counts closure in 1/100 % (0 = open), the API exposes `openPercent`. No software reversal: a switch that runs backwards gets its motor wires swapped (the Orb-RBS accepts the WC `Mode` « motor reversed » bit but does not act on it). Sun schedule per cover (`SunSchedule`: open at sunrise / close at sunset ± offset) computed locally by `sun.rs` from the place in `matter/place.json` (found by name via Open-Meteo geocoding); `run_schedule` every 30 s sends, per cover, the latest event that fell due since the last look, looking back `CATCH_UP` (5 min) at most (`due()` is pure and tested): the Pi boots on swclock's saved time and NTP then steps weeks ahead, which once replayed every sunrise/sunset in between. Removal sends `RemoveFabric` (response may never arrive, the session dies with the fabric) then forgets the node. Live round trip in `tests/matter_live.rs` behind `live-runtime-tests` |
| Nabaztag (garenne) | UDP 9998 `grn1 ` control + GET /status | rabbit runs clapier's garenne firmware; clapier server is hosted on the same Pi (/opt/clapier, OpenRC). `nabaztag.rs` pushes the daily Tempo colors every 15 min: today = static belly LED (`led 2 RRGGBB`), tomorrow = ear position (both ears when RTE published it; a forecast ≥ 80 % moves the left ear only, the right one tilted to 4). Config: `NABAZTAG_HOST` (rabbit IP) + nabaztag.json. All firmware tooling lives in uplg/nabgcc (branch portal-ui); nothing firmware-related remains here. |

## 6. Web app notes

- SvelteKit **3** (not 2): imports are `#lib/...` WITH the file extension
  (`bun scripts/imports.fix.ts` adds missing ones); public build-time values live in
  `src/env.ts` (`defineEnvVars`), read by `app.html`'s `%sveltekit.env.*%` and
  `$app/env/public` (the old `process.env` trick yields empty values). TypeScript 6
  (svelte-check does not run on TS 7 alone).
- Design: Ariane's rules and tokens (`docs/ux/mise-en-page.md` there), Maison's own in
  `docs/ux/tableau-de-bord.md`. Colours are written once each with `light-dark()`;
  `data-theme` on `<html>` (set before first paint from the key `PUBLIC_THEME_KEY`) forces a side.
- Data: `live(key, fetch, every)` — one shared value per key, polled while mounted and visible.
  Gestures: `Gesture.run(send, then, key)`; device on/off: `Command` (optimistic target,
  « Allumage… » after 1 s, « Pas de réponse » at `LIMIT.lamp|plug|tv`).
- Texts: Paraglide from `../i18n`; `bun run check` enforces parity, use, no duplicates.
- No service worker on purpose (live state; the React app had removed it too); `legacy.ts`
  unregisters the old `/sw.js`.
- Tests: Vitest 5 (browser project in Chromium for `*.svelte.test.ts`, node project for the
  rest), `e2e/` for scenarios. See README « Tests ».

## 7. Operational notes

- Production = Raspberry Pi 1 only (Alpine 3.24, sys mode, OpenRC — no
  Docker anywhere). Dev machine runs `make backend` / `make web`;
  deployment goes through `deploy.sh` (wrapped by `make deploy*`), and
  `make cloudflared-upgrade` rebuilds/swaps the ARMv6 tunnel binary.
- Runtime JSON state lives at the repo root (gitignored where mutable).
- Sign-in is by passkey only, Ariane's implementation (`passkey.rs`,
  `people.rs`, `routes/passkeys.rs`, web `passkeys.ts`; notes in
  `docs/dependances/passkeys.md`). People exist once an invitation has
  registered their first passkey; the first invitation comes from
  `maison-backend invite` on the Pi. `auth/auth.json` is read on every use
  and replaced by temp+rename in `auth/` (dir 0700, the service's: the app
  dir is root's), keeping the owner (root's CLI writes it for the service). RP ID = host of `PUBLIC_URL` (default `https://` +
  `CLOUDFLARE_PUBLIC_HOSTNAME`): never an IP, so the LAN address cannot sign
  in. Roles: `admin` invites, `member` does the rest. Coded refusals
  (`AppError::Coded`, `{error, code}`) let the web app word them; no 401
  but « not signed in » (the web app reads a 401 as a session that ended).
  The backend refuses a JWT secret under 32 bytes or a placeholder.
- Sessions (`auth.rs`): the JWT says only who and when (`issued_ms`,
  `auth_ms`); name and role come from `people.rs` on every request
  (cached by file stamp), `not_before_ms` per person ends every session at
  once. Refresh tokens are stored hashed, spent atomically (`take`), and a
  replayed one ends its family. Cookies are `__Host-` when secure.
- `lib.rs`: device routers sit under one `AuthenticatedUser` route layer
  (handlers never repeat it); `AdminUser` marks configuration routes.
  `guard_request` refuses LAN peers (non-loopback) except `/health` and
  `/api/ir/key`, and cross-site changes (`Sec-Fetch-Site`, else `Origin` vs
  `Host`). Invitation tokens are redacted from the request span.
- Every JSON state file goes through `store.rs` (atomic write, 0600 when
  private, owner kept, empty/torn = error unless a cache, cross-process
  `locked`); small helpers in `util.rs`.
- Background jobs go through `every()` in `lib.rs`: a panicking run is
  logged and the job keeps its schedule.
- Every route sits behind a global 180s timeout layer; failed passkey
  attempts are limited per address.

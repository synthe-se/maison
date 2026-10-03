# Maison: agent and contributor brief

Self-hosted home dashboard. One Rust binary (`maison-backend`, Axum 0.8 + tokio) that drives
every device on the LAN and serves a SvelteKit SPA. Production: a Raspberry Pi 1 (ARMv6,
512 MB, Alpine, OpenRC, musl, built `--no-default-features`: no Bluetooth), behind a
Cloudflare tunnel. Development on macOS. Deploying: [docs/deploy.md](docs/deploy.md).

## Map

```
backend/src/
  main.rs            entrypoint; `maison-backend invite <id> --name X [--admin]`
  lib.rs             AppState, router, layers: security headers + CSP (SHA-256 of the
                     page's inline scripts), cache policy, guard_request, 180 s timeout,
                     `every()` / `supervised()` for background jobs (a panic is logged, the job
                     goes on; the JoinHandle aborts it), the app constructors
  config.rs          env → Config (paths under MAISON_SOURCE_ROOT, default the repo root)
  error.rs           AppError; `AppError::Coded {code}` = a refusal the web app words itself
  store.rs           every JSON state file: read, atomic write, owner kept, `locked`
  util.rs            shared helpers (hex, random secrets, hashing, `HOUSE_TZ` = Europe/Paris:
                     never `chrono::Local`, which is UTC on the Pi; `test_dir()` for unit tests)
  auth.rs            sessions: AuthenticatedUser, AdminUser, MachineClient (IR token)
  passkey.rs people.rs routes/passkeys.rs   passkeys, people, invitations (docs/passkeys.md)
  tuya/              feeder, fountain, litter box (local TCP, uplg fork of rust-async-tuyapi):
                     mod (manager), worker (one supervised session per device), cache, parse,
                     feeder (meal-plan codec)
  meross.rs          plugs over local HTTP (Mosquitto exists only for their firmware's boot)
  hue/ hue_stub.rs lamps.rs     Hue over BLE (feature `bluetooth`: mod, ble = GATT, store = kept lamps), what lamp families share
  zigbee/            Zigbee lamps and Hue dimmer, native EZSP driver: mod (manager), driver
                     (task, event loop), network, context, commands, callbacks, discovery,
                     availability, remotes, touchlink, zcl (frames), device, error, config
  matter/ sun.rs     Matter commissioner and shutters (mod), sun schedule (schedule: pure,
                     tested with an injected clock), sunrise/sunset and place search (sun)
  broadlink.rs broadlink_ir.rs mitsubishi_ir.rs philips_ir.rs   IR blaster, AC and TV frames
  tv.rs              Philips TV (JointSPACE)
  androidtv.rs adb.rs atvremote.rs   Android TV box: native ADB, Remote v2
  ir.rs              set-top-box remote → actions (keymap)
  nabaztag.rs        Tempo colors on the rabbit (garenne)
  json_config.rs     a settings file kept in memory and on disk (TV, box, rabbit, IR codes):
                     `Checked` on load and set, `update` = copy, change, save, swap under one lock
  net.rs             every HTTP client (timeouts; devices: no redirects), LAN device address
                     checks (SSRF gate), `unreachable` (the error stays in the log)
  tempo/             colors, quotas, prices, forecast (docs/tempo.md)
  routes/            one module per domain
  bin/               fit_tempo (Mac only), send_ir, atv_pair, decode_mitsubishi_ir, tuya_probe
backend/tests/       integration tests (+ `live-runtime-tests` feature for real devices)
web/src/lib/         live (sources), gesture (pending, unavailable), command, draft, elapsed,
                     stored, options, session, ui (toasts), i18n (every format), focus, errors,
                     api (the fetch core: errors, renewal, upload), passkeys (session, people,
                     invitations, passkeys), haptics
web/src/lib/components/   every shared brick, each Bits UI primitive wrapped once (Group,
                     DeviceTile + TileSettings, Loaded + Stale, ToggleGroup, Disclosure,
                     Combobox, Range, Select, Sheet, ListRow, StatusRow…)
web/src/lib/devices/      one folder per device family: api.ts (its types and endpoints), data.ts
                     (its live sources), its components; a dashboard group is `<Family>Group`,
                     a page body `<Thing>Page` (a part of one `<Thing>Panel`), a row `<Thing>Tile`
web/src/lib/account/      the account page's sections
web/src/styles/      tokens.css (light-dark()), app.css
web/scripts/         i18n.check, i18n.add, css.check, imports.fix
i18n/messages/       fr.json, en.json: the only source of texts
e2e/                 Playwright + axe against the real backend (run.sh, house.ts: typed with the
                     app's types, built from web/src/lib/test fixtures; words from fr.json)
cache/tempo/         seasons, model.json (fit_tempo), netload.json, weather.json
deploy/ deploy.sh Makefile scripts/ bootstrap/   Pi build, deploy, flash
matter-trust/        CSA attestation roots (tracked)
docs/                deploy.md, tempo.md, passkeys.md, ux.md
```

Runtime state lives at the repo root (gitignored): `devices.json`, `meross-devices.json`,
`device-cache.json`, `hue-lamps*.json`, `zigbee-lamps*.json`, `climate-state.json`,
`broadlink-codes.json`, `ir-keymap.json`, `tv.json`, `androidtv.json`, `nabaztag.json`,
`refresh-tokens.json`, `adb-key`, `atv-identity`, `auth/`, `matter/`.

## Rules

- **DRY, absolutely: one logic, one place.** Before writing, find where it already lives. This
  applies to agents too: no parallel helper because the existing one was not found.
- **Every state file goes through `store.rs`** (temp file + fsync + rename, 0600 when private,
  owner kept, empty or torn = error unless a cache, cross-process `locked`); from async code,
  `write_json_async`. Small shared helpers go in `util.rs`.
- **The network through `net.rs`**: no `reqwest::Client` built elsewhere; a device address is
  checked once where it comes in (`net::device_host`, a `Checked` settings file); a device's
  or library's error reaches the client only as `net::unreachable` (« X unreachable »).
- **The HTTP API is camelCase English**: every JSON key a route reads or answers is camelCase
  (`#[serde(rename_all = "camelCase")]` on the type, never field-by-field renames), in English
  (`tariffs`, `startsOn`, `offPeak`/`peak`). Values keep their own spelling: enum values and
  codes snake_case (`tv_power`, `deep_standby`, `not_signed_in`), Tempo colors `BLUE`/`WHITE`/
  `RED`, map keys that are data (keycodes, Tuya data points). Every integration-test answer is
  checked (`util::casing_offences`, from `tests/common` `respond`). **Persisted files keep
  loading their old shapes**: a renamed field of a stored type keeps its old name as a serde
  `alias` (the keymap, scenes, `model.json`), and is written in the new casing.
- **Refusals are coded**: `AppError::Coded` with a `code` the web app maps to words
  (`web/src/lib/errors.ts`). 401 means only « not signed in » (the app reads it as a
  session that ended).
- **Auth is a route layer**: device routers sit under one `AuthenticatedUser` layer, handlers
  never repeat it; configuration routes take `AdminUser`. The IR bridge uses `MachineClient`.
- **Tests for everything**: Rust unit + integration, Vitest for every component and module
  (`*.svelte.test.ts` in Chromium, the rest in Node), e2e for every flow. `cargo test` must
  not touch the repo or the network (temp dirs, local stubs): integration tests build their
  config with `common::isolated_config(name)` only (a temp root removed when the test ends),
  unit tests use `util::test_dir()`.
- **Bits UI for every interactive widget**, wrapped once in `components/`. A gesture goes
  through `Gesture` (a control that cannot act: `unavailable(reasonId)`, never `disabled`), an
  on/off through `Command`, server state through `live(source)` with each source (key, fetch,
  pace) declared once in its family's `data.ts` (live() refuses a key declared with two
  fetches), a form's changes through `Draft`, every slider is `Range`, every choice `Select`
  or `ToggleGroup`, every page head `PageHead`, every group `Group`, a tile's settings its
  `settings` snippet (TileSettings); dates, times, lists and window titles from
  `#lib/i18n.svelte.ts`, what the browser remembers through `#lib/stored.ts` (keys in
  `src/env.ts`). UX rules: [docs/ux.md](docs/ux.md).
- **Texts only from `i18n/messages`**. French UI copy addresses the person as « tu », with
  French typography (narrow no-break spaces). Code, comments and docs are in English.
- **Tokens only** (`web/src/styles/tokens.css`) for corners, control heights, font sizes,
  colors and the focus ring: `css.check.ts` refuses literal radii, control heights, px font
  sizes, rgb()/hex outside tokens.css, and outline literals. Strict CSP, never
  `'unsafe-inline'`.
- **Formatted and linted by oxc** (the owner's choice, not Prettier nor ESLint): `oxfmt`
  (tabs, single quotes, 140 columns, no trailing commas; `.svelte` through its Svelte plugin,
  CSS too) and `oxlint` (plugins typescript, unicorn, oxc, import, vitest, promise;
  categories correctness, suspicious and perf as errors, warnings refused) over `web/` and
  `e2e/`, configs in `web/.oxfmtrc.json` and `web/.oxlintrc.json`. Rules left off, on
  purpose: vitest `require-mock-type-parameters` (noise), `no-await-in-loop` (sequential on
  purpose: polls, e2e steps), import `no-unassigned-import` (stylesheets, fonts), unicorn
  `prefer-add-event-listener` (the XHR upload's own handlers), unicorn
  `consistent-function-scoping` (helpers stay next to their use), `no-underscore-dangle`
  (a framework global). `bun run check` runs both, then type-checks `e2e/` (its tsconfig).
- **Latest dependencies**, pinned exactly; git forks pinned by `rev`, never a branch.
- **Rust edition 2021**: no let-chains.
- SvelteKit 3: imports are `#lib/...` with the file extension (`bun scripts/imports.fix.ts`);
  public build-time values come from `src/env.ts` (`defineEnvVars`) via `$app/env/public`;
  TypeScript 6 (svelte-check fails on 7). No service worker on purpose (`legacy.ts`
  unregisters the old one). Vitest 5: `src/test-setup.ts` sets what `$app/env/public` reads outside Kit's dev server; `getByText` and `toHaveTextContent` are exact by default.

## Commands

```bash
make backend / make web / make test
cd web && bun run check               # i18n, css, oxfmt --check, oxlint, svelte-check, e2e types
cd web && bun run format              # oxfmt over web/ and e2e/ (`bun run lint`: oxlint alone)
cd web && bun scripts/i18n.add.ts '{"key": ["français", "English"]}'   # every locale at once
cd e2e && bun run all                 # after `cargo build` and `bun --cwd web run build`
cargo check --manifest-path backend/Cargo.toml --no-default-features   # the Pi's feature set
MATTER_LIVE_CODE=<code> cargo test --features live-runtime-tests --test matter_live -- --nocapture
```

## Gotchas learned the hard way

### The Pi

- No RTC: it boots on swclock's saved time, then NTP steps the clock, sometimes weeks. The
  Matter fabric is created on first pairing, not at boot (certificates minted before NTP
  would be dated 1970). The sun schedule (`run_schedule`, every 30 s) sends per cover only
  the latest event due since the last look, catching up 5 min at most (`CATCH_UP`; `due()` is
  pure and tested), and does nothing while the kernel says the clock is unsynced
  (`clock_trusted`). Once, an NTP step replayed every sunrise in between. A « not tonight »
  skip names its occurrence by time (`skipCloseAt`): one that passed while the Pi was off or
  unsure of the time skips nothing else and goes after 12 h.
- `/opt/maison` is `root:maison 1775`: the service can create and rename its own files but
  not replace root's. State is written by temp file + rename, so state that needs privacy
  gets its own 0700 directory (`auth/`, `matter/`), never a pre-created file in the app dir.
  `maison-backend invite` runs as root and writes `auth/auth.json`: `store.rs` keeps the
  service as owner.
- Generating a 2048-bit RSA key takes tens of seconds: off the runtime (`spawn_blocking`).
- APK uploads are capped at 96 MB (held in memory on 512 MB).

### Access

- Passkeys need a domain and HTTPS: the RP ID is the host of `PUBLIC_URL` (default
  `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`), never an IP. From the LAN the backend answers
  only `/health` and `/api/ir/key`; everything else comes through cloudflared on loopback.
  Client address = `CF-Connecting-IP` from loopback only.
- Cloudflare overrides the backend's `Cache-Control` unless the zone's Browser Cache TTL is
  « Respect Existing Headers ».
- e2e runs on `http://localhost`, not `127.0.0.1`: a secure context with a domain.
- `JWT_SECRET` under 32 bytes or a placeholder: the backend refuses to start.

### Zigbee (`zigbee/`)

- Dongle: Sonoff Dongle Lite MG21, EmberZNet 7.4.x = **EZSP v13**. `ashv2` and `ezsp` are uplg
  forks (`main` = upstream PaulmannLighting + fixes; `legacy-*` branches keep the old
  lineage), pinned by `rev`, `ezsp` through `[patch.crates-io]` so ashv2's internal dep
  resolves to it too. Fork patches: ASH transmitter deadlock, frame-number reset after RST,
  retransmission and dedupe; `importTransientKey` without the SecManContext prefix below v14;
  response routing never blocked on a full callback channel. On EZSP ≥ 14 firmware the
  importTransientKey patch can go.
- `ZIGBEE_*` are read once into `Config::zigbee` (`zigbee/config.rs`); a value that does not
  parse is logged and its default used. The protocol version comes from
  `ZIGBEE_EZSP_PROTOCOL_VERSION` (default 13); on a mismatch the pipeline is rebuilt once
  with the NCP's version.
- The driver never dies: one task owns serial → ASH → EZSP; bounded command channel and reply
  waits; watchdog and per-step timeouts tear down and rebuild; reconnects forever with
  capped backoff (reported `Failed` past `MAX_RECONNECT_ATTEMPTS`, retries continue).
- Errors are typed (`zigbee/error.rs`): only EZSP failures count toward a rebuild (5 in a
  row, or one transport error at once); an unknown lamp is a 404 in the manager and never
  reaches the radio. Clients never see the serial path nor a panic text.
- The driver publishes its devices on a watch channel; the manager syncs (and saves) only
  when they changed.
- No zigbee2mqtt, no MQTT client in the backend.

### Matter shutters (Sonoff Orb-RBS, `matter/`, `matter-controller`, pure Rust)

- Maison is the commissioner, pairing over IP with a multi-admin code from a phone ecosystem
  (no BLE on the Pi). Attestation against `matter-trust/`; `MATTER_TEST_ROOTS=1` for
  simulators. On macOS a simulator's `node` must be allowed through the firewall or PASE
  never arrives. Needs IPv6.
- The Orb-RBS accepts the Window Covering `Mode` « motor reversed » bit and ignores it: a
  shutter that runs backwards gets its motor wires swapped, then recalibrated (hold 10 s).
- Matter counts closure in 1/100 % (0 = open); the API and UI say `openPercent`.
- Removal sends `RemoveFabric` (its response may never come) then forgets the node.

### Philips TV (55PUS6753, Saphi, `tv.rs`)

- JointSPACE on plain HTTP :1925, no pairing. Unimplemented endpoints (`/6/sources`,
  `/6/applications`, `/6/activities/*`, `/5/*`) kill the server persistently: only unplugging
  the set for ~30 s revives it. Endpoints are an enum, every call goes through one gate
  spaced by `MIN_REQUEST_GAP`; the web app polls once a minute. Never widen without testing.
- Power goes over IR only (RC5 via the Broadlink, discrete `0x3F` on / `0x3D` off): deep
  standby drops the network (WoL and CEC fail). Saphi cannot switch sources: `switchToBox`
  wakes the box over DIAL and CEC One Touch Play routes the input.

### Android TV box (MECOOL LEAP-S1)

- Keys and app launches go over **Remote v2** (`atvremote.rs`: mutual TLS :6466, hand-encoded
  protobuf, one session owned by a task): ADB's `input` boots a JVM per key (~150 ms, ~10 s
  when the box sleeps). Pairing is per host; the client cert must be RSA (the secret hashes
  both moduli), so the key comes from `rsa` and rcgen only wraps it. `protocol_version` 2,
  `service_name` `atvremote`. Falls back to ADB when unpaired.
- **ADB** natively (`adb.rs`, no `adb` binary): the 20-byte auth token is already a SHA-1
  digest and must be signed **pre-hashed** (`sign_prehash`); hashing again makes the box
  re-prompt forever. Key in `adb-key` (0600), one on-screen approval ever. ADB keeps CEC, box
  state and APK sideloading (`sync:` then `pm install -r`).

### IR remote (AirTies AIR 7310T, custom firmware [uplg/BCM7231B2](https://github.com/uplg/BCM7231B2))

- Its `kird` daemon POSTs Ruwido key events to `/api/ir/key` with `IR_API_TOKEN`
  (constant-time compare, not JWT). Keymap in `ir-keymap.json`, edited from « Télécommande »
  (capture via `/api/ir/recent`, test without saving), never pushed by deploy. Actions:
  Nabaztag command, Zigbee and Hue lamp power and brightness (`repeat` while held), Meross
  power, Broadlink code, shutter (`cover`: open, close, stop, position), Mitsubishi AC toggle,
  off and on (`climate_on`: the last settings sent, without their sleep timer), TV power, box
  app, scene. One engine (`ir::run_actions`) for a key, the configurator's test and a scene.
- Same-key presses within 1.2 s are phantom doubles; bindings may lower it (`debounceMs`,
  150 ms for D-pad, media, volume), clamped at 50 ms. Unmapped keys answer 200 and are logged.
  The AC actions wait ~1.2 s so the remote's own IR repeats do not collide with the blaster.

### Climate (Broadlink → Mitsubishi AC, `mitsubishi_ir.rs`)

- 18-byte frames from one grammar: `state-<mode>-<temp>-fan-<fan>-vane-<vane>[-wide-…]
  [-econo-…][-stop-HH-MM][-stopin-<minutes>][-timer-off]` or `state-off`. Each setting is one
  `(value, token, frame code)` table (`mitsubishi_ir::Setting`), read by the parser, the
  encoder, the serde form and `decode_mitsubishi_ir`. The backend returns parsed `settings`
  wherever it returns a command (`SendResult`, the stored state), and
  `POST /api/broadlink/mitsubishi/send` takes `settings` instead of `command` (it writes the
  command: `ClimateSettings::command`).
- The frame sets the unit's clock, so it is sent from local time every time; `stopin-<min>`
  is the relative off timer (10-minute steps, wraps midnight).
- The last order is persisted (`climate-state.json`) and served at
  `GET /api/broadlink/mitsubishi/state`; the form restores it once on mount.

### Nabaztag

garenne firmware, UDP 9998 `grn1` + `GET /status`; clapier runs on the same Pi.
Every 15 min: belly LED = today, ears = tomorrow (both ears when RTE published, the left one
only for a forecast ≥ 80 %). Firmware tooling lives in uplg/nabgcc, not here.

### Tempo

the forecast is refitted on the Mac yearly (`fit_tempo`), never on the Pi
([docs/tempo.md](docs/tempo.md)).

# Maison: agent and contributor brief

Self-hosted home dashboard. One Rust binary (`maison-backend`, Axum 0.8 + tokio) that drives
every device on the LAN and serves a SvelteKit SPA. Production: a Raspberry Pi 1 (ARMv6,
512 MB, Alpine, OpenRC, musl, built `--no-default-features`: no Bluetooth), behind a
Cloudflare tunnel. Development on macOS. Deploying: [docs/deploy.md](docs/deploy.md).

## Map

```
backend/src/
  main.rs            entrypoint; `maison-backend invite <id> --name X [--admin]`,
                     `maison-backend token create|list|revoke` (service tokens)
  lib.rs             AppState, router (`DEVICE_ROUTES`), layers: security headers + CSP
                     (SHA-256 of the page's inline scripts), cache policy, guard_request (the
                     LAN's rules, `lan_only`, Origin vs PUBLIC_URL), audit (every 401/403 logged
                     with who and the address), coded extractor rejections, 180 s timeout,
                     `every()` / `supervised()` for background jobs (a panic is logged, the job
                     goes on; the JoinHandle aborts it), the app constructors
  config.rs          env → Config (paths under MAISON_SOURCE_ROOT, default the repo root)
  error.rs           AppError; `Refusal` = `maison_api::codes::Code` (`AppError::from(Refusal::X)`,
                     `AppError::too_many(retry_s)`); `AppError::http` is for 5xx only
  store.rs           every JSON state file: read, atomic write, owner kept, `locked`
  util.rs            shared helpers: hex/unhex, random secrets, hashing, `clean_name`/`MAX_NAME`,
                     `lock` (a poisoned mutex taken anyway), `ip_group` (IPv4, IPv6 /64),
                     `Bounded` (a TTL map capped per address and prefix), `capped_backoff`,
                     `refused(code, why)`, `house_time`, `minutes_of_day`, `HOUSE_TZ` =
                     Europe/Paris (never `chrono::Local`, which is UTC on the Pi), `test_dir()`
  auth.rs            sessions (JWT `sid`, ended sign-ins), AuthenticatedUser, AdminUser,
                     DeviceClient (a person, an app's bearer token, a screen's cookie),
                     SettingsClient (never a screen), MachineClient (IR token), `REAUTH_MS`
  passkey.rs people.rs routes/passkeys.rs   passkeys, people, invitations (docs/passkeys.md)
  routes/auth.rs     the session's routes: refresh, sign out (Clear-Site-Data), my sessions
  screens.rs routes/screens.rs   wall screens: pairing by code, their cookie, revocation
  events.rs          `GET /api/events`: server-sent change names (`maison_api::events`), one
                     task per family's watch
  scenes.rs          scenes: named lists of IR actions, run by `ir::run_actions` (scenes.json)
  tuya/              feeder, fountain, litter box (local TCP, uplg fork of rust-async-tuyapi):
                     mod (manager), worker (one supervised session per device), cache, parse,
                     feeder (meal-plan codec)
  meross.rs          plugs over local HTTP, one cached reading per plug whatever the clients
                     (Mosquitto exists only for their firmware's boot)
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
  net.rs             every HTTP client (timeouts, `USER_AGENT`; devices: no redirects), LAN
                     device address checks (SSRF gate), `fetch_json` (a public service's JSON),
                     `unreachable` (coded `device_unreachable`; the error stays in the log)
  tempo/             colors, quotas, prices, forecast (docs/tempo.md)
  routes/            one module per domain
  bin/               fit_tempo (Mac only), send_ir, atv_pair, decode_mitsubishi_ir, tuya_probe
backend/tests/       integration tests (+ `live-runtime-tests` feature for real devices)
api/                 maison-api (edition 2024): the API's types said once (screens, sun, tempo
                     and `Today::price_at`, lamps, meross, matter, scenes, remote; the request
                     bodies; `Answer`, `Message`, `OfDevice`, `DeviceRef`), `HOUSE_TZ` and `Tz`,
                     and three tables: `codes.rs` (every refusal: code, status, message),
                     `paces.rs` (every read pace, and how long a gesture, a slider, a toast
                     take), `events.rs` (the event stream's names). `bin/shared-ts` writes the
                     web app's `codes.ts`, `paces.ts`, `events.ts` from them; an api test
                     refuses one out of step. The backend re-exports them, frame/ui reads them
web/src/lib/         live (sources), gesture (pending, unavailable), command, draft, elapsed,
                     clock (the time every view reads), stored, options, session, ui (toasts),
                     i18n (every format), focus, errors (`CODE_WORDS`), form (required fields),
                     limits (the backend's limits, copied by hand), api (the fetch core: errors,
                     renewal, upload), passkeys (session, people, invitations, passkeys),
                     screens (wall screens: pairing, their language), haptics; generated, never
                     edited: codes, paces, events
web/src/lib/components/   every shared brick, each Bits UI primitive wrapped once (Group,
                     DeviceTile, Fold (the one Collapsible: a tile's settings, Disclosure), Menu
                     (DropdownMenu), Door (a door page's head and body), TextField, Loaded +
                     Stale, ToggleGroup, Combobox, Range, Select, Sheet, ListRow, StatusRow…)
web/src/lib/devices/      one folder per device family: api.ts (its types and endpoints), data.ts
                     (its live sources), its components; a dashboard group is `<Family>Group`,
                     a page body `<Thing>Page` (a part of one `<Thing>Panel`), a row `<Thing>Tile`
web/src/lib/account/      the account page's sections
web/src/styles/      tokens.css (light-dark()), app.css
web/scripts/         i18n.check, i18n.add, css.check, imports.fix
i18n/messages/       fr.json, en.json: the only source of texts
e2e/                 Playwright + axe against the real backend (run.sh, house.ts: typed with the
                     app's types, built from web/src/lib/test fixtures; words from fr.json;
                     frame.ts: the native frame app against it, through a proxy serving house.ts)
frame/               the wall screen (a Frameo photo frame, RK3126, Android 6): android/ (the
                     GeckoView kiosk APK, Kotlin), system/ (build.py: our /system, logo and
                     rescue recovery from the vendor's backup; bootimg.py, rescue-tools.sh),
                     rkwrite.py (loader writes), flash.sh, provision.sh, rescue.sh,
                     bootanimation.sh/.ts, test_frame.py (docs/frame.md); linux/ (mainline
                     from the recovery slot: build.sh, rootfs.sh, initramfs.py, image.py,
                     install.sh, try.sh, console.sh, the board's device tree, rootfs/: units and
                     launchers); ui/ (the native interface: SDL3 under cage, Rust 2024,
                     ARCHITECTURE.md; maison-api's types, codes, paces and events, `paths.rs`
                     for every path it asks; build.sh cross-builds armv7, simulate.sh runs that
                     binary on the frame's system emulated)
cache/tempo/         seasons, model.json (fit_tempo), netload.json, weather.json
deploy/ deploy.sh Makefile scripts/ bootstrap/   Pi build, deploy, flash
matter-trust/        CSA attestation roots (tracked)
docs/                deploy.md, tempo.md, passkeys.md, ux.md, frame.md, audio.md (planned audio
                     companion)
```

Runtime state lives at the repo root (gitignored): `devices.json`, `meross-devices.json`,
`device-cache.json`, `hue-lamps*.json`, `zigbee-lamps*.json`, `climate-state.json`,
`broadlink-codes.json`, `ir-keymap.json`, `scenes.json`, `tv.json`, `androidtv.json`,
`nabaztag.json`, `refresh-tokens.json`, `adb-key`, `atv-identity`, `auth/`, `matter/`.

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
- **Refusals are coded**: every 4xx is `AppError::from(Refusal::X)` (or `util::refused(code,
  why)`, the why in the log), a row of `api/src/codes.rs`; no uncoded 4xx is left to build
  (`AppError::http` is for 5xx). An extractor's own rejection (a body that is not the JSON a
  route reads, too large) is coded too (`invalid`, `too_large`). The words for each code are
  the i18n key `code_<code>`: the web app's `CODE_WORDS` (`web/src/lib/errors.ts`, typed by the
  generated `codes.ts`: a code without words does not type-check) and frame/ui's `i18n.rs`. A
  new code: its row, `shared-ts codes`, its words. 401 means only « not signed in » (the apps
  read it as a session that ended).
- **Said once in maison-api, generated for the web app**: refusal codes, read paces (and the
  times a gesture, a slider, a toast take), event names. Change the table in `api/src/`, then
  `cargo run --manifest-path api/Cargo.toml --bin shared-ts codes|paces|events >
  web/src/lib/<name>.ts`; never edit those three by hand. What no generator writes yet (the
  backend's limits) is `web/src/lib/limits.ts`, each value naming the constant it copies.
- **Auth is a route layer**: device routers (`DEVICE_ROUTES`) sit under one `DeviceClient`
  layer (someone signed in, a `devices` service token, a paired screen), handlers never repeat
  it; configuration routes take `AdminUser`, which a service token never is; settings under
  the device layer (a name, a schedule) take `SettingsClient`, which a wall screen never is
  (`not_for_screens`). Each token comes by its own carrier only: an app's as a bearer header,
  a screen's in its cookie. What could hand the house to someone else (a passkey added,
  renamed or removed, an admin's or a recovery invitation, a screen paired, a person removed)
  wants a passkey sign-in within `REAUTH_MS` (5 min). The IR bridge uses `MachineClient`.
- **Changes are pushed by name, read by route**: a family whose state changes on its own
  says so on `events.rs`'s broadcast (`GET /api/events`); the event's data is the prefix of
  the `live()` keys that read it, never the value. The client reads again through the usual
  route; while the stream is open its polls of pushed sources slow to `PUSHED_SAFETY_MS`
  (60 s).
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
  or `ToggleGroup`, every page head `PageHead` (a door page's `Door`), every group `Group`,
  every menu `Menu`, every unfolding `Fold`, a form's text field `TextField` (its required
  fields through `#lib/form.ts`), a tile's settings its `settings` snippet (DeviceTile, folded
  by `Fold`); dates, times, numbers, lists and window titles from `#lib/i18n.svelte.ts`, the
  time of day from `#lib/clock.svelte.ts`, what the browser remembers through
  `#lib/stored.ts` (keys in `src/env.ts`). UX rules: [docs/ux.md](docs/ux.md).
- **Texts only from `i18n/messages`**. French UI copy addresses the person as « tu », with
  French typography (narrow no-break spaces). Code, comments and docs are in English.
- **Tokens only** (`web/src/styles/tokens.css`) for corners, control heights, font sizes,
  colors (the raw palette only there: components use its meanings, `--accent`, `--brand`…),
  the focus ring, layers (`--z-*`), durations (`--dur-*`), opacities and breakpoints:
  `css.check.ts` refuses literal radii, control heights, px font sizes, rgb()/hex outside
  tokens.css, outline literals, literal z-index, durations and opacities, and media widths
  that are not tokens.css's. Strict CSP, never `'unsafe-inline'`.
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
- **The backend is Rust edition 2021**: no let-chains there. `api/` and `frame/ui` are 2024.
- SvelteKit 3: imports are `#lib/...` with the file extension (`bun scripts/imports.fix.ts`);
  public build-time values come from `src/env.ts` (`defineEnvVars`) via `$app/env/public`;
  TypeScript 6 (7 is the native port: SvelteKit's sync needs the JS API it dropped). No service worker on purpose (`legacy.ts`
  unregisters the old one). Vitest 5: `src/test-setup.ts` sets what `$app/env/public` reads outside Kit's dev server; `getByText` and `toHaveTextContent` are exact by default.

## Commands

```bash
make backend / make web
make test                             # fmt, clippy, tests of api, backend, frame/ui; web check and tests
make deny                             # cargo deny on the backend (backend/deny.toml)
cd web && bun run check               # i18n, css, oxfmt --check, oxlint, svelte-check, e2e types
cd web && bun run format              # oxfmt over web/ and e2e/ (`bun run lint`: oxlint alone)
cd web && bun scripts/i18n.add.ts '{"key": ["français", "English"]}'   # every locale at once
cd e2e && bun run all                 # after `cargo build` (backend and frame/ui) and `bun run --cwd web build`
cargo check --manifest-path backend/Cargo.toml --no-default-features   # the Pi's feature set
cargo fmt --manifest-path backend/Cargo.toml --check                    # rustfmt's defaults, kept clean
MATTER_LIVE_CODE=<code> cargo test --features live-runtime-tests --test matter_live -- --nocapture
python3 -m unittest discover -s frame -p 'test_*.py'   # the frame's tooling
cd frame/ui && cargo test && cargo clippy --all-targets -- -D warnings && cargo deny check   # the native app
frame/ui/build.sh && FRAME_SIM=armv7 sh e2e/run.sh frame.ts   # its armv7 binary, emulated, under cage
cargo run --manifest-path api/Cargo.toml --bin shared-ts paces > web/src/lib/paces.ts   # codes, events alike
cd frame/android && ./gradlew lintDebug testDebugUnitTest assembleDebug   # the kiosk APK
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
- cloudflared runs as its own user (`PI_TUNNEL_USER`, default `cloudflared`), its token in
  `/etc/cloudflared-maison/token` (0600, written by `deploy.sh push`, never in the Pi's
  `.env`). Mosquitto's 8883 is open only to the plugs' addresses (`maison-mqtt-firewall`,
  from `meross-devices.json`; mosquitto needs it). Logs are 0640, kept 14 days.
- APK uploads are capped at 96 MB (held in memory on 512 MB).

### Access

- Passkeys need a domain and HTTPS: the RP ID is the host of `PUBLIC_URL` (default
  `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`), never an IP. From the LAN the backend answers
  only `/health`, `/api/ir/key` and, to an app's bearer token, the device routes (over
  Tailscale always, over the plain LAN only with `ALLOW_LAN_TOKENS`: the token would cross the
  Wi-Fi in clear); everything else, wall screens included, comes through cloudflared on
  loopback. `/api/ir/key` is refused through the tunnel (`lan_only`), whatever the token.
  Client address = `CF-Connecting-IP` from loopback only.
- A request that changes something must come from this site: `Sec-Fetch-Site`, else `Origin`
  equal to `PUBLIC_URL`'s origin or matching `Host` (`cross_site`); clients that send neither
  (kird, curl) pass.
- `IR_API_TOKEN` under 32 characters: the backend refuses to start; wrong tokens are limited
  per address (`too_many_attempts`).
- Invitation links are `<PUBLIC_URL>/invite#<token>`: the token in the fragment, never in a
  request (the page asks `POST /api/invites/greet`). Admin invitations last 72 h, others 7
  days; a session lives `SESSION_MAX_DAYS` (30) from its passkey sign-in, refreshed or not.
- A wall screen's cookie is renewed only by `POST /api/screens/me` (each start of its page);
  `GET` only reads. A screen unseen for `SCREEN_IDLE_DAYS` (90; 0 = never) is revoked.
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
- The driver's loop serves callbacks and commands as they come; its chores (link health,
  availability) run on a 1 s tick. Lamps are asked for ZCL attribute reporting; only those
  that do not report (or whose reports stopped) are pinged.
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
- Shutters are followed by Matter subscriptions (`Node::subscribe`), served from what they
  last reported, not read per request.

### Philips TV (55PUS6753, Saphi, `tv.rs`)

- JointSPACE on plain HTTP :1925, no pairing. Unimplemented endpoints (`/6/sources`,
  `/6/applications`, `/6/activities/*`, `/5/*`) kill the server persistently: only unplugging
  the set for ~30 s revives it. Endpoints are an enum, every call goes through one gate
  spaced by `MIN_REQUEST_GAP`; the web app polls once a minute. Never widen without testing.
- Power goes over IR only (RC5 via the Broadlink, discrete `0x3F` on / `0x3D` off): deep
  standby drops the network (WoL and CEC fail). Saphi cannot switch sources: `switchToBox`
  wakes the box over DIAL and CEC One Touch Play routes the input. The box's address and DIAL
  app are the box's own settings (`androidtv.json` `host`, `wakeApp`); a `tv.json` that kept
  `boxHost`/`boxWakeApp` still loads.

### Android TV box (MECOOL LEAP-S1)

- Keys and app launches go over **Remote v2** (`atvremote.rs`: mutual TLS :6466, hand-encoded
  protobuf, one session owned by a task): ADB's `input` boots a JVM per key (~150 ms, ~10 s
  when the box sleeps). Pairing is per host; the client cert must be RSA (the secret hashes
  both moduli), so the key comes from `rsa` and rcgen only wraps it. `protocol_version` 2,
  `service_name` `atvremote`. Falls back to ADB when unpaired. The box's certificate is pinned
  by the first session after pairing (trust on first use): another is refused until a new
  pairing forgets it.
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
  An AC action run from a key waits for that key's release (1.2 s at most), then 100 ms, so
  the remote's own IR repeats do not collide with the blaster's frame.
- A scene's or a key's run answers each action structured (`{action, ok, code}`), never words.

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
Pushed when Tempo's answer changes, when the day turns, and every hour as a safety net: belly
LED = today, ears = tomorrow (both ears when RTE published, the left one only for a forecast
≥ 80 %). Firmware tooling lives in uplg/nabgcc, not here.

### The wall frame (`frame/`, [docs/frame.md](docs/frame.md))

- Our /system is the vendor's with Frameo, Adups FOTA and SuperSU gone and the kiosk APK the
  only home and device owner; boot, uboot and userdata stay the vendor's. U-Boot boots images we
  pack (proved with recovery), so `--boot` is possible, never flashed so far.
- The recovery is a rescue (`frame/rescue.sh`): its menu needs buttons the frame lacks, its
  adbd never runs as root and lets nothing gain privileges, `/sbin` is closed to its user,
  toybox `nc -L` is mute on this kernel: root comes from an init service over fifos
  (/dev/rootsh), tools (mksh, toybox from source) in the ramdisk's /system/bin.
- ADB: Frameo approved a computer's key for the session only, and SystemUI (the prompt) is
  disabled on this build: without Frameo, only keys in `/data/misc/adb/adb_keys` work.
  `flash.sh keys` puts them there while root exists; `flash.sh` refuses to start without them.
  Once flashed there is no root (`ro.debuggable=0`): APK updates are `pm install -r`, same key.
- No ADB at all: power off, hold the reset pinhole, plug USB-C into the Mac, then power: the
  loader. It can drop mid-write: `rkwrite.py` writes 16 MiB chunks read back, and resumes. Its
  sector 0 is the `parameter`'s (kernel sector − 8192), proved before every write.
- A system app's native libraries are extracted in `lib/arm` (Android 6 reads the ABI there);
  the vendor build forces a home screen to the panel's landscape unless the activity asks
  `screenOrientation="user"`; the clock says 1970 until NTP, so Gecko stays hidden (the brand's
  petrol) until a page has loaded. The boot animation draws before any rotation applies;
  U-Boot's logo is `logo.bmp` in the resource partition, drawn on the raw panel (docs/frame.md).
- Frameo reinstalls itself into /data while its beta is on: `pm uninstall net.frameo.frame`.
- Powered up with the USB-C on a computer, U-Boot boots the recovery after a wait: power it with
  the USB-C unplugged to get Android, then plug it in.
- The backup, the vendor's device tree, the release keystore and rkdeveloptool live in
  `~/maison-frame/`, never in the repo; rkdeveloptool writes a `log/` into its working directory.
- Our build.prop sets `ro.adb.secure=1` (the vendor's default.prop leaves it out). The kiosk's
  activity is exported (it is the home): it lifts itself only with the one-time grant
  `ProvisionReceiver` (DUMP: the shell's alone) hands it, never on its own action's word.
- Mainline: Maison's address is root's, `/android-data/maison/target` (0600), what the units
  load as their `target` credential. install.sh can only write Android's shell-writable
  `/data/local/tmp/maison/target`; the initramfs adopts it once, while root keeps none, then
  removes it. The units read only root's copy, so a new system wants the new boot image
  (its initramfs adopts): flash both. To change the address, remove root's copy first.
- Linux boots by default from `boot` (14 MiB: our image without the vendor's logo, `image.py
  --slot boot`; the screen stays black until Linux lights it); Android sits in `recovery`. A new
  system goes over the Wi-Fi from Linux (`try.sh --install`, its SHA-256 checked); a new kernel to
  `boot` from the loader (`try.sh --flash`, the reset pinhole). No battery: timesyncd's clock is
  kept on userdata, and the app never waits for NTP. `ssh root@maison-frame.local` (the Mac's key
  only); the journal kept on userdata (`journalctl -D /var/log/journal/<boot's machine id>`); the
  boot picture is the initramfs's (splash.py), each step said under the mark.
- The mainline USB-C root console exists only while `/android-data/maison/debug-console`
  does (root's password is locked): remove the file once the frame is on the wall.
  `initramfs.py --debug` adds the bring-up's backlight blinks and 8 s pause.

### Cat devices, plugs, blaster, Hue

- Tuya status is served from the device's session (read on connect, then its pushes);
  `?refresh=true` forces a radio read (2 s or more). Feeds: one per 5 s per feeder, 48
  portions per house day (`too_many_attempts` past them). The litter box's factory reset is an
  admin's route (`POST /api/devices/{id}/litter-box/factory-reset`).
- Meross: one reading per plug whatever the number of clients (callers that come together
  share one request).
- Broadlink: `POST /api/broadlink/discover` (admin) forces a scan; a member's `host` must be
  a blaster already known.
- Hue: lamps are heard through their GATT notifications; only a lamp whose notifications
  could not be subscribed to is polled.

### Shared code and the repository

- The root `.gitignore` anchors runtime files (`/matter/`, `/devices.json`): an unanchored
  `matter/` hid `backend/src/matter/` from git for months.
- `util::capped_backoff(base, attempt, max)` counts attempts from 1 (1 = `base`, 2 = twice):
  every retry and reconnect delay goes through it.
- The event stream carries names, never values: adding a pushed family means its watch in
  `events.rs`, its name in `api/src/events.rs` (the prefix of its `live()` keys), then
  `shared-ts events`. Cloudflare closes an idle answer after ~100 s: the stream says a
  keep-alive every `KEEP_ALIVE_MS` (25 s).

### Tempo

the forecast is refitted on the Mac yearly (`fit_tempo`), never on the Pi
([docs/tempo.md](docs/tempo.md)).

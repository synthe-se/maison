# The frame's native interface: architecture (as built, 2026-10-16)

Maison's dashboard on the wall frame (RK3126, four Cortex-A7, Mali-400 / Lima GLES 2.0, 1 GB,
800×1280 portrait, touch), drawn natively with SDL3 under cage, in Rust (edition 2024). Same
house, same API, same words, colors, fonts and icons as the web app; nothing copied by hand.
Choices made with the owner: SDL3 under cage, a shared API crate, the paces generated for the
web app, the web app's own fonts, the turn measured on the frame.

## Process and display

- **cage** (wlroots kiosk compositor, Debian trixie) on KMS, one client: `maison-frame-ui`, in a
  logind session (`maison-ui.service`, user `kiosk`, tty7, PAM `maison-ui`), in place of cog/WPE.
  The launcher (`/usr/local/sbin/maison-ui`) turns the output 180° with `wlr-randr` (the panel is
  mounted upside down); if the compositor refuses, the app turns its own picture
  (`MAISON_ROTATE=180`, one full-screen copy a frame). The touch needs neither: the device tree
  maps it to what the person sees (cog turned its picture only).
- **SDL 3** and **SDL_ttf 3** from their sources, linked in: an 8.9 MB armv7 binary needing only
  libc, libm, libgcc_s; SDL opens Wayland, EGL and GLES 2 at run time. The renderer is SDL's GLES 2
  (`SDL_Renderer`), vsync on.
- **Drawn only when something changed**: the loop sleeps on SDL's events (a finger, a window
  event, or a thread's word that a read, an answer or a change named by the server came, an SDL
  user event); it wakes by itself only for what changes with time (a gesture turning slow or
  late, a held slider value ending, a value turning stale, a toast leaving) and every frame while
  a fling runs. Idle: no frame, no CPU.

## Shared with the backend and the web app

| What | Source | How the frame gets it |
|---|---|---|
| API types | **`api/` (maison-api)**: screens (and the cookie's names), sun, Tempo (and `Today::price_at`: the price in force is read, never worked out again here), lamps, Meross, Matter covers, scenes (and their icons) and the remote's actions, `KeyResponse`, `Answer`, `DeviceRef`, `HOUSE_TZ` and its `Tz`; the request bodies (`PowerRequest`, `BrightnessRequest`, `ToggleRequest`, `PositionRequest`, `SkipRequest`), serialized, never written as JSON by hand | path dependency of the backend and of the frame |
| Paces | `maison_api::paces` (one macro: each pace and its words), and how long a gesture, a slider, a toast take (`COMMAND_SLOW_MS`, the families' `*_LIMIT_MS`, `SLIDER_*`, `TOAST_MS`, `WALL_WARNING_MS`), `STALE_CADENCES` | the frame reads them; the web app imports `web/src/lib/paces.ts`, generated (`cargo run --manifest-path api/Cargo.toml --bin shared-ts paces > web/src/lib/paces.ts`), an api test refuses it out of step. The families the frame does not read yet keep theirs in `data.ts` |
| Refusals | `maison_api::codes::Code` | a refusal's `code` read as a `Code`; each said by the web app's words, the key `code_<code>` (as errors.ts `CODE_WORDS`; a test checks every code has its words), else by its status |
| Changes | `maison_api::events` (`GET /api/events`) | one thread reads the server-sent stream for the app's life: each `change` names the prefix of the sources to read again (never a value), read at once; while it is open those sources poll only every `PUSHED_SAFETY_MS` (60 s); closed, everything reads again once it reopens (backoff to 32 s; refused before pairing, it opens the moment the screen is paired) |
| Paths | `src/paths.rs` | every path the frame asks, once (the families, the gestures, the pairing, the event stream) |
| Words | `i18n/messages/*.json` | embedded; `k::KEY` consts generated (a misspelt key does not build), every message's shape checked by build.rs (a text, or a plural on `count`); plurals as Intl's; numbers, clocks, durations, lists as the web app's Intl writes them |
| Tokens | `web/src/styles/tokens.css` | build.rs: colors (both sides of `light-dark()`), lengths, the type scale, what `@media (min-width: …)` and `(pointer: coarse)` change (the page margin, a finger's control heights), the red day's hatching (`--tempo-red-hatch`). The components' own few measures (Icon.svelte's 18 px, DeviceTile's 22 px glyph and 1.5 px ring, `.tiles`' 20rem, Range's 2 px thumb ring, Stale's 14 px clock, the code's 0.15em) are in their Svelte, said once in `ui/layout.rs` |
| Icons | `web/src/lib/components/Icon.svelte` (lucide) | build.rs extracts them, one typed const each (`icon::LIGHTBULB_OFF`: a misspelt one does not build); resvg rasterises each once per size, white, tinted when drawn |
| Fonts | Cal Sans, Borel (`web/static/fonts`), Fraunces (fontsource, from `web/node_modules`: `bun install` first) | build.rs (allsorts): WOFF2 to TrueType, the variable faces instanced at each weight the scale draws (Cal Sans 400/500/600, Fraunces 500/600); Noto Sans behind for the three signs Cal Sans lacks |
| Pairing, auth | `/screens/pairing`, `/claim`, `POST /me` | the web app's flow (`POST /me` says who the screen is and renews its cookie: `GET` would only read); the cookie kept 0600 in `/android-data/maison/ui` with the origin that set it (sent nowhere else; another origin's: the screen pairs again); HTTPS only (`http://` to this computer, or a simulation's `MAISON_ALLOW_HTTP=1`), no proxy from the environment |
| Sun theme | `/matter/place` (signed in), `/sun` (signed out) | the palette follows the side |

## Inside the app

```
frame/ui/
  build.rs       tokens, icons, keys, fonts (above)
  src/main.rs    SDL, the loop (wait, input, draw when dirty), the turn, a simulation's shots
  config.rs      the address (credential `target`: root's /android-data/maison/target, which the
                 initramfs adopts from install.sh's; else MAISON_TARGET), state dir, language,
                 turn, scale, a window for a simulation
  client.rs      HTTP (ureq 3, rustls, no redirects, no proxy, bounded waits), the screen cookie
                 bound to its origin, the event stream (line by line, no overall timeout; a
                 body read ten minutes at most, then opened again), four failures (signed
                 out, refused with its `Code`, unreachable, unreadable)
  paths.rs       the API's paths
  live.rs        the web app's live(): one thread per source at its pace, read while a drawing
                 wants it, stopped when none does, stale after STALE_CADENCES paces, its last
                 failure kept for the words, refresh(prefix); listen(): the event stream's
                 thread, each name a refresh(prefix), pushed sources slowed while it is open
  pairing.rs     who the screen is: POST /me, a code until claimed, expired, removed
  commands.rs    gestures: target shown at once, slow after COMMAND_SLOW_MS, late past the family's limit,
                 held until the family is read again after the answer (the web app's settle),
                 one order on the wire per key (the latest waits), « Tout … » said once
  i18n.rs theme.rs   words and tokens
  sim.rs         a simulation's hands and eyes (MAISON_STDIN=1): press, slide, drag, shot
  ui/            gfx (text atlas, discs and rounded boxes, icons, pictures), icon (typed),
                 model (Group, Tile, Part, Button, Slider, Screen), layout (the web app's
                 measures, the painter's primitives, hits), bricks (button, slider, tile),
                 screens (house, door, toasts), slider (live sends every SLIDER_THROTTLE_MS,
                 held SLIDER_HOLD_MS), touch (tap, drag, fling)
  families/      tempo, lamps (Hue + Zigbee), shutters, plugs, scenes: pure functions from the
                 API's answers to a group, each tested on the backend's JSON
  simulate.sh sim/   the armv7 binary on the frame's system, emulated (below)
```

## Scope

1. Done: pairing (and removal), the sun's theme, the scrolled column, scenes, Tempo, lamps,
   shutters (move, skip tonight's, position), plugs (power and its cost, red peak hours).
2. Next: climate, TV and box, cats, Nabaztag, the « Maintenant » strip, a spinner for slow
   gestures (today a static ring).
3. Not on the frame: device pages, settings, admin, the keymap.

The house is laid out as the web dashboard is (+page.svelte's `.dashboard`): the groups in
balanced columns at least 22rem wide (two on the panel), each group whole, its devices one
framed list of rows; heights come from the drawing code itself in a measuring pass
(`P::measuring`), so the balance never disagrees with what is drawn. The header wears the mark.

## Quality and simulation

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check` (no GPL; the
  repository's own crates unlicensed until the owner chooses), 67 unit tests (client against a
  stub server, pairing, live, commands, sliders, touch, words and refusals, tokens and media
  queries, fonts, icons, each family, the app's scroll, toasts, wakes and removal, and a real
  drawing on SDL's dummy video driver and software renderer: hits, state lines, fit).
- **e2e/frame.ts** (in `bun run all`): the real backend on a throwaway root, the simulated house
  (e2e/house.ts, the same one the pages see) behind a proxy, the app paired for real from an
  admin's phone, then driven: a lamp off and on, its brightness slid, a plug, a shutter, a lamp
  that never answers, the screen removed. Pictures in e2e/shots.
- **`FRAME_SIM=armv7`**: the same scenario on the frame's own binary (`build.sh`), in Debian
  trixie armhf under qemu, under headless cage turned 180° by the frame's own launcher, GLES 2
  from Mesa's llvmpipe: the frame's Wayland, EGL, renderer, fonts and HTTP path, only the GPU
  differs. First frame 2.3 s emulated.

## To measure on the frame

First frame, frame times while scrolling (`MAISON_STATS=1` in the unit says them every 10 s),
memory, the turn (wlr-randr against MAISON_ROTATE), the touch's way up, the scale (1 as the kiosk
drew; `MAISON_SCALE`).

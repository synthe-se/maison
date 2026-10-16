# <img src="web/static/brand.svg" alt="" width="36" align="top"> Maison

[![last commit](https://img.shields.io/github/last-commit/synthe-se/maison)](https://github.com/synthe-se/maison/commits/main)
[![top language](https://img.shields.io/github/languages/top/synthe-se/maison)](https://github.com/synthe-se/maison)
[![license](https://img.shields.io/github/license/synthe-se/maison)](LICENCE)
[![runs on](https://img.shields.io/badge/runs%20on-Raspberry%20Pi%201-C51A4A?logo=raspberrypi&logoColor=white)](docs/deploy.md)
[![sign-in](https://img.shields.io/badge/sign--in-passkeys%20only-3B82F6)](docs/passkeys.md)

A self-hosted home dashboard: one Rust binary on a 2012 Raspberry Pi 1.

![Maison](/screenshots/maison-e4a164a85c37.jpg)

## Why not Home Assistant

| | Maison | Home Assistant |
|---|---|---|
| Runs on | Raspberry Pi 1 (ARMv6, 700 MHz, 512 MB) | Raspberry Pi 4 or 5 with at least 2 GB of RAM and a 32 GB card ([requirements](https://www.home-assistant.io/installation/raspberrypi)); 32-bit ARM, the original Pi included, unsupported since 2025.12 ([announcement](https://www.home-assistant.io/blog/2025/05/22/deprecating-core-and-supervised-installation-methods-and-32-bit-systems/)) |
| Footprint | one ~20 MB binary, ~18 MB of RAM (measured: the whole Pi uses 67 MB of 427 MB) | Python runtime, OS image or containers |
| Cloud | none: no account, every device driven on the LAN; a Cloudflare tunnel only exposes it | local-first; remote access through the optional Nabu Casa cloud; some integrations are cloud-based |
| Sign-in | passkeys only, by invitation, no password | username and password (optional MFA) |
| Radios | Zigbee (native EZSP), Matter commissioning, infrared: built in, nothing to install | Matter needs the Matter Server add-on; Zigbee via ZHA or the Zigbee2MQTT add-on |

Controls Hue and Zigbee lamps (and the Hue dimmer), Matter roller shutters that follow the sun, Meross plugs, Tuya cat devices (feeder, fountain, litter box), a Mitsubishi air conditioner over IR, a Philips TV and an Android TV box, a set-top-box remote turned into a house remote, a Nabaztag, and EDF Tempo with a 7-day forecast. Changes are pushed to every open page and screen as they happen (server-sent events); polls are only a safety net.

## On the wall

A Frameo photo frame (RK3126, four Cortex-A7, 1 GB, an 800×1280 touch panel) turned into the house's wall screen: no Android, no browser.

- **Mainline Linux 7.2 and Debian trixie**, built here (`frame/linux`): our device tree, the panel, the touch, the Wi-Fi; the system a read-only squashfs on the eMMC. It boots by itself on its own power, a power cut included; the vendor's Android stays whole in the recovery slot.
- **A native app in Rust**, SDL3 under cage on the GPU (`frame/ui`): the web dashboard's layout, words, colours, fonts and icons, generated from the same sources, drawn only when something changes. Every dashboard group, the summary strip at the top, and a Tempo page.
- **A wall screen of the house**: paired once from an admin's phone, then in by its own cookie; it dims after two minutes without a finger, nearly dark at night, and the first touch only wakes it.
- **Maintained over the network**: system and kernel updates over Wi-Fi (SHA-256 checked), SSH by key only, a journal that survives power cuts, boot steps on screen.

Bring-up and maintenance: [docs/frame.md](docs/frame.md). The app: [frame/ui/ARCHITECTURE.md](frame/ui/ARCHITECTURE.md).

## Develop

Needs Rust and Bun.

```bash
cp .env.example .env                      # set JWT_SECRET: openssl rand -hex 32
PUBLIC_URL=http://localhost:5173 make backend   # API on :3033
make web                                  # http://localhost:5173
make test                                 # fmt, clippy, tests (api, backend, frame/ui), web checks, Vitest
cd e2e && bun install && bun run all      # Playwright + axe (the frame's app too), after cargo build and web build
frame/ui/build.sh && frame/linux/rootfs.sh && frame/linux/try.sh --install   # wall screen: build, update over Wi-Fi
```

Sign in with an invitation: `PUBLIC_URL=http://localhost:5173 cargo run --manifest-path backend/Cargo.toml -- invite me --name Me --admin`.

Deploying to the Pi: [docs/deploy.md](docs/deploy.md). Contributors and agents: [AGENTS.md](AGENTS.md).

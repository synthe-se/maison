# Maison

A self-hosted home dashboard: one Rust binary on a 2012 Raspberry Pi 1.

![Maison](/screenshots/maison.jpg?v=1791001056)

## Why not Home Assistant

| | Maison | Home Assistant |
|---|---|---|
| Runs on | Raspberry Pi 1 (ARMv6, 700 MHz, 512 MB) | Raspberry Pi 4 or 5 with at least 2 GB of RAM and a 32 GB card ([requirements](https://www.home-assistant.io/installation/raspberrypi)); 32-bit ARM, the original Pi included, unsupported since 2025.12 ([announcement](https://www.home-assistant.io/blog/2025/05/22/deprecating-core-and-supervised-installation-methods-and-32-bit-systems/)) |
| Footprint | one ~20 MB binary, ~18 MB of RAM (measured: the whole Pi uses 67 MB of 427 MB) | Python runtime, OS image or containers |
| Cloud | none: no account, every device driven on the LAN; a Cloudflare tunnel only exposes it | local-first; remote access through the optional Nabu Casa cloud; some integrations are cloud-based |
| Sign-in | passkeys only, by invitation, no password | username and password (optional MFA) |
| Radios | Zigbee (native EZSP), Matter commissioning, infrared: built in, nothing to install | Matter needs the Matter Server add-on; Zigbee via ZHA or the Zigbee2MQTT add-on |

Controls Hue and Zigbee lamps (and the Hue dimmer), Matter roller shutters that follow the sun, Meross plugs, Tuya cat devices (feeder, fountain, litter box), a Mitsubishi air conditioner over IR, a Philips TV and an Android TV box, a set-top-box remote turned into a house remote, a Nabaztag, and EDF Tempo with a 7-day forecast.

## Develop

Needs Rust and Bun.

```bash
cp .env.example .env                      # set JWT_SECRET: openssl rand -hex 32
PUBLIC_URL=http://localhost:5173 make backend   # API on :3033
make web                                  # http://localhost:5173
make test                                 # cargo test, web checks, Vitest
cd e2e && bun install && bun run all      # Playwright + axe, after cargo build and web build
```

Sign in with an invitation: `PUBLIC_URL=http://localhost:5173 cargo run --manifest-path backend/Cargo.toml -- invite me --name Me --admin`.

Deploying to the Pi: [docs/deploy.md](docs/deploy.md). Contributors and agents: [AGENTS.md](AGENTS.md).

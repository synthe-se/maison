# Audio companion (planned)

A small box next to the Triangle Elara LN01 (passive) that plays whatever the house sends it:
AirPlay 2 from the Mac and the iPhones, Bluetooth from Android. Maison
drives it (volume, source, standby) like any other device. Nothing here is built yet: this is
the plan, with the measurements that decide its open points.

Goals, in order: **the sound is good**, **it draws next to nothing when silent**, any app on
any device can play to it (no streaming service targeted).

Expected use, most to least: **the Mac** straight to the speakers, **an iPhone** over
AirPlay, **Android** now and then (Bluetooth, SBC is enough).

## The speakers

Elara LN01: 2-way bass reflex, 25 mm dome + 135 mm mid-woofer, **89 dB/W/m, 8 Ω nominal
(4.5 Ω minimum)**, 50 W RMS, 56 Hz–22 kHz. Easy to drive: 17 W per channel gives about
101 dB at 1 m, more than a living room needs. Normal listening is around 1 W.

## Hardware

| Part | Choice | Why |
|---|---|---|
| Computer | **Raspberry Pi Zero 2 W** | The smallest board shairport-sync supports for AirPlay 2 (« Pi 2 or Zero 2 W, or better »); Linux gives Bluetooth A2DP with AAC/aptX/LDAC; ~0.4–0.6 W idle with Wi-Fi |
| Amplifier | **HiFiBerry Amp2** (already owned) | TI TAS5756M, I²S in (no separate DAC stage), THD+N < 0.02 % (0.1–10 W), SNR 104 dB, 17 W/ch into 8 Ω at 18 V (< 0.1 % THD); powers the Pi from the same supply |
| Supply | **Mean Well GST60A18** (18 V, 60 W) | HiFiBerry recommends 12–20 V (24 V absolute max) and 18 V for 8 Ω; < 0.075 W no-load, efficiency level VI |
| Network | Wi-Fi (2.4 GHz), next to the router | AirPlay 2 buffers seconds of audio; no Ethernet socket there |

The Amp2 fits the Zero 2 W: same 40-pin header (bought already soldered: the Zero 2 WH), the Pi Zero is in HiFiBerry's list, Zero W users run it, cases exist for the pair. It
overhangs the Zero (HAT size); only the two holes on the header side line up. Power comes
from the Amp2 through the header: never plug the Pi's micro-USB at the same time. Software
side the Amp2 is « a DAC+ with a power stage » (`hifiberry-dacplus` overlay).

### Ruled out

- **ESP32-S3** (Sonocotta Louder, airplay-esp32): lowest power, but the S3 has Bluetooth LE
  only (no A2DP, so nothing from Android), one protocol per firmware, and the AirPlay 2 port
  is a single developer's reverse engineering under a non-commercial license.
- **Infineon MA12070P**: better on paper (0.007 % THD+N at high power, < 160 mW idle chip),
  but Infineon's Pi HAT is end of life and the remaining boards are generic « 2×80 W » ones of
  unknown quality. At 1 W, 0.02 % and 0.007 % are both far below audibility. It comes back only
  if the Amp2's idle draw fails the gate below.
- **HiFiBerry Amp4 Pro**: the Amp2's figures (power, THD+N, SNR) plus a dual-domain clock.
  Lower jitter, not worth replacing a board that is already here.
- **Powerline (CPL)**: 2.3–3.9 W per adapter in use, 0.4–0.5 W idle, two needed: more than
  the whole companion.
- **Google Cast**: a closed stack for Google-certified devices; no working open receiver
  exists. Android casts already reach the TV through the box (LEAP-S1, Chromecast built-in),
  and its speakers.
- **Maison's Pi 1 as the receiver**: ARMv6, below shairport-sync's AirPlay 2 floor. The
  companion receives, Maison orchestrates.

### The HiFiBerry DAC+ ADC Pro (also in the box)

Burr-Brown ADC (110 dB SNR, line in up to 2.1 Vrms, electret mic bias) and DAC, its own
low-jitter dual-domain clock, < 0.3 W. Not needed by this plan. Its one possible use: an
**analog TV input** instead of the optical receiver, the Pro as I²S clock master, its ADC on
the Pi's I²S input, the Amp2 as a slave on the output. That means stacking two HATs (two
EEPROMs, a custom overlay), the TV sound through the Pi (some latency, but the room EQ
applies to it), and the TV's analog out may follow its volume. Worth one bench try only if
the optical board stalls; otherwise it stays in the box.

## Sources

| Device | Path | Notes |
|---|---|---|
| Mac | AirPlay 2 as the system output | Any app, lossless (ALAC) |
| iPhone | AirPlay 2 | Any app, or Control Center |
| Android | Bluetooth A2DP sink (bluez-alsa) | The only system-wide path on Android; AAC everywhere, LDAC if bluez-alsa is built with the decoder (check with `bluealsa --help`) |

One source plays at a time: the latest to start wins.

## The TV (later)

Not now: the TV's own speakers are good enough. Kept here in case.

The 55PUS6753 has one optical output. A DIR9001 turns it into I²S; a 74LVC157 picks which
I²S the Amp2 hears, the Pi's or the receiver's (the Amp2, like the DAC+, takes its clocks
from outside, so either source can drive it: to confirm on the bench). The receiver's lock
pin goes to a Pi GPIO: lock = the TV is playing. Maison also knows the TV's power from
JointSPACE and from its own IR orders. The TV's digital output must be set to **PCM**, not
Dolby. A small interposer board sits between the Pi and the Amp2; the TV would then take over while
its signal is present and give back when it stops.

## Power

Budget at the wall, silent, Wi-Fi connected, amp in software standby:

| Part | Expected |
|---|---|
| GST60A18 no-load | < 0.075 W |
| Pi Zero 2 W | 0.4–0.6 W |
| Amp2 (TAS5756M standby) | not published: **measure** |

**Gate: ≤ 1.5 W at the wall when silent.** The TAS5756M's modulation idles higher than the
TAS5805M's hybrid mode, and the Amp2's supply also feeds the Pi, so the amp cannot be cut
alone. Over the gate: switch to Sonocotta's Louder Raspberry HAT (TAS5805M, 16.5 mA idle at
13.5 V, deep sleep, built-in DSP) or an MA12070P board. Under it: keep the Amp2. For scale,
1 W all year is 8.8 kWh.

## Software on the companion

- Alpine aarch64, diskless (the SD card is read at boot, not worn), OpenRC, as on Maison's Pi.
- **shairport-sync** with AirPlay 2, built from source and pinned (distributions lag), and
  **nqptp** (root, ports 319/320).
- **bluez-alsa** as an A2DP sink: AAC, aptX, LDAC if the decoder builds.
- Wi-Fi power saving off (`iw dev wlan0 set power_save off`): it causes latency spikes and
  AirPlay discovery failures.
- Room correction in the Pi's ALSA chain (REW measurement, then EQ), for every source.
- **`maison-audio`** (Rust, aarch64 musl), the only thing Maison talks to:
  - source arbitration (pauses or drops the others),
  - volume (ALSA mixer of the TAS5756M), mute,
  - amp standby after a few silent minutes, wake on play,
  - now playing (shairport-sync's metadata pipe, bluez-alsa's device name),
  - a small HTTP API with a bearer token (constant-time compare, as `IR_API_TOKEN`).

Sketch of the API (camelCase keys, snake_case values, as Maison's):

```
GET  /status   → { "source": "airplay" | "bluetooth" | "idle", "volume": 0-100,
                   "muted": bool, "amp": "standby" | "on",
                   "nowPlaying": { "title", "artist", "album", "device" } | null }
POST /volume   { "volume": 0-100 } | { "step": -5 }
POST /mute     { "muted": bool }
```

## Option: our own AirPlay 2 receiver, in Rust

Not decided; to try on the bench once shairport-sync works as the reference.

**Why.** shairport-sync (C, since 2014) is maintained (5.0.4 in April 2026, 5.1 in
development) but heavy: avahi, openssl, libplist, libsodium…, a separate root daemon (nqptp),
built from source. A receiver inside `maison-audio` would make the companion one static musl
binary: arbitration with Bluetooth, volume and now playing native, no metadata pipe, deployed
like Maison.

**What exists** (none mature):

| Project | License | State |
|---|---|---|
| openairplay2 (st3fan) | see its LICENSE | v0.5.0, Linux, one stream, verified on a Mac; AAC 44.1 kHz only: no ALAC, no PTP, no persistent pairing |
| shairplay-rust (metaneutrons) | LGPL-3.0 | « clean-room », `forbid(unsafe)`, AirPlay 1 and 2 buffered audio with PTP; 19 stars, partly AI-written by its own account |
| rairplay | GPL-3.0 | A library; FairPlay from bundled C (shairplay): not for an MIT repo |

**The work.** HomeKit pairing (SRP-6a, Ed25519, X25519, ChaCha20-Poly1305: existing crates),
the encrypted RTSP channel with binary plists, buffered AAC and realtime ALAC (symphonia
decodes both in pure Rust), PTP so the Mac's sound stays on its picture, and **FairPlay
fp-setup**, reverse-engineered obfuscated tables: the hard part, and its license follows
whatever code it derives from. Then every iOS or macOS release may break it.

**The hardware it changes.** Less than it seems. shairport-sync's « Pi 2 or Zero 2 W » floor
is its AAC decoding, resampling and PTP; the ESP32 port shows AirPlay 2 fits in 240 MHz, so a
lean receiver could open ARMv6 boards (Pi 1, first Zero W) and non-Pi boards. What still
fixes the hardware:

- **Android needs Bluetooth Classic (A2DP)**: no ESP32-S3 (BLE only). A classic ESP32 has
  A2DP but SBC only, and would mean the receiver on ESP-IDF (`esp-idf-svc`, Rust with std).
- **The Amp2 needs a Pi's 40-pin header.** Changing the amp is acceptable: an ESP32 board with
  a TAS5805M (Sonocotta Louder) or an MA12070P board would follow the receiver.
- Maison's own Pi 1 as the receiver stays a no: one core already running Zigbee, Matter and
  the tunnel, RSA keys that take tens of seconds; audio would drop.

**The way.** Phase 1 with shairport-sync (a known reference, traffic to compare with), then
openairplay2 and shairplay-rust on the same bench against the Mac and an iPhone: video in
sync, pause, volume, wake after standby. If one holds, `maison-audio` is built on it, forked
under uplg like `ashv2` and `ezsp`, and completed (ALAC, persistent pairing); FairPlay is never
rewritten here. If none holds, shairport-sync stays and `maison-audio` only orchestrates.

## Variant: a thin ESP32 next to the speakers

Kept open because the Zero 2 W is out of stock and the Amp2 does not need a Pi: its header
carries I²S, I²C, ground and its own 5 V out. Its TAS5756M is the SqueezeAMP's chip, a board
squeezelite-esp32 and airplay-esp32 already drive. A **classic ESP32-WROVER** (PSRAM, and
Bluetooth Classic for A2DP, which the S3 lacks) wires to it:

| Amp2 header (Pi pin) | Signal | ESP32 |
|---|---|---|
| 2 or 4 | 5 V from the Amp2's regulator | 5V pin (the board's LDO makes 3.3 V) |
| 6 | Ground | GND |
| 12 (GPIO18) | I²S BCK | any I²S-capable GPIO |
| 35 (GPIO19) | I²S LRCK | idem |
| 40 (GPIO21) | I²S data to the DAC | idem |
| 3 / 5 (GPIO2/3) | I²C SDA / SCL (TAS5756M registers) | any GPIO |

No master clock: the TAS5756M locks its PLL on BCK. Check the Amp2's schematic for any other
pin it uses (mute) before wiring.

Two ways to run it, tried in this order:

1. **Standalone: airplay-esp32 on the ESP32.** The Mac and the iPhone play to it over AirPlay 2
   natively (picture and sound kept in sync by AirPlay itself), Bluetooth A2DP takes over when
   AirPlay is idle. Covers the expected use as is, nothing depends on the Mac. Limits: one
   developer's reverse engineering under a non-commercial license (flashed, never vendored
   here), may break with an iOS or macOS release; 48 kHz output (44.1 kHz resampled); what
   Maison can drive (volume, standby) depends on its web API, to check.
2. **The Mac Studio as the hub** (always on). A Rust daemon (launchd) receives AirPlay 2
   (shairplay-rust runs on macOS) under its own name, takes the Mac's own sound from a virtual
   output, and streams lossless audio to the ESP32, which only buffers, plays, sleeps the amp
   and bridges IR. The link: Snapcast (macOS server, ESP32 clients exist but young) or a
   minimal protocol of ours (FLAC or PCM over TCP, fixed buffer: one endpoint needs no
   multi-room sync), Rust on both ends (`esp-idf-svc`). Knowing the endpoint's fixed latency,
   the daemon could in principle announce it to AirPlay senders, keeping video in sync; the
   Mac's own video needs a virtual device that reports that latency (a CoreAudio
   AudioServerPlugIn: BlackHole does not). Costs: silence whenever the Mac sleeps, updates or
   reboots (it must never sleep); macOS cannot be a Bluetooth speaker, so Android still lands
   on the ESP32 (SBC), mixed with the Mac's stream by custom firmware.

Either way the IR bridge moves to the ESP32 (its RMT peripheral decodes IR; POST to
`/api/ir/key` as `kird` does), which frees the AirTies to go. Power is about the Zero 2 W's
(an ESP32 with Wi-Fi up is around half a watt): the gain is parts in stock, the Amp2 kept, and
almost no software next to the speakers. The Amp2's own idle draw is still the number to
measure.

Bench: an ESP32-WROVER dev board, female-female jumper wires, the Amp2 and its 18 V supply,
airplay-esp32 set up as a SqueezeAMP (TAS5756). Sound out and idle draw measured decide
between this variant and the Zero 2 W.

## In Maison

- Backend: `audio.rs`, client through `net.rs` (device errors as `net::unreachable`), its
  address and token in a `Checked` settings file (`audio.json`, as `tv.json`), routes under
  `DEVICE_ROUTES`, refusals coded.
- Web: `web/src/lib/devices/audio/` (`api.ts`, `data.ts` with its `live` source), an
  `AudioGroup` tile: source and now playing, volume as `Range`, mute and standby through
  `Command`, settings in its `settings` snippet. Texts in `i18n/messages`.
- IR keymap actions: `audio_volume` (with `repeat` while held), `audio_mute`, so the
  set-top-box remote drives the speakers. Run by `ir::run_actions`, hence by scenes too.
- Tests: unit and integration against a local stub of `maison-audio`, Vitest for the tile,
  e2e for the flow.

## Shopping list

Already here: the HiFiBerry Amp2 (and the DAC+ ADC Pro, not needed). Buy from a Raspberry Pi
approved reseller (Kubii, Kiwi Electronics, Berrybase…) and an electronics distributor
(Mouser, Farnell, RS…) rather than a marketplace: genuine parts, datasheets, returns.

Prices seen on 2026-10-08, VAT included unless said (Farnell shows prices without VAT).
Amazon pages could not be read: for the generic items, the search words are given.

| # | Item | Exact reference | Where, price | Check before paying |
|---|---|---|---|---|
| 1 | Computer | **Raspberry Pi Zero 2 WH** (header already soldered) | Out of stock everywhere: Kiwi Electronics 20.39 € (back 2026-11-06, max 2, shipping to France from 7.19 €), Kubii 19.50 €, Berrybase 20.90 €, Farnell 13.76 € excl. VAT (delivery April 2027). Amazon: third-party sellers only, several times the price | « 2 », and « WH », not the Zero W nor a Zero 2 W without header |
| 2 | Power supply | **Mean Well GST60A18-P1J** (18 V 3.33 A, 60 W, level VI) | Berrybase 19.90 € (in stock), Farnell ~25 € | Suffix **-P1J** = 5.5 × 2.1 mm plug, centre positive: the Amp2's jack. Not the 24 V model |
| 3 | Mains cord | IEC **C13** cord, EU plug | Amazon « câble alimentation C13 » | The GST60A has a C14 inlet: no cord in the box. Any old PC cord will do |
| 4 | microSD | **SanDisk High Endurance** 32 GB (SDSQQNR-032G) or Samsung PRO Endurance | LDLC 22.29–22.86 €, Materiel.net 26.75 €; Amazon if sold by Amazon itself | A known brand, sold by the shop itself: fake cards are the first cause of a dead Pi |
| 5 | Spacers | M2.5 nylon standoffs, screws and nuts, assorted kit | Amazon « entretoises nylon M2.5 kit » | Two used (the header side), the rest for later |
| 6 | Speaker cable | 2 × 1.5 mm² copper (OFC), the length to both speakers + 50 cm | Amazon « câble haut-parleur 1,5 mm² OFC » | Pure copper (OFC), not CCA (copper-clad aluminium) |
| 7 | Measuring the idle draw | Barrel-jack adapters 5.5 × 2.1 mm with screw terminals, one male, one female | Amazon « connecteur jack DC 5.5x2.1 bornier » (sold in packs) | Goes between supply and Amp2, the multimeter in series on the + (mA range): P = 18 V × I. A plug-in wattmeter is not reliable under ~1 W |
| 8 | Optional | A plug-in energy meter with 0.1 W resolution (PeakTech P 9035 or similar) | | For playing levels and a sanity check, not the idle figure |
| 9 | Optional | A case: the Zero 2 W + Amp2 model on MakerWorld (3D print) | | Or keep it open on the shelf while on the bench |
| 10 | ESP32 variant instead of item 1 | A **classic ESP32-WROVER** dev board (not S3, not WROOM: PSRAM needed), female-female jumper wires | | See [the variant](#variant-a-thin-esp32-next-to-the-speakers) |

Nothing else: no heatsink at these levels, no screen or keyboard (the card is prepared on the
Mac with Raspberry Pi Imager: Wi-Fi and SSH preset), no Bluetooth dongle (built in).

## Phases

1. **Bench.** Zero 2 W + Amp2 + 18 V supply, Raspberry Pi OS Lite to go fast. AirPlay 2 from
   the Mac and an iPhone, Bluetooth from Android (which codecs?), a wattmeter at the wall:
   silent, playing at 1 W, playing loud. Exit: the power gate passed or the amp replaced.
   Then the Rust receivers on the same bench (see the option above): its outcome decides
   whether phase 2 carries shairport-sync and nqptp or not.
2. **Companion OS.** Alpine diskless, shairport-sync, nqptp, bluez-alsa, Wi-Fi settings, a
   deploy like Maison's.
3. **`maison-audio`.** Arbitration, volume, standby, metadata, the API, its tests.
4. **Maison.** Backend, tile, IR actions, tests.
5. **Tuning.** REW measurement at the listening seat, EQ in the Pi's chain.
6. **Later, maybe.** The TV input (above).

## Sources

- Elara LN01: [LDLC](https://ldlc.com/en/product/PB00440788.html), [datasheet](https://www.easylounge.com/pdf/Produits/07658018-0a10-44da-a403-d187083519f2/elara-datasheets-en.pdf)
- shairport-sync: [AirPlay 2](https://git.sudo.is/mirrors/shairport-sync/src/branch/master/AIRPLAY2.md), [README](https://github.com/mikebrady/shairport-sync/blob/master/README.md)
- HiFiBerry: [Amp2 datasheet](https://www.hifiberry.com/docs/data-sheets/datasheet-amp2/), [Amp2 = TAS5756M](https://www.audiophonics.fr/en/amplifier-and-headphone-amplifier-modules/hifiberry-amp2-amplifier-board-stereo-tas5756m-class-d-for-raspberry-pi-2x20w-4-ohm-p-15647.html), [Amp2 models list](https://buyzero.de/en/products/hifiberry-amp2), [Amp4 Pro](https://www.hifiberry.com/docs/data-sheets/datasheet-amp4-pro/), [DAC+ ADC Pro](https://www.hifiberry.com/docs/data-sheets/datasheet-dac-adc-pro/), [Zero 2 W + Amp2 case](https://makerworld.com/models/1013760)
- TI: [TAS5805M](https://www.ti.com/document-viewer/TAS5805M/datasheet), [TAS5754M/56M modulation](https://www.ti.com/product/TAS5754M); Sonocotta: [Louder Raspberry HAT](https://www.tindie.com/products/sonocotta/louder-raspberry-hat/)
- Infineon: [MA12070 eval](https://infineon.com/cms/en/product/evaluation-boards/eval_audio_ma12070), [Pi HAT (end of life)](https://www.infineon.com/cms/en/product/evaluation-boards/kit_40w_amp_hat_zw/)
- Pi Zero 2 W power: [CNX Software](https://www.cnx-software.com/2021/12/09/raspberry-pi-zero-2-w-power-consumption/)
- [Mean Well GST60A](https://elmark-automation.com/shop/mean-well/desktop-power-supply-gst60a)
- Powerline: [TRENDnet TPL-408E](https://downloads.trendnet.com/TPL-408E/datasheets/FR_spec_TPL-408E(v1.0R).pdf), [TP-Link TL-PA7017P](https://www.bruneau.be/fr/product/tp-link-tl-pa7017pkit-fr-1000-mbit-s-ethernet-lan-blanc-2-piece-s-/8073230)
- Bluetooth codecs: [bluez-alsa NEWS](https://github.com/arkq/bluez-alsa/blob/b11cff0c2fd436e64c98b77943bb20a2d3d0ea77/NEWS)
- Rust receivers: [openairplay2](https://docs.rs/crate/openairplay2-receiver/latest/source/README.md), [shairplay-rust](https://github.com/metaneutrons/shairplay-rust), [rairplay](https://github.com/r4v3n6101/rairplay); shairport-sync releases: [Arch 5.0.4](https://archlinux.org/packages/extra/x86_64/shairport-sync/), [Debian 5.1 dev](https://tracker.debian.org/news/1755822/accepted-shairport-sync-51devgit20260518-1-source-into-unstable/)
- ESP32 variant: [squeezelite-esp32](https://github.com/sle118/squeezelite-esp32), [esparagus-snapclient](https://github.com/sonocotta/esparagus-snapclient), [snapclient on Louder boards (Hack42)](https://hack42.nl/wiki/Gebruiker:Stitch/Snap), [BlackHole loopback](https://juergenkoller.software/en/blog/audio-loopback-macos-blackhole-setup.html), [Louder ESP32 (sold out)](https://www.tindie.com/products/sonocotta/louder-esp32/)
- ESP32 path: [airplay-esp32](https://github.com/rbouteiller/airplay-esp32), [Louder ESP32 Mini](https://www.cnx-software.com/2026/06/25/louder-esp32-mini-board-adds-wifi-and-bluetooth-to-old-speakers-for-squeezelite-snapclient-or-esphome-support/)
- Shopping: [Kiwi Zero 2 WH](https://www.kiwi-electronics.com/nl/raspberry-pi-zero-2-w-met-header-20130), [Kubii Zero 2 W/WH](https://www.kubii.com/fr/cartes-nano-ordinateurs/3455-2110-raspberry-pi-zero-2-w-3272496326088.html), [Berrybase Zero 2 WH](https://www.berrybase.de/en/raspberry-pi-zero-2-wh), [Berrybase Mean Well](https://www.berrybase.de/fr/meanwell/), [LDLC SanDisk High Endurance 32 GB](https://www.ldlc.pro/fiche/PB00313629.html), [GST60A18-P1J plug](https://botland.com.pl/en/power-supply-dogniazdkowe/5154-power-supply-meanwell-gs60a18-p1j-18v-333a-dc-plug-55-21mm.html), [Amp2 jack 5.5/2.1](https://www.audiophonics.fr/fr/modules-amplificateurs-et-amplificateurs-casque/hifiberry-amp2-module-amplificateur-stereo-tas5756m-class-d-pour-raspberry-pi-2x20w-4-ohm-p-15647.html), [Zero 2 WH](https://www.kiwi-electronics.com/nl/raspberry-pi-zero-2-w-met-header-20130), [PeakTech P 9035](https://www.peaktech.de/uk/Products/Electrical-engineering/Power-analyzers/Energy-cost-meters/)
- TV: [55PUS6753 connections](https://www.kitele.com/fr/tv-philips-55pus6753-12), [LEAP-S1, Chromecast built-in](https://androidtv-guide.com/streaming-gaming/skyworth-leap-s1), [S/PDIF in and I²S priority switching](https://www.diyaudio.com/community/threads/3-way-dsp-amp.415065/)

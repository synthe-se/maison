# The wall frame

A Frameo photo frame (Rockchip RK3126, four Cortex-A7, 1 GB, an 800×1280 touch panel) that shows
Maison and nothing else. Its Linux is [synthe-se/frame-kernel](https://github.com/synthe-se/frame-kernel),
checked out beside this repository: the kernel, the initramfs, the read-only system, the two
slots, the signed updates, the Wi-Fi service, and the tools that build, try, install and flash
them. The hardware, the loader and what bringing it up taught are in its README and
`docs/frame.md`.

Maison is one product of that frame. Here: what the product is (`frame/product`), its interface
(`frame/ui`, [its architecture](../frame/ui/ARCHITECTURE.md)) and its setup page (`frame/tools`).
Both crates name `../../../frame-kernel/tools` (`frame-tools`).

## The product

| `frame/product/` | |
|---|---|
| `product.conf` | its name, the frame's host name, its boot picture (the app's mark on its ground), where its frames look for updates |
| `rootfs/` | `maison-ui.service` and its launcher, `maison-target.service`, the setup page's unit |
| `preset` | its unit, and the frame's Wi-Fi service, power key and network watch, which its interface uses |
| `defconfig` | Mesa (Lima on KMS), what its SDL opens at run time |
| `build.sh` | the default address of the house, when `FRAME_TARGET` says one |
| `build-target.sh` | the interface and the setup page, built against the frame's libraries |
| `release.pub` | the key its updates are signed with; the private half stays off the repository |

```bash
cd ../frame-kernel
just image ../maison/frame/product      # the system (FRAME_TARGET=https://… : a default address)
just install ../maison/frame/product    # to the frame's free slot, tried, kept or rolled back
just release ../maison/frame/product --publish   # a signed release, for updates over the air
just on 'systemctl status maison-ui'    # a command over SSH
```

## Where the house is

Maison's address is never typed on the frame. It is root's: `/data/maison/target` (0600),
changed over SSH or by the setup page. Without it the image's default applies
(`FRAME_TARGET=https://…` at build writes `/etc/maison-target`). `maison-target` hands the one
that holds to the interface as `/run/maison-target`, its `target` credential (`systemctl restart
maison-ui` runs it again); with neither, the interface says so in plain words.

On the house's LAN the frame reaches Maison at its edge listener (`EDGE_LISTEN`), not out to the
public address and back in: root's `/data/maison/edge` (0700) holds the frame's identity,
`maison-backend edge client <EDGE_DIR> <dir> <name>`'s `ca.pem`, `client.pem`, `client-key.pem`,
and the target is the edge's address (`https://<house>:<port>`). The interface (its `edge`
credential, `frame/ui/src/edge.rs`) then trusts the edge's CA alone, presents its certificate, and
asks the edge's name, sent to that address; an empty directory leaves it a client like any.

The frame is in as a paired wall screen ([passkeys.md](passkeys.md), Wall screens): it shows a
code, an admin gives it from their account page. No token, no person, no passkey.

## The interface

`maison-ui.service` runs `maison-frame-ui`: SDL straight on KMS, no compositor, no session, as
the `kiosk` user, whose groups open the panel and the touch panel (the frame's initramfs gives
them before the system starts); its cookie 0600 in `/data/maison/ui`. The unit has no default
dependencies: it starts with the system, before the network, 5.0 s after the kernel's first
line, its first picture 3.3 s later (SDL and the graphics driver).

- It turns its own picture (`MAISON_ROTATE=180`); its launcher (`/usr/local/sbin/maison-ui`)
  gives SDL the card that has the panel. The touch panel already reports the person's way up:
  it is never inverted when the picture turns.
- SDL is built without a window system (`sdl-unix-console-build`) and hides the cursor.
  xkbcommon's data are needed without a keyboard: no `xkeyboard-config`, no context, the app exits.
- The app links through the frame's `link-gcc`: the sdl3 crate always adds `/usr/local/lib`.
- KMSDRM makes its surface anew at the first present and shows that, blank: the app draws twice
  at its start, or a screen that never changes stays black until a touch.
- On the frame: 92 MB of RAM, a draw in 29 to 34 ms.
- The panel dims after a minute without a finger (a quarter of its level by day, 4 % at night, by
  the sun's side); the first touch only wakes it, the next acts. The app writes
  `/sys/class/backlight` itself: the video group may (the frame's udev rule).
- With photos added from the account page (`Cadre photo`), the panel shows them instead, 5 s
  each, at its full level by day and dimmed at night, from its own copy
  (`/data/maison/ui/photos`, 256 MiB at most); the unit's `ReadWritePaths` covers it. A photo's
  fade draws at 20 images a second, 63 ms at worst.
- It never waits for NTP: after a power cut the clock starts from the one the frame kept, and
  until the house answers the app says so and asks again.
- A wall screen's theme follows the sun at the house when nothing is chosen: dark from sunset to
  sunrise, from the house's place (set with the shutters' sun schedule), read again just after
  each sunrise and sunset.

## Its network

Joining a network happens on the frame's own touch screen, no phone: the interface's Wi-Fi screen
lists the networks in range, a keyboard takes the password, the frame says it connected or why
not (through the frame's `frame-wifid`, the only root behind it). It opens by itself with no
network configured, or when the frame lost its own for 10 minutes (`/run/frame-lost`, never on a
trial), and leaves by itself when one is back. By hand: a link on every door, and on the band
that says Maison is silent. A house that answers offers none: a wall is touched by anyone.
Listed: WPA-PSK and open networks; a WPA3-only network is not.

The hotspot is the last resort of a dead touch panel, the frame's own (its power key held 10 s);
the interface, which reads `FRAME_SETUP_*` as it starts, shows that Wi-Fi and the address of its
page (`maison-setup`, started as `frame-setup.service`). The page takes the house's network and
Maison's address, the kept one already in the form, writes them (0600) and restarts.

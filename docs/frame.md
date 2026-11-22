# The wall frame

A Frameo photo frame (AMABOO GBZK-Y1000: Rockchip RK3126, 1 GB, 32 GB eMMC, 800×1280 MIPI panel,
Android 6.0.1) turned into a screen that shows Maison and nothing else. It runs our own /system,
built from the vendor's: no Frameo, no Adups FOTA, no SuperSU, and a GeckoView kiosk app as the
only home and the device owner. It is in as a paired wall screen ([passkeys.md](passkeys.md),
« Wall screens »): « Utiliser comme écran mural » on its sign-in screen shows a code, an admin
gives it from « Mon compte », Écrans, on the house's Wi-Fi. No token, no person, no passkey.

Everything that is the vendor's, or secret, stays off the repo, in `~/maison-frame/`:
`backup/` (the eMMC up to userdata, each partition, `layout.txt`, the vendor's device tree),
`keys/` (the APK's release key), `bin/rkdeveloptool` (built from rockchip-linux/rkdeveloptool).

## Build

```bash
cd frame/android && MAISON_FRAME_SIGNING=~/maison-frame/keys/release.properties ./gradlew assembleRelease
python3 frame/system/build.py --apk frame/android/app/build/outputs/apk/release/app-release.apk
python3 -m unittest discover -s frame -p 'test_*.py'
```

`build.py` changes the vendor's system image in place with debugfs: what goes (`REMOVED`), the
kiosk in `/system/priv-app/MaisonFrame` with its libraries extracted in `lib/arm`, the boot
animation (`frame/bootanimation.sh`: the sign-in door's identity panel, from the app's sources),
`build.prop` (French, Paris, ADB over Wi-Fi, and `ro.adb.secure=1`: adbd asks every computer
for its key, which the vendor's default.prop left unsaid). Before a flash, compare its tree with
the vendor's: only those paths may differ. It also makes `resource.img`: the vendor's, with U-Boot's logo (the
first screen, read from that partition, not from boot.img's copy) replaced in place by the boot
animation's still frame; the device tree beside it stays byte for byte.

Rotations: U-Boot draws on the raw panel (800×1280); Android's landscape is the panel turned 90°
(`ro.sf.hwrotation`); the boot animation draws in Android's rotation 0, before the locked one
applies; the frame stands at `ROTATION` (1). So the animation's frames turn ROTATION × 90°
clockwise, the logo (1 + ROTATION) × 90°: both seen upright on the frame.

## Flash

```bash
export ANDROID_SERIAL=<serial> RKDEVELOPTOOL=~/maison-frame/bin/rkdeveloptool
frame/flash.sh keys     # once, on the vendor's Android: this computer's ADB key in adb_keys
frame/flash.sh          # adb reboot loader, then resource, recovery and system, chunk by chunk, read back
adb shell pm uninstall net.frameo.frame    # if Frameo had updated itself into /data
ANDROID_SERIAL=<serial> frame/provision.sh https://<Maison's address>   # then pair it on the frame
```

`flash.sh keys` matters: Frameo approved a computer's ADB key for the session only, and this
build's SystemUI, which would ask, is disabled. Without the key in `/data/misc/adb/adb_keys`, ADB
is gone with Frameo; `flash.sh` refuses to start without it. Only /system, resource and
recovery are written: boot and userdata (Wi-Fi, ADB's keys, the kiosk's address) stay. U-Boot
boots the images we pack (a recovery repacked with a changed ramdisk came up), though the
vendor's own boot.img id matches no SHA-1 we could reproduce.

## A root shell without a button

The frame has one button, so the recovery's menu is out of reach; `recovery.img` (build.py) is
the vendor's recovery made a rescue instead: ADB behind this computer's key, mksh and toybox
built from source (`frame/system/rescue-tools.sh`, pinned versions), and root from init.

```bash
frame/rescue.sh                       # adb reboot recovery, then a root prompt, line by line
frame/rescue.sh 'dd if=/dev/block/mmcblk0p13 bs=1M count=1 | md5sum'   # one command
adb -s 0123456789ABCDEF reboot        # back to Maison (the recovery says that serial)
```

Why it is built this way, each point seen on the frame: adbd there is a user build and never
runs as root, and forbids its children any new privilege (a set-uid binary stays uid 2000);
`/sbin` is 0750, closed to adbd's `shell` user, so the tools live in the ramdisk's
`/system/bin` (/system is not mounted); toybox's `nc -L` answers nothing on this 3.10 kernel. So
init's `rootsh` service, root, runs each command written to `/dev/rootsh/in` (0660, group
shell) and answers on `/dev/rootsh/out`; mksh is restricted under a name starting with `r`.

## Booting with the USB-C on a computer

The vendor's U-Boot, powered up with the USB-C linked to a computer, waits for a flashing tool
and then boots the **recovery** (« Usb Timeout, Return for boot recovery! »). To come back to
Android after a power cut: power it with the USB-C unplugged, wait for Maison, then plug it in.

## When nothing answers

No ADB, a system that does not come up: the loader is still there. Power off, hold the reset
pinhole, plug USB-C into a computer, then the power; `rkdeveloptool ld` says `Loader`. The vendor's
system comes back with:

```bash
python3 frame/rkwrite.py ~/maison-frame/backup/mmcblk0p11.img mmcblk0p11 && rkdeveloptool rd
```

(`mmcblk0p3.img` and `mmcblk0p3` for the vendor's logo.)

The loader may drop halfway: do the reset again, `rkwrite.py` resumes where it stopped.

## Afterwards

- The kiosk is updated with `adb install -r` (same release key): there is no root any more.
- `adb shell am broadcast -a maison.frame.PROVISION -n maison.frame/.ProvisionReceiver --es url
  …` gives it a new address; `maison.frame.RELEASE` to the same receiver lifts the kiosk. The
  receiver requires DUMP (the shell's alone) and hands the activity a one-time grant: the
  activity is exported (it is the home), so its own action without the grant is refused.
  Removed from « Mon compte », it offers to pair again.
- A page that could not load is tried again as soon as the Wi-Fi is back (a NetworkCallback).
- The frame stands in portrait, rotation 1: `ROTATION` in `provision.sh` and
  `bootanimation.sh` is one setting.

## Mainline Linux

Maison on the frame without Android. The frame's Linux is no longer here: its kernel, its
initramfs, its system, its updates and the tools that build, try, install and flash them are
`synthe-se/frame-kernel`'s, checked out beside this repository, and what they taught is in its
`docs/frame.md`. Maison is one product of that frame, said in `frame/product/`:

| | |
|---|---|
| `product.conf` | its name, the frame's (`maison-frame.local`), its boot picture (the app's mark on its ground), where its frames look for updates |
| `rootfs/` | `maison-ui.service` and its launcher, `maison-target.service`, the setup page's unit |
| `preset` | its unit, and the frame's Wi-Fi service, power key and network watch, which its interface uses |
| `defconfig` | Mesa (Lima on KMS), what its SDL opens at run time |
| `build-target.sh` | the interface (`frame/ui`) and the setup page (`frame/tools`), built against the frame's libraries |
| `release.pub` | the key its updates are signed with (its private half in `~/maison-frame/keys/maison.pk8`) |

```bash
cd ../frame-kernel
just image ../maison/frame/product      # the system (FRAME_TARGET=https://… : a default address)
just install ../maison/frame/product    # to the frame's free slot, tried, kept or rolled back
just release ../maison/frame/product --publish   # a signed release, for updates over the air
just on 'systemctl status maison-ui'    # a command over SSH
```

Maison's address is never typed on the frame. It is root's: `/android-data/maison/target` (0600),
changed over SSH or by the setup page. Without it the image's default applies
(`FRAME_TARGET=https://…` at build writes `/etc/maison-target`). `maison-target` hands the one
that holds to the interface as `/run/maison-target`, its `target` credential (`systemctl restart
maison-ui` runs it again); with neither, the interface says so in plain words.
On the house's LAN the frame reaches Maison at its edge listener (`EDGE_LISTEN`), not out to the
public address and back in: root's `/android-data/maison/edge` (0700) holds the frame's identity,
`maison-backend edge client <EDGE_DIR> <dir> <name>`'s `ca.pem`, `client.pem`, `client-key.pem`,
and the target is the edge's address (`https://<house>:<port>`). The interface (its `edge`
credential, `frame/ui/src/edge.rs`) then trusts the edge's CA alone, presents its certificate, and
asks the edge's name, sent to that address; an empty directory leaves it a client like any.

The native interface (`frame/ui`, [its architecture](../frame/ui/ARCHITECTURE.md)):
`maison-ui.service` runs `maison-frame-ui`, SDL straight on KMS with no compositor and no session,
as the `kiosk` user, whose groups open the panel and the touch panel (the frame's initramfs gives
them before the system starts); its cookie 0600 in `/android-data/maison/ui`. It starts with the
system, before the network: 5.0 s after the kernel's first line, its first picture 3.3 s later
(SDL and the graphics driver; it was 15.5 s to start under a logind session ordered behind the
network). It turns its own picture (`MAISON_ROTATE=180`); its launcher
(`/usr/local/sbin/maison-ui`) gives SDL the card that has the panel. SDL is built without a window
system (`sdl-unix-console-build`), and hides the cursor on the panel. xkbcommon's data are needed
without a keyboard: no `xkeyboard-config`, no context, the app exits. The app links through the
frame's `link-gcc`: the sdl3 crate always adds `/usr/local/lib`. Measured against cage on the
frame: 99 to 92 MB of RAM, a draw 28 to 29-34 ms; what compositing itself cost is not measured.

- The panel dims after a minute without a finger (a quarter of its level by day, 4 % at night, by
  the sun's side); the first touch only wakes it, the next acts. The app writes
  /sys/class/backlight itself: the video group may (the frame's udev rule).
- With photos added from « Cadre photo », the panel shows them instead, 5 s each, at its full
  level by day and dimmed at night, from its own copy (`/android-data/maison/ui/photos`, 256 MiB
  at most); the unit's `ReadWritePaths` covers it. A photo's fade draws at 20 images a second,
  63 ms at worst.
- It never waits for NTP: after a power cut the clock starts from the one the frame kept, and
  until the house answers the app says so and asks again.

A wall screen's theme follows the sun at the house when nothing is chosen (« Soleil », which
anyone may choose too): dark from sunset to sunrise, from the house's place (set with the
shutters' sun schedule), read again just after each sunrise and sunset.

Joining a network happens on the frame's own touch screen, no phone: the interface's Wi-Fi screen
lists the networks in range, a keyboard takes the password, the frame says it connected or why
not (through the frame's `frame-wifid`). It opens by itself with no network configured, or when
the frame lost its own, and leaves by itself when one is back. By hand: a link on every door, and
on the band that says Maison is silent. A house that answers offers none: a wall is touched by
anyone. Listed: WPA-PSK and open networks; a WPA3-only network is not.

The hotspot is the last resort of a dead touch panel, the frame's own (its power key held 10 s);
the interface, which reads `FRAME_SETUP_*` as it starts, shows that Wi-Fi and the address of its
page (`maison-setup`, http://192.168.4.1, started as `frame-setup.service`). The page takes the
house's network and Maison's address, the kept one already in the form, writes them (0600) and
restarts.

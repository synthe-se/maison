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

Maison on the frame without Android: Linux 7.3 (`frame/linux`) from the **boot** slot, what a
power-up starts (a power cut included); the vendor's Android sits in the **recovery** slot
(`mmcblk0p5.img` of the backup), kept whole. The system, Buildroot with the native interface on
KMS, is a read-only squashfs in Android's userdata, a tmpfs over it. Linux updates itself: no
Android, no ADB.

```bash
frame/linux/build.sh            # kernel, device tree, the AIC8800 Wi-Fi modules (Docker)
frame/linux/buildroot.sh        # the system: frame/build/buildroot/rootfs.squashfs (Docker, ~25 min from nothing)
python3 -I frame/linux/initramfs.py [--debug] frame/build/tools ~/maison-frame/recon/gslX680.fw frame/build/linux-initramfs.cpio.gz
frame/linux/build.sh --initramfs frame/build/linux-initramfs.cpio.gz   # the kernel of `boot`, holding it
python3 -I frame/linux/image.py frame/build/linux/zImage.boot frame/build/linux/rk3126-amaboo-gbzk-y1000.dtb frame/build/boot-linux.img
frame/linux/try.sh --install [--kernel]   # an update to the free slot, tried, kept or rolled back
frame/linux/try.sh --ram        # a new kernel and initramfs run from RAM (kexec): a power cut returns to boot's
frame/linux/try.sh --logo       # U-Boot's picture, in the resource partition
frame/linux/try.sh --flash      # a new kernel or initramfs: to boot, from Linux (addressing proved), else the loader
frame/linux/console.sh 'systemctl status maison-ui'   # a command over SSH (FRAME_HOST=<address> without mDNS)
frame/linux/release.sh [--kernel] [--publish]   # a signed release, for updates over the air
frame/linux/ssh-cert.sh         # a certificate for 8 hours (console.sh and send.sh ask for it themselves)
```

`--debug` is for bring-up: a root console on the USB-C, the backlight blinks five times as soon as
init runs (a sign with no console at all) and the initramfs waits 8 s before saying what came up.

Maison's address is never typed on the frame. It is root's: `/android-data/maison/target` (0600),
changed over SSH, or adopted once from where Android's shell writes, `/data/local/tmp/maison/target`,
while root keeps none. Without it the image's default applies: `FRAME_TARGET=https://… frame/linux/buildroot.sh`
writes `/etc/maison-target`. `maison-wifi` hands the one that holds to the interface as
`/run/maison-target`, its `target` credential (`systemctl restart maison-ui` runs it again); with
neither, the interface says so in plain words.
The Wi-Fi takes the network joined on the frame (`wifi.conf`), else the ones Android knows (its own
`wpa_supplicant.conf`).

There is no console on a product's boot: no getty, no shell on the USB-C, root's password locked.
`initramfs.py --debug` builds an image with a root shell on the USB-C, for bring-up only. A
certificate over SSH is the way in.

What the bring-up taught, each point seen on the frame:

- The panel stays black under a continuous DSI clock: the JD9365 is reset while the host's HS
  clock runs and never locks on the video. `MIPI_DSI_CLOCK_NON_CONTINUOUS` (panel.py).
- Switching PD_VIO off faults (a PMU register this chip lacks): no power domain on the VOP, the
  DSI and its PHY.
- The AIC8800DC answers only without the SoC's pulls on the SDIO lines (the board has its own),
  and with the vendor's power-on wait. No RTC on the board, despite the vendor's tree.
- No partition table mainline reads: userdata alone is declared (`blkdevparts=` in the kernel's
  command line, which has 1024 bytes, 700 of them U-Boot's).
- The touch chip's axes are swapped, 1024 down by 640 across; `touchscreen-size-*` are the
  chip's axes, before the swap.
- simpledrm left beside Rockchip's card is a second card, where a client would draw unseen.
- U-Boot draws its logo only with the vendor's device tree, kept in the boot image's resource, and it
  takes the picture from the `resource` partition (`try.sh --logo`), not from the image. The kernel
  ignores that device tree: ours follows the zImage (`ARM_APPENDED_DTB`). U-Boot moves an image's
  initramfs (0x65bf0000 here) and says so only in its own device tree, so the kernel of `boot` holds
  its initramfs (`build.sh --initramfs`); none of U-Boot's command line reaches it either.
- Powered up with the USB-C on a computer, U-Boot still boots `boot` (Linux); `misc` is ignored
  (a power cycle is the boot slot's): the loader comes from the reset pinhole.
- No battery and a system in RAM: the clock came back six months behind after a power cut, and
  the app waited minutes for NTP. timesyncd's clock file is kept on userdata
  (`systemd-timesyncd.service.d/maison.conf`), and the app never waits for NTP: it retries.
- A file goes over the Wi-Fi by send.sh, from Linux: scp, then the frame checks the SHA-256 said
  over the console before it renames the file. A new system (install.sh) and a new boot image
  (try.sh --flash) go that way; boot is then written by Linux on /dev/mmcblk0 once the layout's
  addressing is proved (U-Boot's head where layout.txt says, boot's `ANDROID!` header), read back
  and compared. The loader stays the way back.
- The boot pictures (splash.py): U-Boot draws Synthèse's mark; as soon as Linux has the panel, the
  initramfs draws Maison's mark with a ring turning under it (loader.c, a static binary, outliving the initramfs)
  until `maison-ui` starts. The kernel has no console on the panel: no text at boot, the log stays
  in the journal. A line appears only on failure (« Système introuvable… »).
- SSH: root by a certificate of the frames' CA only (`ssh-cert.sh`, 8 hours, the CA in
  `~/maison-frame/keys`): no key on any frame, reachable from private networks only, the host key
  made once on the frame and kept on userdata. The frame answers as `maison-frame.local` (mDNS); its
  Wi-Fi address comes from the eMMC's serial (maison-mac.service): the AIC8800 has none, the driver
  drew one per boot (a new lease, a new IP).
- No cable, the tools go over SSH: console.sh takes the USB console when it is there, else
  `ssh root@maison-frame.local`; install, flash and journals work with the frame on the wall.
- The panel dims after a minute without a finger (a quarter of its level by day, 4 % at
  night, by the sun's side); the first touch only wakes it, the next acts. The app writes
  /sys/class/backlight itself: the video group may (70-maison-backlight.rules).
- With photos added from « Cadre photo », the panel shows them instead, 5 s each, at its full level by day and dimmed
  at night, from its own
  copy (`/android-data/maison/ui/photos`, 256 MiB at most); the unit's `ReadWritePaths` covers it.
- The journal is kept on userdata (`/var/log/journal`, written every minute, at once for errors):
  a boot that went wrong reads at the next one. The initramfs derives the machine id from the
  eMMC's serial, so that it stays one directory, within its 64 MB.

The native interface (`frame/ui`, [its architecture](../frame/ui/ARCHITECTURE.md)) is in the
system: `maison-ui.service` runs `maison-frame-ui` on tty7, SDL straight on KMS with no
compositor, as the `kiosk` user in a logind session, its cookie 0600 in `/android-data/maison/ui`.
It turns its own picture (`MAISON_ROTATE=180`); its launcher (`/usr/local/sbin/maison-ui`) gives
SDL the card that has the panel. SDL is built without a window system
(`sdl-unix-console-build`), and hides the cursor on the panel. Measured against cage on the frame:
99 to 92 MB of RAM, a draw 28 to 29-34 ms, first picture unchanged at 1.1 s; what compositing itself
cost is not measured.

A wall screen's theme follows the sun at the house when nothing is chosen (« Soleil », which
anyone may choose too): dark from sunset to sunrise, from the house's place (set with the
shutters' sun schedule), read again just after each sunrise and sunset.

The kernel is `allnoconfig` + `maison.config`: every symbol is said there, a dropped one and any
warning of the build fail `build.sh`. `VMSPLIT_2G`: all of the 1 GB directly mapped, no highmem.

Nothing drives the CPU's rail: the vendor's tree names a regulator on PWM1, its kernel never got
it, and PWM1's registers read zero. Its voltage is unknown, so it is never written: `cpu-supply`
stays out, and the OPPs change the clock alone. There is no temperature sensor either. A
frequency enters the device tree only after holding under load on the frame, each result checked
against a reference: 1008 MHz, the vendor's own top, held 8 minutes on all four cores. Above it
is unproven.

The system is Buildroot (`buildroot/maison_defconfig`, a pinned release, `0001-*.patch` for what it
ships behind), every program tuned for the Cortex-A7. What its build taught, each point seen:

- Buildroot's `preset-all` runs after the post-build, on the image's copy: an `enable` there is
  undone. What runs is said in the preset the post-build writes.
- Its timesyncd has no servers: they are named in `timesyncd.conf.d/maison.conf`. The clock comes
  back from the file on userdata before any network, then NTP about 50 s later; `clock-epoch` is
  the floor if that file is lost.
- A system shares userdata with the one before it: `kiosk` stays uid 988 (its `screen-cookie`), and
  `maison-timesync` gives the clock file to this system's user.
- Its fstab mounts `/` read-only, which turns the overlay read-only: the line is removed.
- xkbcommon's data are needed without a keyboard: no `xkeyboard-config`, no context, the app exits.
- The app links through `buildroot/link-gcc`: the sdl3 crate always adds `/usr/local/lib`.

Updates go to one of two slots on userdata, `/android-data/maison/slots/{a,b}`, root's: a system,
and with `--kernel` a kernel, its device tree and its initramfs. `boot` is written by none of them,
only by `try.sh --flash`: its kernel is what any failure comes back to. `state` says `active`,
`pending`, `tries` and `boots`. The initramfs counts a try before it starts one (2 at first: a power cut
spends one), starts a slot's kernel by kexec, and watches from outside (`guard.c`: 240 s, then a
restart, whatever became of PID 1; for a kernel, the SoC's watchdog left running, 30 s to reach
systemd, the kernel not feeding it before). `maison-commit` keeps an update once the interface
has drawn (its heartbeat) without a restart, with the Wi-Fi and SSH, for 45 s, else restarts within
150 s; a system already proved only has to draw, so that a router out of order is no failure.
Tries spent, the slot goes to `slots/failed` and the frame stays on the one before. Each boot of
the active slot is counted the same way: 4 without a proof and the frame moves to the other, 8 and
it stops on the failure's line. All of a slot is checked against its `sums` at every boot, the Wi-Fi driver loading beside it when no slot holds a kernel (a kexec wants the hardware free). e2fsck takes `broken_system_clock`: no RTC, a clock at 1970 must force no full check. A slot's kernel adds a kexec to each boot until it is flashed to `boot`. A flash that dies
midway still wants the reset pinhole.

Over the air: a release is `manifest.json`, its Ed25519 signature `manifest.sig` and its files under
`<version>/`, static files over HTTPS, made by `frame/linux/release.sh [--kernel]` (the key in
`~/maison-frame/keys/release.pk8`, its public half in `frame/tools/release.pub`, in the image).
`FRAME_UPDATE_URL` at build names where, by default `https://updates.synthe.se/frame`
(mailing-infra's `updates` site, `release.sh --publish`); set empty, the frame has none. Each night
(`maison-update.timer`, 3 to 4 am) the frame reads the manifest, refuses what is not signed, not
newer, or once failed, downloads to the free slot, resumed if cut, and restarts to try it.

Failures provoked on the frame, each back on the good slot by itself: an image that fails its
`sums` (27 s), an unreadable image with valid `sums` (39 s), a kernel that panics (49 s), a kernel
that hangs (73 s, a real watchdog reset). A good `--kernel` slot boots by kexec under that watchdog.
An update over the air, from updates.synthe.se to the frame: 80 s.

Joining a network happens on the frame's own touch screen, no phone: the interface's Wi-Fi screen
lists the networks in range, a keyboard takes the password, the frame says it connected or why not.
It opens by itself with no network configured, or when the frame lost its own, and leaves by itself
when one is back. By hand: a link on every door, and on the band that says Maison is silent. A house
that answers offers none: a wall is touched by anyone. Listed: WPA-PSK and open networks, what
`wifi.conf` can say; a WPA3-only network is not. An image's factory install: `rootfs.squashfs` left
in `/data/local/tmp/maison`, adopted once into slot a while userdata holds no state.

The interface stays `kiosk`: `maison-wifid`, root, answers it over `/run/maison-wifi/socket` (the
directory 0750 root:kiosk), one JSON line each way: scan, list, connect, status, nothing else
(`frame/tools/src/wifi.rs`). It is the boundary: a name goes in hex, a password only as its 802.11i
key (`psk`, also what `wifi.conf` holds: never the password), a request is at most 1 KiB, unknown fields are refused.
It speaks to wpa_supplicant's control socket directly (no wpa_cli). A network is joined live and
written to `wifi.conf` (0600, atomic, as `maison-setup` writes it) only once associated: a wrong
password loses nothing, the others are enabled again.

Network lost (`maison-wifi-watch`, the kernel's `operstate`, not carrier: a wrong password raises
carrier too): 10 minutes without associating, after boot or a lost link, `/run/maison-lost` says so
and `maison-wifid` tells the interface, at its next look (15 s). An association removes it. The photos
go on over it; a finger finds the Wi-Fi screen. An update on trial never counts: `maison-commit` judges
it, within its 150 s, before the 10 minutes.

The hotspot is the last resort of a dead touch panel. It stays in the image and never starts by
itself: only the power key held 10 s opens it (`maison-power-key`, gpio-keys' `KEY_POWER`, then
`maison-hotspot open`; `logind.conf.d/maison.conf` keeps logind off the key). `/run/maison-hotspot`
makes `maison-wifi` take its access-point branch: the frame is its own Wi-Fi, named and locked from
its eMMC's serial; wpa_supplicant and networkd restart, and `maison-ui`, which reads `MAISON_SETUP_*`
as it starts, shows both and the address of its page (`maison-setup`, http://192.168.4.1). The page
takes the house's network and Maison's address, the kept one already in the form, writes them to
`/android-data/maison` (0600) and restarts. 30 minutes after its last request, `maison-hotspot close`
returns to the configured network without a restart.

Seen on the frame: scan and list, a wrong password then the right one, the power key held 10 s, the
access point on the AIC8800, the setup view and its page's form from a phone, the way back to the
network 30 minutes after, without a restart. A photo's fade draws at 20 images a second, 63 ms at worst.

A frozen frame restarts alone: systemd feeds the SoC's watchdog (`RuntimeWatchdogSec`), an oops
panics, a panic reboots after 5 s.

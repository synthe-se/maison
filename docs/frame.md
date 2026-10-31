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
KMS, is a read-only squashfs in Android's userdata, a tmpfs
over it. Linux updates itself: no Android, no ADB.

```bash
frame/linux/build.sh            # kernel, device tree, the AIC8800 Wi-Fi modules (Docker)
frame/linux/buildroot.sh        # the system: frame/build/buildroot/rootfs.squashfs (Docker, ~25 min from nothing)
python3 -I frame/linux/initramfs.py [--debug] frame/build/tools ~/maison-frame/recon/gslX680.fw frame/build/linux-initramfs.cpio.gz
python3 -I frame/linux/image.py --slot boot frame/build/linux/zImage frame/build/linux/rk3126-amaboo-gbzk-y1000.dtb frame/build/linux-initramfs.cpio.gz frame/build/boot-linux.img
frame/linux/try.sh --install [--kernel]   # an update to the free slot, tried, kept or rolled back
frame/linux/try.sh --ram        # a new kernel and initramfs run from RAM (kexec): a power cut returns to boot's
frame/linux/try.sh --flash      # a new kernel or initramfs: to boot, from Linux (addressing proved), else the loader
frame/linux/console.sh 'systemctl status maison-ui'   # a command over SSH (FRAME_HOST=<address> without mDNS)
ssh root@maison-frame.local     # the same over Wi-Fi, key only
```

`--debug` is for bring-up: a root console on the USB-C, the backlight blinks five times as soon as
init runs (a sign with no console at all) and the initramfs waits 8 s before saying what came up.

Maison's address is root's: `/android-data/maison/target` (0600), loaded by the units as their
`target` credential. `install.sh --target` can only write where Android's shell writes,
`/data/local/tmp/maison/target`; the next mainline boot's initramfs adopts it, only while root
keeps none, and removes it. The units never read the old place, so a system built since wants
the recovery image built since (its initramfs adopts): flash both. To change the address,
remove root's copy from the console (`frame/linux/console.sh 'rm /android-data/maison/target'`),
then `--target` again and reboot.

There is no console on a product's boot: no getty, no shell on the USB-C, root's password locked.
`initramfs.py --debug` builds an image with a root shell on the USB-C, for bring-up only. SSH by
key is the way in.

The Wi-Fi takes the networks Android knows (its own `wpa_supplicant.conf`).

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
- WebKit's sandbox needs user namespaces; cog's modeset renderer cannot rotate (gles does);
  simpledrm left beside Rockchip's card is where cog would draw, unseen.
- `boot` holds 14 MiB: our image fits only without the vendor's 3 MB logo in its resource
  (`image.py --slot boot`, `bootimg.rsce_only`). U-Boot draws its first screen from the image it
  boots, not from the resource partition: the screen stays black until Linux lights it.
- Powered up with the USB-C on a computer, U-Boot still boots `boot` (Linux); `misc` is ignored
  (a power cycle is the boot slot's). Linux has no way to ask U-Boot for another slot yet: the
  loader comes from the reset pinhole.
- No battery and a system in RAM: the clock came back six months behind after a power cut, and
  the app waited minutes for NTP. timesyncd's clock file is kept on userdata
  (`systemd-timesyncd.service.d/maison.conf`), and the app never waits for NTP: it retries.
- A file goes over the Wi-Fi by send.sh, from Linux: scp, then the frame checks the SHA-256 said
  over the console before it renames the file. A new system (install.sh) and a new boot image
  (try.sh --flash) go that way; boot is then written by Linux on /dev/mmcblk0 once the layout's
  addressing is proved (U-Boot's head where layout.txt says, boot's `ANDROID!` header), read back
  and compared. The loader stays the way back.
- The boot picture (splash.py): the initramfs draws Maison's mark on /dev/fb0 as soon as the
  panel is up. The kernel has no console on the panel: no text at boot, the log stays in the
  journal. A ring turns under the mark (loader.c, a static binary started by the initramfs,
  outliving it) until `maison-ui` starts. A line appears only on failure (« Système introuvable… »).
- SSH: OpenSSH, root by key only (the post-build installs `~/.ssh/id_ed25519.pub`,
  `FRAME_SSH_KEY`), no password; the host key made once on the frame, kept on userdata. The
  frame answers as `maison-frame.local` (mDNS); its Wi-Fi address comes from the eMMC's serial
  (maison-mac.service): the AIC8800 has none, the driver drew one per boot (a new lease, a new IP).
- Without a host on the USB-C (the frame on its own power), /dev/ttyGS0 never opens: the
  console's getty must open it itself, non-blocking (`StandardInput=null`, `agetty -L`). With
  systemd's `StandardInput=tty` the unit's child blocked opening it while holding the lock on
  /dev/console, PID 1 waited on that lock for ever, and nothing else started: the app neither
  (`chvt 7` waiting, cage waiting for its session). Found with the journal kept and SSH.
- No cable, the tools go over SSH: console.sh takes the USB console when it is there, else
  `ssh root@maison-frame.local`; install, flash and journals work with the frame on the wall.
- The panel dims after 2 minutes without a finger (a quarter of its level by day, 4 % at
  night, by the sun's side); the first touch only wakes it, the next acts. The app writes
  /sys/class/backlight itself: the video group may (70-maison-backlight.rules).
- The journal is kept on userdata (`/var/log/journal`, written every 5 s): a boot that went wrong
  reads at the next one. Each boot has its own machine id (the system lives in RAM): read one with
  `journalctl -D /var/log/journal/<id>`.

The native interface (`frame/ui`, [its architecture](../frame/ui/ARCHITECTURE.md)) is in the
system: `maison-ui.service` runs cage on tty7 with `maison-frame-ui` as its one
client, as the `kiosk` user in a logind session, its cookie 0600 in `/android-data/maison/ui`;
its launcher (`/usr/local/sbin/maison-ui`) turns the output with `wlr-randr` (`MAISON_TURN`, 0
or 180, else the app turns its own picture).

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
- Debian's system stays as `rootfs-debian.squashfs`, and `rootfs.sh` still builds it.

Updates go to one of two slots on userdata, `/android-data/maison/slots/{a,b}`, root's: a system,
and with `--kernel` a kernel, its device tree and its initramfs. `boot` is written by none of them,
only by `try.sh --flash`: its kernel is what any failure comes back to. `state` says `active`,
`pending` and `tries`. The initramfs counts a try before it starts one (2 at first: a power cut
spends one), starts a slot's kernel by kexec, and watches from outside (`guard.c`: 240 s, then a
restart, whatever became of PID 1; for a kernel, the SoC's watchdog left running, and
`WATCHDOG_OPEN_TIMEOUT` if systemd never opens it). `maison-commit` makes the slot active once the
interface runs without a restart, with the network and SSH, for 45 s, else restarts within 150 s.
Tries spent, the slot goes to `slots/failed` and the frame stays on the one before. Seen on the
frame: a trial that succeeds. Not yet: a failing system, a kernel that panics or hangs, a watchdog
reset. A slot's kernel adds a kexec to each boot until it is flashed to `boot`. A flash that dies
midway still wants the reset pinhole.

A frozen frame restarts alone: systemd feeds the SoC's watchdog (`RuntimeWatchdogSec`), an oops
panics, a panic reboots after 5 s.

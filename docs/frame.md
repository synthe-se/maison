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
`build.prop` (French, Paris, ADB over Wi-Fi). Before a flash, compare its tree with the vendor's:
only those paths may differ. It also makes `resource.img`: the vendor's, with U-Boot's logo (the
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
pinhole, plug USB-C into the Mac, then the power; `rkdeveloptool ld` says `Loader`. The vendor's
system comes back with:

```bash
python3 frame/rkwrite.py ~/maison-frame/backup/mmcblk0p11.img mmcblk0p11 && rkdeveloptool rd
```

(`mmcblk0p3.img` and `mmcblk0p3` for the vendor's logo.)

The loader may drop halfway: do the reset again, `rkwrite.py` resumes where it stopped.

## Afterwards

- The kiosk is updated with `adb install -r` (same release key): there is no root any more.
- `adb shell am broadcast -a maison.frame.PROVISION -n maison.frame/.ProvisionReceiver --es url
  …` gives it a new address; `maison.frame.RELEASE` lifts the kiosk. Removed from « Mon
  compte », it offers to pair again.
- The frame stands in portrait, rotation 1: `ROTATION` in `provision.sh` and
  `bootanimation.sh` is one setting.

## Mainline Linux (in progress)

Maison on the frame without Android: Linux 7.2 (`frame/linux`) from the **recovery** slot, so
Android stays whole and boots after a power cut; the system, Debian trixie armhf with cog (WPE
WebKit) on KMS, is a read-only squashfs in Android's userdata, a tmpfs over it.

```bash
frame/linux/build.sh            # kernel, device tree, the AIC8800 Wi-Fi modules (Docker)
frame/linux/rootfs.sh           # the system: rootfs.squashfs (Docker, armhf emulated, ~20 min)
python3 -I frame/linux/initramfs.py frame/build/tools ~/maison-frame/recon/gslX680.fw frame/build/linux-initramfs.cpio.gz
python3 -I frame/linux/image.py frame/build/linux/zImage frame/build/linux/rk3126-amaboo-gbzk-y1000.dtb frame/build/linux-initramfs.cpio.gz frame/build/recovery-linux.img
frame/linux/install.sh --target https://<maison>   # once: the system and Maison's address
frame/linux/try.sh --install --flash                # then: on the frame, booted, its console up
frame/linux/console.sh 'systemctl status maison-kiosk'   # a root shell over the USB-C
```

The kiosk opens Maison's address with `#lang=fr` (WPE says `en-US` whatever its system), and is
paired as a wall screen like the Android one; its cookies, WebKit's storage and cache persist in
userdata's `/data/maison/kiosk` (mounted read-write; the system itself is read-only). The Wi-Fi
takes the networks Android knows (its own `wpa_supplicant.conf`).

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
- `reboot` from mainline brings Android even with the USB-C plugged in; `try.sh` handles either.

Still to do: the theme following the sun (WebKit says light, whatever the hour), the CPU above
U-Boot's 600 MHz (its voltage), the boot's speed.

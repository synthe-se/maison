# The wall frame

A Frameo photo frame (AMABOO GBZK-Y1000: Rockchip RK3126, 1 GB, 32 GB eMMC, 800×1280 MIPI panel,
Android 6.0.1) turned into a screen that shows Maison and nothing else. It runs our own /system,
built from the vendor's: no Frameo, no Adups FOTA, no SuperSU, and a GeckoView kiosk app as the
only home and the device owner. It signs in with a service token ([passkeys.md](passkeys.md),
« Service tokens »), which the web app keeps (`web/src/lib/screen.ts`): no person, no passkey.

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
frame/flash.sh          # adb reboot loader, then resource.img and system.img, chunk by chunk, read back
adb shell pm uninstall net.frameo.frame    # if Frameo had updated itself into /data
ANDROID_SERIAL=<serial> frame/provision.sh https://<Maison's address>   # asks for the token
```

`flash.sh keys` matters: Frameo approved a computer's ADB key for the session only, and this
build's SystemUI, which would ask, is disabled. Without the key in `/data/misc/adb/adb_keys`, ADB
is gone with Frameo; `flash.sh` refuses to start without it. Only /system and resource are
written: boot, recovery and userdata (Wi-Fi, ADB's keys, the kiosk's address) stay.

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
  … --es token …` gives it a new address or token; `maison.frame.RELEASE` lifts the kiosk.
- The frame stands in portrait, rotation 1: `ROTATION` in `provision.sh` and
  `bootanimation.sh` is one setting.

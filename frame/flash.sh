#!/usr/bin/env bash
# Writes frame/build's system.img, resource.img (U-Boot's logo) and recovery.img (the rescue
# shell; boot.img too, when build.py made one) to the frame over USB-C,
# in Rockchip's loader mode, through frame/rkwrite.py: the loader's addressing proved against the
# backup before anything is written, 16 MiB chunks each read back, resumed if the loader drops.
# The rest of the eMMC (loader, uboot, kernel, resource, recovery, userdata) is left as it is:
# Wi-Fi, ADB's authorised keys (/data/misc/adb/adb_keys) and the apps' data stay.
#
# ADB must not depend on Frameo, which approves a computer's key for the session only (and
# SystemUI, which would ask, is disabled on this build): before the first flash, on the vendor's
# Android while its root is there, this computer's key goes into adb_keys, where adbd itself
# reads it: `frame/flash.sh keys`. The flash refuses to start without it.
# Usage: frame/flash.sh [keys]     BACKUP (~/maison-frame/backup), RKDEVELOPTOOL (rkdeveloptool)
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$REPO/frame/build"
export BACKUP="${BACKUP:-$HOME/maison-frame/backup}"
export RKDEVELOPTOOL="${RKDEVELOPTOOL:-rkdeveloptool}"
KEY="$HOME/.android/adbkey.pub"
# rkdeveloptool keeps a log/ in the current directory: a temporary one, never the repo
cd "$(mktemp -d)"

if [ "${1:-}" = keys ]; then
	adb push "$KEY" /data/local/tmp/adb_keys >/dev/null
	adb shell "su -c 'cp /data/misc/adb/adb_keys /data/local/tmp/adb_keys.vendor; cat /data/local/tmp/adb_keys > /data/misc/adb/adb_keys && chown system:shell /data/misc/adb/adb_keys && chmod 640 /data/misc/adb/adb_keys && restorecon /data/misc/adb/adb_keys; rm /data/local/tmp/adb_keys'"
	adb shell "su -c 'cut -d\" \" -f2 /data/misc/adb/adb_keys'"
	exit 0
fi

[ -f "$BUILD/system.img" ] || { echo "$BUILD/system.img: run frame/system/build.py first" >&2; exit 1; }
if ! "$RKDEVELOPTOOL" ld 2>/dev/null | grep -q Loader; then
	adb shell "su -c 'cat /data/misc/adb/adb_keys'" | tr -d '\r' | grep -q -F "$(cut -d' ' -f1 "$KEY")" ||
		{ echo "this computer's ADB key is not in the frame's adb_keys: frame/flash.sh keys" >&2; exit 1; }
	echo "to the loader…"
	adb reboot loader
fi
[ -f "$BUILD/boot.img" ] && python3 -I "$REPO/frame/rkwrite.py" "$BUILD/boot.img" mmcblk0p5
[ -f "$BUILD/resource.img" ] && python3 -I "$REPO/frame/rkwrite.py" "$BUILD/resource.img" mmcblk0p3
[ -f "$BUILD/recovery.img" ] && python3 -I "$REPO/frame/rkwrite.py" "$BUILD/recovery.img" mmcblk0p6
python3 -I "$REPO/frame/rkwrite.py" "$BUILD/system.img" mmcblk0p11
"$RKDEVELOPTOOL" rd >/dev/null
echo "rebooting into Maison"

#!/usr/bin/env bash
# Puts the frame's Linux system where its initramfs looks for it, from Android over ADB (no root,
# no partition written): rootfs.squashfs in /data/local/tmp/maison, pushed under a temporary name
# then renamed (a copy cut halfway never replaces the image). With --target, also Maison's address,
# the one thing the kiosk is given: it is in as a wall screen paired from its sign-in screen
# (docs/passkeys.md « Wall screens »), no token to carry. The address lands where Android's shell
# writes, /data/local/tmp/maison/target; the next mainline boot's initramfs adopts it into root's
# /data/maison/target (what the units read) and removes it, but only while root keeps none: to
# change it later, remove root's first from the mainline console
# (`frame/linux/console.sh 'rm /android-data/maison/target'`), then --target again and reboot.
# Usage: frame/linux/install.sh [--kernel]       from Linux, an update
#        frame/linux/install.sh [--target https://home.example]   from Android, the first install
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
IMG="$REPO/frame/build/buildroot/rootfs.squashfs"
DIR=/data/local/tmp/maison
DATA=/android-data/local/tmp/maison
SERIAL="${ANDROID_SERIAL:-}"
a() { adb ${SERIAL:+-s "$SERIAL"} "$@"; }

[ -f "$IMG" ] || { echo "no $IMG: frame/linux/buildroot.sh first" >&2; exit 1; }

# Linux up: the update goes to the slot not in use, over the Wi-Fi (send.sh: the SHA-256 checked
# before the rename), to be tried at the next boot (initramfs.py, maison-commit). --kernel sends
# this build's kernel with it; without it, the slot runs on what boots it. Android is not needed.
if "$REPO/frame/linux/console.sh" true 10 >/dev/null 2>&1; then
	case "${1:-}" in "" | --kernel) ;; *) echo "$1: from Linux, only --kernel" >&2; exit 2 ;; esac
	c() { "$REPO/frame/linux/console.sh" "$@"; }
	slots=/android-data/maison/slots
	# the first update makes the slots of what is installed
	c "[ -s $slots/state ] || { mkdir -p $slots/a && chmod 700 $slots && mv $DATA/rootfs.squashfs $slots/a/ && printf 'active=a\npending=\ntries=0\n' >$slots/state; sync; }" 30
	active=$(c "sed -n 's/^active=//p' $slots/state" 20)
	case "$active" in a) slot=b ;; b) slot=a ;; *) echo "no slot active: $active" >&2; exit 1 ;; esac
	c "rm -rf $slots/$slot; mkdir -p $slots/$slot; sync" 20
	"$REPO/frame/linux/send.sh" "$IMG" "$slots/$slot/rootfs.squashfs"
	if [ "${1:-}" = --kernel ]; then
		"$REPO/frame/linux/send.sh" "$REPO/frame/build/linux/zImage" "$slots/$slot/zImage"
		"$REPO/frame/linux/send.sh" "$REPO/frame/build/linux/rk3126-amaboo-gbzk-y1000.dtb" "$slots/$slot/board.dtb"
		"$REPO/frame/linux/send.sh" "$REPO/frame/build/linux-initramfs.cpio.gz" "$slots/$slot/initramfs.cpio.gz"
	fi
	c "printf 'active=$active\npending=$slot\ntries=2\n' >$slots/state.new && sync && mv $slots/state.new $slots/state && sync" 20
	echo "system: slot $slot, tried at the next boot"
	exit 0
fi

a wait-for-device
a shell "mkdir -p $DIR && chmod 700 $DIR"
# SHA-1: the copy's integrity (Android 6 has no sha256sum); the image already there is kept
local_sum=$(shasum -a 1 "$IMG" | cut -c1-40)
if [ "$(a shell "sha1sum $DIR/rootfs.squashfs 2>/dev/null" | cut -c1-40)" != "$local_sum" ]; then
	a push "$IMG" "$DIR/rootfs.squashfs.new" | tail -1
	remote_sum=$(a shell "sha1sum $DIR/rootfs.squashfs.new" | cut -c1-40)
	[ "$local_sum" = "$remote_sum" ] || { echo "the copy on the frame differs: not installed" >&2; exit 1; }
	a shell "chmod 600 $DIR/rootfs.squashfs.new && mv $DIR/rootfs.squashfs.new $DIR/rootfs.squashfs"
fi
echo "system: $DIR/rootfs.squashfs ($local_sum)"

if [ "${1:-}" = --target ]; then
	URL="${2:?usage: frame/linux/install.sh --target <Maison URL>}"
	case "$URL" in https://*) ;; *) echo "an HTTPS address" >&2; exit 1 ;; esac
	# pushed as a file: Android 6's `adb shell` has no stdin of its own (a pty that never ends)
	tmp=$(mktemp)
	trap 'rm -f "$tmp"' EXIT
	printf '%s' "${URL%%#*}" >"$tmp"
	a push "$tmp" "$DIR/target.new" >/dev/null
	a shell "chmod 600 $DIR/target.new && mv $DIR/target.new $DIR/target"
	echo "target: ${URL%%#*} (adopted at the next mainline boot, unless the frame already keeps one)"
fi

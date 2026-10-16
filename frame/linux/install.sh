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
# Usage: frame/linux/install.sh [--target https://home.example]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
IMG="$REPO/frame/build/linux/rootfs.squashfs"
DIR=/data/local/tmp/maison
SERIAL="${ANDROID_SERIAL:-}"
a() { adb ${SERIAL:+-s "$SERIAL"} "$@"; }

[ -f "$IMG" ] || { echo "no $IMG: frame/linux/rootfs.sh first" >&2; exit 1; }
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

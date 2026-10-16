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

# Linux up (its USB console there): the frame takes the image itself, over the Wi-Fi. It listens
# for one connection, the Mac sends, the frame checks the SHA-256 said over the console (the
# channel trusted) and only then renames it over the old one; Android is not needed (it sits in
# the recovery slot since Linux boots by default).
if compgen -G '/dev/cu.usbmodem*' >/dev/null; then
	[ -z "${1:-}" ] || { echo "--target goes through Android (power up with the USB-C on the Mac)" >&2; exit 2; }
	c() { "$REPO/frame/linux/console.sh" "$@"; }
	# where Linux sees Android's userdata
	DIR="/android-data${DIR#/data}"
	local_sum=$(shasum -a 256 "$IMG" | cut -c1-64)
	if [ "$(c "sha256sum $DIR/rootfs.squashfs 2>/dev/null | cut -c1-64" 120)" = "$local_sum" ]; then
		echo "system: $DIR/rootfs.squashfs already this one ($local_sum)"
		exit 0
	fi
	ip=$(c "ip -4 -o addr show wlan0 | sed -E 's/.* inet ([0-9.]+).*/\1/'" 20)
	case "$ip" in *.*.*.*) ;; *) echo "the frame says no Wi-Fi address: $ip" >&2; exit 1 ;; esac
	port=9799
	# one connection at most, five minutes at most: never a listener left behind
	c "rm -f $DIR/rootfs.squashfs.new; (timeout 300 systemd-socket-activate -l $port --accept --inetd sh -c 'cat > $DIR/rootfs.squashfs.new; pkill -f \"systemd-socket-activate -l $port\"' >/dev/null 2>&1 &); sleep 1; echo listening" 20
	echo "sending the system to $ip:$port"
	# --accept: the connection, not the listening socket, is the reader's stdin
	nc -w 10 "$ip" "$port" <"$IMG" || {
		echo "the send to $ip:$port failed (nc: $?): not installed" >&2
		exit 1
	}
	remote_sum=$(c "sync; sha256sum $DIR/rootfs.squashfs.new | cut -c1-64" 120)
	if [ "$remote_sum" != "$local_sum" ]; then
		c "rm -f $DIR/rootfs.squashfs.new" 20
		echo "the copy on the frame differs ($remote_sum): not installed" >&2
		exit 1
	fi
	c "chmod 600 $DIR/rootfs.squashfs.new && mv $DIR/rootfs.squashfs.new $DIR/rootfs.squashfs && sync && echo installed" 30
	echo "system: $DIR/rootfs.squashfs ($local_sum), at the next boot"
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

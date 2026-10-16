#!/usr/bin/env bash
# One mainline try on the frame: from Android or from the mainline console; with --install the
# system (install.sh) put on userdata while on Android; with --flash the image
# (frame/build/recovery-linux.img) written to the recovery partition (rkwrite.py: proved
# addressing, chunks read back); then the frame booted into it and its USB console waited for.
# Every wait has a deadline and fails the script, loudly, when it runs out; each step is timed.
# U-Boot boots Android or the recovery after a reset depending on the USB link: both are handled.
# Usage: frame/linux/try.sh [--install] [--flash]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
IMG="$REPO/frame/build/recovery-linux.img"
export PATH="$HOME/maison-frame/bin:$PATH"
# rkdeveloptool writes a log/ into its working directory: a temporary one, never the repo
RKDIR=$(mktemp -d)
rk() { (cd "$RKDIR" && rkdeveloptool "$@"); }

step() { echo "[$(date +%H:%M:%S)] $*"; }
console() { compgen -G '/dev/cu.usbmodem*' >/dev/null; }
# Android counts once it says its boot is complete: adb answers « device » for a moment before
android() { [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; }
no_console() { ! console; }
no_android() { [ "$(adb get-state 2>/dev/null)" != device ]; }
loader() { rk ld 2>/dev/null | grep -q Loader; }
# wait_for <what> <seconds> <test…>
wait_for() {
	local what=$1 secs=$2 i
	shift 2
	for ((i = 0; i < secs; i += 2)); do "$@" && return 0; sleep 2; done
	echo "timeout: $what, after ${secs}s" >&2
	exit 1
}
to_console() { printf '%s\r' "$1" >"$(compgen -G '/dev/cu.usbmodem*' | head -1)"; }
android_or_console() { android || console; }

if console; then
	step "mainline is up: rebooting to Android"
	to_console "reboot -f"
	wait_for "the mainline console to go" 60 no_console
	wait_for "Android after the reboot" 240 android
elif ! android; then
	echo "the frame is neither on Android nor on the mainline console" >&2
	exit 1
fi
step "on Android"
# bring-up: the USB console runs only while this file exists (rootfs autologin.conf)
adb shell "mkdir -p /data/local/tmp/maison && touch /data/local/tmp/maison/debug-console"
install=false flash=false
for a in "$@"; do
	case "$a" in --install) install=true ;; --flash) flash=true ;; *) echo "unknown: $a" >&2; exit 2 ;; esac
done

if $install; then
	step "installing the system on userdata (about 1 min when it changed)"
	"$REPO/frame/linux/install.sh"
fi

if $flash; then
	[ -f "$IMG" ] || { echo "no $IMG" >&2; exit 1; }
	step "to the loader"
	adb reboot loader
	wait_for "Android to go" 60 no_android
	wait_for "the loader" 60 loader
	step "writing $(basename "$IMG") to the recovery (about 1 min)"
	python3 "$REPO/frame/rkwrite.py" "$IMG" mmcblk0p6
	rk rd
	step "reset: waiting for Android or the recovery (1 to 2 min)"
	wait_for "Android or the recovery after the reset" 240 android_or_console
fi

if ! console; then
	wait_for "Android's boot to complete" 240 android
	step "Android to the recovery"
	adb reboot recovery
	wait_for "Android to go" 60 no_android
	wait_for "the mainline console" 240 console
fi
step "mainline console up"

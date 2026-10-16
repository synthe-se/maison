#!/usr/bin/env bash
# The frame's mainline system, updated: Linux boots by default (the `boot` slot; the vendor's
# Android sits in `recovery`, for the record). With --install the system (rootfs.squashfs) goes to
# userdata from Linux itself over the Wi-Fi (install.sh: its SHA-256 checked over the USB
# console); with --flash the kernel image (frame/build/boot-linux.img: image.py --slot boot) is
# written to `boot` from the loader (rkwrite.py: proved addressing, chunks read back), which Linux
# cannot reach by itself: the reset pinhole, as docs/frame.md says. Then the frame is rebooted and
# its app waited for. Every wait has a deadline and fails the script, loudly; each step is timed.
# Usage: frame/linux/try.sh [--install] [--flash]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
IMG="$REPO/frame/build/boot-linux.img"
export PATH="$HOME/maison-frame/bin:$PATH"
# rkdeveloptool writes a log/ into its working directory: a temporary one, never the repo
RKDIR=$(mktemp -d)
rk() { (cd "$RKDIR" && rkdeveloptool "$@"); }

step() { echo "[$(date +%H:%M:%S)] $*"; }
console() { compgen -G '/dev/cu.usbmodem*' >/dev/null; }
no_console() { ! console; }
loader() { rk ld 2>/dev/null | grep -q Loader; }
# wait_for <what> <seconds> <test…>
wait_for() {
	local what=$1 secs=$2 i
	shift 2
	for ((i = 0; i < secs; i += 2)); do "$@" && return 0; sleep 2; done
	echo "timeout: $what, after ${secs}s" >&2
	exit 1
}
c() { "$REPO/frame/linux/console.sh" "$@"; }
ui_active() { [ "$(c 'systemctl is-active maison-ui' 20 2>/dev/null)" = active ]; }

install=false flash=false
for a in "$@"; do
	case "$a" in --install) install=true ;; --flash) flash=true ;; *) echo "unknown: $a" >&2; exit 2 ;; esac
done

if $install; then
	wait_for "the frame's console (the USB-C on the Mac, Linux up)" 120 console
	step "installing the system on userdata over the Wi-Fi (about 1 min when it changed)"
	"$REPO/frame/linux/install.sh"
fi

if $flash; then
	[ -f "$IMG" ] || { echo "no $IMG: frame/linux/image.py --slot boot first" >&2; exit 1; }
	step "the loader: power off, hold the reset pinhole, power on with the USB-C on the Mac"
	wait_for "the loader" 300 loader
	step "writing $(basename "$IMG") to boot (about 1 min)"
	python3 "$REPO/frame/rkwrite.py" "$IMG" mmcblk0p5
	rk rd
elif console; then
	step "rebooting Linux"
	c 'systemctl reboot' 10 || true
	wait_for "the console to go" 60 no_console
fi

wait_for "Linux's console after the boot" 240 console
step "Linux up: waiting for the native app"
wait_for "maison-ui active" 180 ui_active
step "maison-ui up"

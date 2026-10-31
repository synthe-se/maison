#!/usr/bin/env bash
# The frame's mainline system, updated: Linux boots by default (the `boot` slot; the vendor's
# Android sits in `recovery`, for the record). With --install the system (rootfs.squashfs) goes to
# userdata from Linux itself over the Wi-Fi (install.sh: its SHA-256 checked over the USB
# console); with --flash the kernel image (frame/build/boot-linux.img: image.py) is
# written to `boot`: from Linux itself when it is up (sent over the Wi-Fi, the eMMC's addressing
# proved first by reading U-Boot's head where layout.txt says and finding boot's header, then
# written, read back and compared), else from the loader (rkwrite.py; the reset pinhole, as
# docs/frame.md says). With --ram the new kernel only runs: sent to /tmp and started by kexec, the
# eMMC's slots untouched, so that cutting the power comes back to what is flashed. Then the frame
# is rebooted and its app waited for. Every wait has a deadline and fails the script, loudly; each
# step is timed.
# Usage: frame/linux/try.sh [--install [--kernel]] [--flash | --ram]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
IMG="$REPO/frame/build/boot-linux.img"
export PATH="$HOME/maison-frame/bin:$PATH"
# rkdeveloptool writes a log/ into its working directory: a temporary one, never the repo
RKDIR=$(mktemp -d)
rk() { (cd "$RKDIR" && rkdeveloptool "$@"); }

step() { echo "[$(date +%H:%M:%S)] $*"; }
# Linux up: its USB console, else its SSH (console.sh takes either)
console() { "$REPO/frame/linux/console.sh" true 8 >/dev/null 2>&1; }
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

install=false flash=false ram=false kernel=
for a in "$@"; do
	case "$a" in --install) install=true ;; --kernel) kernel=--kernel ;; --flash) flash=true ;; --ram) ram=true ;; *) echo "unknown: $a" >&2; exit 2 ;; esac
done
if $flash && $ram; then echo "--flash or --ram" >&2; exit 2; fi

if $install; then
	wait_for "the frame (USB console or SSH, Linux up)" 120 console
	step "sending the update to the free slot over the Wi-Fi (about 30 s)"
	"$REPO/frame/linux/install.sh" $kernel
fi

BACKUP="$HOME/maison-frame/backup"
start_of() { awk -v p="$1" -F'[ =]' '$1==p {print $3}' "$BACKUP/layout.txt"; }
size_of() { awk -v p="$1" -F'[ =]' '$1==p {print $5}' "$BACKUP/layout.txt"; }

if $ram; then
	BUILD="$REPO/frame/build"
	for f in linux/zImage linux/rk3126-amaboo-gbzk-y1000.dtb linux-initramfs.cpio.gz; do
		step "sending $f"
		"$REPO/frame/linux/send.sh" "$BUILD/$f" "/tmp/ram-$(basename "$f")"
	done
	# U-Boot's part of the command line: the new kernel adds its own (CONFIG_CMDLINE_EXTEND)
	step "loading the kernel"
	c 'kexec -l /tmp/ram-zImage --dtb=/tmp/ram-rk3126-amaboo-gbzk-y1000.dtb --initrd=/tmp/ram-linux-initramfs.cpio.gz --command-line="$(. /usr/local/lib/maison-slots; uboot_line)"' 60
	c 'systemctl kexec' 10 || true
	wait_for "the console to go" 60 no_console
elif $flash && console; then
	[ -f "$IMG" ] || { echo "no $IMG: frame/linux/image.py first" >&2; exit 1; }
	boot=$(start_of mmcblk0p5) uboot=$(start_of mmcblk0p1)
	sectors=$(($(wc -c <"$IMG") / 512))
	[ "$sectors" -le "$(size_of mmcblk0p5)" ] || { echo "$IMG: $sectors sectors, boot holds $(size_of mmcblk0p5)" >&2; exit 1; }
	# the eMMC as Linux sees it is the layout's: U-Boot's head where it says, boot's header where it says
	head_sum=$(head -c 4096 "$BACKUP/mmcblk0p1.img" | shasum -a 256 | cut -c1-64)
	[ "$(c "dd if=/dev/mmcblk0 bs=512 skip=$uboot count=8 2>/dev/null | sha256sum | cut -c1-64" 90)" = "$head_sum" ] ||
		{ echo "the eMMC's addressing is not the layout's (U-Boot's head differs): nothing written" >&2; exit 1; }
	[ "$(c "dd if=/dev/mmcblk0 bs=512 skip=$boot count=1 2>/dev/null | head -c 8; echo" 90)" = "ANDROID!" ] ||
		{ echo "no boot image header at sector $boot: nothing written" >&2; exit 1; }
	step "addressing proved: sending $(basename "$IMG")"
	"$REPO/frame/linux/send.sh" "$IMG" /tmp/boot-linux.img
	step "writing it to boot (sector $boot, $sectors sectors)"
	c "dd if=/tmp/boot-linux.img of=/dev/mmcblk0 bs=512 seek=$boot conv=notrunc,fsync 2>&1 | tail -1; sync; echo 3 > /proc/sys/vm/drop_caches" 120
	want=$(shasum -a 256 "$IMG" | cut -c1-64)
	got=$(c "dd if=/dev/mmcblk0 bs=512 skip=$boot count=$sectors 2>/dev/null | sha256sum | cut -c1-64" 120)
	[ "$got" = "$want" ] || { echo "boot reads back $got, not $want: write it again (try.sh --flash), or from the loader" >&2; exit 1; }
	step "boot written and read back"
	c 'rm -f /tmp/boot-linux.img; systemctl reboot' 10 || true
	wait_for "the console to go" 60 no_console
elif $flash; then
	[ -f "$IMG" ] || { echo "no $IMG: frame/linux/image.py first" >&2; exit 1; }
	step "the loader: power off, hold the reset pinhole, power on with the USB-C on this machine"
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
if $install; then
	settled() { [ -z "$(c "sed -n 's/^pending=//p' /android-data/maison/slots/state" 20 2>/dev/null)" ]; }
	step "the update proving itself (about 1 min)"
	wait_for "the trial to end" 300 settled
	if [ -n "$(c "cat /android-data/maison/slots/failed 2>/dev/null; rm -f /android-data/maison/slots/failed" 20)" ]; then
		echo "the update failed: the frame went back to the system before it" >&2
		exit 1
	fi
	step "the update is the system: slot $(c "sed -n 's/^active=//p' /android-data/maison/slots/state" 20)"
fi

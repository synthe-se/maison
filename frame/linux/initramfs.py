"""The mainline boot's initramfs: its /init (mksh) draws the boot picture, says in the kernel's log
what came up (the display, the touch, the eMMC, the SDIO card, the Wi-Fi interface), chooses a slot
(an update on trial, a rollback, its kernel by kexec) and starts its system from Android's
userdata. It writes to the eMMC only there: the top directory made root's (Android's
init gives it back at its own boots; a directory another owner holds above the journal, systemd
refuses to fix), and Maison's address, adopted once from where install.sh can leave it (Android's
shell-writable /data/local/tmp) into root's /data/maison (the units' `target` credential). With --debug (bring-up), it makes the USB-C port a root serial console
(CDC ACM: /dev/cu.usbmodem* on macOS), blinks the backlight five times (a sign with no console at
all, ~10 s) and waits 8 s before saying what came up. Tools:
rescue-tools.sh's static mksh and toybox; the touch firmware extracted from the vendor kernel
(~/maison-frame/recon, never in the repo).

    python3 frame/linux/initramfs.py [--debug] <tools dir> <touch firmware> <out.cpio.gz>
"""

import gzip
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import splash  # noqa: E402

FIRMWARE = "lib/firmware/silead/gsl-amaboo-gbzk-y1000.fw"  # the DT's firmware-name
# build.sh's Wi-Fi driver: its modules, and its firmware where the patched driver reads it
AIC8800 = Path(__file__).resolve().parents[1] / "build/linux/aic8800"
AIC8800_FIRMWARE = "lib/firmware/aic8800_fw/SDIO/aic8800DC"
# build.sh's wireless-regdb: cfg80211 reads it as it starts, before any system
REGDB = AIC8800.parent

# --debug only: a sign without any console, the backlight blinking 5 times as soon as init runs
BLINK = """
# 0. a sign without any console: the backlight blinks 5 times as soon as init runs
for _ in 1 2 3 4 5 6 7 8 9 10; do ls /sys/class/backlight/*/brightness >/dev/null 2>&1 && break; sleep 1; done
for b in /sys/class/backlight/*/brightness; do
	for _ in 1 2 3 4 5; do echo 10 >$b; sleep 1; echo 255 >$b; sleep 1; done
done
"""
# --debug: what came up is said once everything had its time; else once the display says its
# state (bounded: a panel that never probes is said as such)
SETTLE_DEBUG = "sleep 8"
# --debug: a root shell on the USB-C port (CDC ACM). Never in a product's boot: whoever holds a cable
# would have root
CONSOLE = """# 2. a serial console on the USB-C port
g=/sys/kernel/config/usb_gadget/maison
mkdir -p $g/strings/0x409 $g/configs/c.1/strings/0x409 $g/functions/acm.usb0
echo 0x1d6b >$g/idVendor
echo 0x0104 >$g/idProduct
echo maison-frame >$g/strings/0x409/serialnumber
echo "Maison frame" >$g/strings/0x409/product
echo console >$g/configs/c.1/strings/0x409/configuration
ln -s $g/functions/acm.usb0 $g/configs/c.1/
udc=$(ls /sys/class/udc | head -1)
[ -n "$udc" ] && echo $udc >$g/UDC && say "USB console on $udc"
(while [ ! -e /tmp/stop-console ]; do [ -c /dev/ttyGS0 ] && mksh -i </dev/ttyGS0 >/dev/ttyGS0 2>&1; sleep 1; done) &
console=$!
"""
HANDOVER = """	touch /tmp/stop-console
	pkill -P $console
	pkill -f "mksh -i"
"""
# No battery: at this point the clock says 1970, so a time-based check must never be believed
E2FSCK_CONF = "[options]\nbroken_system_clock = 1\n"
SETTLE = 'for _ in 1 2 3 4 5 6 7 8 9 10; do ls /sys/class/drm/*/status >/dev/null 2>&1 && break; sleep 1; done'

INIT = """#!/bin/mksh
export PATH=/bin
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev
mount -t configfs none /sys/kernel/config
mkdir -p /tmp
say() {{ echo "maison: $*" >/dev/kmsg; }}
say "init up, $(uname -r)"
{blink}
# 1. what is there (the vendor's U-Boot does not resume a recovery from misc: a power cycle is
#    the boot slot's, proved on the frame)
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -b /dev/mmcblk0 ] && break; sleep 1; done
[ -b /dev/mmcblk0 ] && say "eMMC: $(cat /sys/block/mmcblk0/size) sectors" || say "no eMMC"

{console}
# 3. what came up, in the log
{settle}
# the boot picture (splash.py) as soon as the framebuffer is there: Maison's mark, its ring turning
# until the interface takes the panel, no text
for _ in 1 2 3 4 5; do [ -c /dev/fb0 ] && break; sleep 1; done
show() {{ [ -c /dev/fb0 ] && dd if=/splash/$1.raw of=/dev/fb0 bs={stride} seek={row} conv=notrunc 2>/dev/null; }}
if [ -c /dev/fb0 ]; then
	cat /splash/splash.raw >/dev/fb0 2>/dev/null
	maison-loader {loader} &
fi
# (echo $(…) joins the lines: toybox has no tr)
say "display: $(echo $(cat /sys/class/drm/*/status 2>/dev/null))"
say "backlight: $(echo $(ls /sys/class/backlight 2>/dev/null))"
say "input: $(echo $(cat /sys/class/input/*/name 2>/dev/null))"
say "mmc: $(echo $(ls /sys/bus/mmc/devices 2>/dev/null))"
say "sdio: $(echo $(cat /sys/bus/sdio/devices/*/vendor /sys/bus/sdio/devices/*/device 2>/dev/null))"

# 4. userdata, and which system: two slots, a and b, each a system and perhaps a kernel, all of it
#    checked against its `sums`. An update goes to the free slot and is tried for its tries, the
#    active slot's boots counted the same way, each before it starts (a power cut or a crash spends
#    one), until maison-commit says the frame works. Tries spent, 4 boots without a proof, or a
#    slot not whole: the other one; 8 boots and neither, the failure's line stays. A slot's kernel starts by kexec, here, before anything holds the hardware; no update
#    writes `boot`, so that its kernel is what any failure comes back to
data=$(grep -l "^PARTNAME=userdata$" /sys/class/block/*/uevent | head -1)
{slots}
mkdir -p /android-data /lower /rw /newroot
slot=
trial=
# the factory's first system, left where Android's shell writes: adopted once, while userdata holds
# no state at all, into root's slots
adopt() {{
	first=/android-data/local/tmp/maison/rootfs.squashfs
	[ ! -s $state ] && [ -s $first ] || return 0
	mkdir -p $slots/a && chown 0:0 $slots $slots/a && chmod 700 $slots && mv $first $slots/a/rootfs.squashfs &&
		(cd $slots/a && sha256sum rootfs.squashfs >sums) && sync && remember a "" 0 0
}}
choose() {{
	adopt
	active=$(key active) pending=$(key pending) tries=$(key tries) boots=$(key boots)
	case "$active" in a | b) ;; *)
		# no usable state: what is whole
		whole a && active=a || active=b
		pending=
		;;
	esac
	if [ -n "$pending" ]; then
		if [ "${{tries:-0}}" -gt 0 ] && whole "$pending" && remember "$active" "$pending" $((tries - 1)) "$boots"; then
			slot=$pending
			trial=1
			say "update: trying slot $slot"
			return
		fi
		say "update: slot $pending failed, back on $active"
		echo "$pending" >$slots/failed
		remember "$active" "" 0 "$boots"
	fi
	if [ "${{boots:-0}}" -ge 4 ] && whole "$(other $active)"; then
		say "slot $active failed $boots boots: on to $(other $active)"
		active=$(other $active)
		boots=0
	fi
	if ! whole "$active"; then
		active=$(other $active)
		# neither whole: nothing is mounted, the failure's line stays
		whole "$active" || return 0
	fi
	# what cannot be counted (userdata full) still boots, and a slot failed for ever stops here
	if [ "${{boots:-0}}" -lt 8 ]; then remember "$active" "" 0 $((${{boots:-0}} + 1)); slot=$active; fi
}}
own_kernel() {{
	here=$slots/$slot
	maison-guard arm
	say "kernel of slot $slot"
	pkill maison-loader
	line=$(uboot_line)
	if kexec -l $here/zImage --dtb=$here/board.dtb --initrd=$here/initramfs.cpio.gz --command-line="$line maison.slot=$slot"; then
		umount /android-data
		kexec -e
		reboot -f
	fi
	# never a trial of a kernel that did not run: spent, the next boot goes back
	say "kexec of slot $slot failed"
	[ -z "$trial" ] || remember "$active" "$pending" 0 "$boots"
	reboot -f
}}
# its board support, then the driver itself, without its power save (the frame lives on the mains;
# the driver sets it whatever cfg80211's default)
wifi() {{ insmod /lib/modules/aic8800_bsp.ko && insmod /lib/modules/aic8800_fdrv.ko ps_on=0; }}
loading=
# after a kexec, the first kernel chose
slot=$(sed -n 's/.*maison\\.slot=\\([ab]\\).*/\\1/p' /proc/cmdline)
if [ -n "$data" ]; then
	device=/dev/$(grep "^DEVNAME=" $data | cut -d= -f2)
	# clean or journal replayed, it stops at the superblock; a full check (errors found) repairs
	# without optimising
	[ -n "$slot" ] || e2fsck -p -E fixes_only $device >/dev/null 2>&1
	if mount -t ext4 -o rw,noatime,nosuid,nodev,noexec $device /android-data; then
		if [ -n "$slot" ]; then
			[ "$(key pending)" = "$slot" ] && trial=1
		else
			# no slot holds a kernel: whichever choose takes, no kexec follows, so the driver
			# loads beside the sums (nothing here reads a slot)
			if [ ! -s $slots/a/zImage ] && [ ! -s $slots/b/zImage ]; then
				wifi &
				loading=$!
			fi
			choose
			[ -z "$slot" ] || [ ! -s $slots/$slot/zImage ] || own_kernel
		fi
	fi
fi

# 5. the Wi-Fi driver, unless it already started beside the sums
if [ -n "$loading" ]; then wait $loading; else wifi; fi
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -d /sys/class/net/wlan0 ] && break; sleep 1; done
say "net: $(echo $(ls /sys/class/net))"

# 6. the system: its squashfs, read-only, a tmpfs over it, and userdata at /android-data, read-write
#    for what the system keeps (/android-data/maison); else the failure's line stays
img=$slots/$slot/rootfs.squashfs
if [ -n "$slot" ] && loop=$(losetup -f) && losetup -r $loop $img && mount -t squashfs -o ro $loop /lower &&
	mount -t tmpfs -o mode=755 tmpfs /rw &&
	mkdir -p /rw/upper /rw/work && mount -t overlay overlay -o lowerdir=/lower,upperdir=/rw/upper,workdir=/rw/work /newroot; then
	say "system: $img"
	chown 0:0 /android-data
	# Maison's address: root's, read by the units (LoadCredential); install.sh's copy, where
	# Android's shell (any ADB key) writes, is adopted only while root keeps none, then removed
	kept=/android-data/maison/target
	left=/android-data/local/tmp/maison/target
	mkdir -p /android-data/maison && chown 0:0 /android-data/maison && chmod 755 /android-data/maison
	if [ -s $left ]; then
		if [ -s $kept ]; then
			say "address: $left ignored, $kept kept (removed from the console to adopt another)"
		elif cp $left $kept.new && chown 0:0 $kept.new && chmod 600 $kept.new && sync && mv $kept.new $kept && sync; then
			say "address: adopted from $left"
		fi
		rm -f $left
	fi
	[ -s $kept ] || say "address: none yet: the frame's setup asks for it"
	# one identity across boots: the journal stays one directory, its limit holds; what earlier
	# systems left under random ids goes
	id=$(sha256sum /sys/block/mmcblk0/device/cid | cut -c1-32)
	echo "$id" >/newroot/etc/machine-id
	for old in /android-data/maison/journal/*; do [ "$old" = /android-data/maison/journal/$id ] || rm -rf "$old"; done
	mkdir -p /newroot/android-data
	mount --move /android-data /newroot/android-data
	[ -z "$trial" ] || maison-guard 240 &
{handover}	for m in dev proc sys; do mount --move /$m /newroot/$m; done
	exec switch_root /newroot /sbin/init
fi
say "no system: slot $slot"
# what cannot even start spends its count here, so that the next boot chooses again
[ -z "$slot" ] || reboot -f
pkill maison-loader
show missing
while :; do sleep 3600; done
"""


SLOTS = (Path(__file__).resolve().parent / "rootfs/usr/local/lib/maison-slots").read_text()


def init(debug: bool) -> str:
    """The /init script; `debug` adds the blinks and the fixed wait of the bring-up."""
    ground, ink = splash.colours()
    return INIT.format(
        blink=BLINK if debug else "",
        console=CONSOLE if debug else "",
        handover=HANDOVER if debug else "",
        settle=SETTLE_DEBUG if debug else SETTLE,
        slots=SLOTS,
        stride=splash.STRIDE,
        row=splash.STRIP_ROW,
        loader=f"{splash.LOADER_X} {splash.LOADER_Y} {ground:#x} {ink:#x} 120",
    )


def build(tools: Path, firmware: Path, debug: bool = False) -> bytes:
    entries, ino = [], [300000]

    def add(name, data=b"", mode=0o100644):
        ino[0] += 1
        entries.append({"name": name, "data": data, "ino": ino[0], "mode": mode, "uid": 0, "gid": 0, "nlink": 1, "mtime": 0})

    for d in ("bin", "dev", "etc", "proc", "sys", "lib", "lib/modules", "lib/firmware", "lib/firmware/silead", "splash"):
        add(d, mode=0o40755)
    parts = AIC8800_FIRMWARE.split("/")
    for i in range(2, len(parts)):
        add("/".join(parts[: i + 1]), mode=0o40755)
    add("init", init(debug).encode(), 0o100755)
    add("etc/e2fsck.conf", E2FSCK_CONF.encode())
    add("bin/mksh", (tools / "mksh").read_bytes(), 0o100755)
    add("bin/toybox", (tools / "toybox").read_bytes(), 0o100755)
    for tool in ("maison-loader", "maison-guard", "kexec", "e2fsck"):
        add(f"bin/{tool}", (tools / tool).read_bytes(), 0o100755)
    for applet in (tools / "applets.txt").read_text().split():
        if applet not in ("mksh", "toybox", "sh"):
            add(f"bin/{applet}", b"toybox", 0o120777)
    add("bin/sh", b"mksh", 0o120777)
    add(FIRMWARE, firmware.read_bytes())
    for name in ("regulatory.db", "regulatory.db.p7s"):
        add(f"lib/firmware/{name}", (REGDB / name).read_bytes())
    for name, data in splash.frames().items():
        add(f"splash/{name}.raw", data)
    for ko in ("aic8800_bsp.ko", "aic8800_fdrv.ko"):
        add(f"lib/modules/{ko}", (AIC8800 / ko).read_bytes())
    for f in sorted((AIC8800 / "firmware").iterdir()):
        add(f"{AIC8800_FIRMWARE}/{f.name}", f.read_bytes())
    return gzip.compress(bootimg.write_cpio(entries), compresslevel=9, mtime=0)


def main() -> int:
    args = sys.argv[1:]
    debug = args[:1] == ["--debug"]
    args = args[1:] if debug else args
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(args[2])
    out.write_bytes(build(Path(args[0]), Path(args[1]), debug))
    print(f"{out}: {out.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

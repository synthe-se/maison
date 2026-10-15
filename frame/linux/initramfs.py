"""The first mainline boot's initramfs: no distribution yet, only what tells us how far the kernel
got. Its /init (mksh) writes nothing to the eMMC (only reads, and reports); makes the USB-C port
a serial console (CDC ACM: /dev/cu.usbmodem* on the Mac); blinks the backlight five times first
(a sign with no console at all); and prints on the panel (the kernel's console, fbcon) what came up: the display,
the touch, the eMMC, the SDIO card, the Wi-Fi interface. Tools: rescue-tools.sh's static mksh and toybox; the touch
firmware extracted from the vendor kernel (~/maison-frame/recon, never in the repo).

    python3 frame/linux/initramfs.py <tools dir> <touch firmware> <out.cpio.gz>
"""

import gzip
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

FIRMWARE = "lib/firmware/silead/gsl-amaboo-gbzk-y1000.fw"  # the DT's firmware-name
# build.sh's Wi-Fi driver: its modules, and its firmware where the patched driver reads it
AIC8800 = Path(__file__).resolve().parents[1] / "build/linux/aic8800"
AIC8800_FIRMWARE = "lib/firmware/aic8800_fw/SDIO/aic8800DC"

INIT = f"""#!/bin/mksh
export PATH=/bin
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev
mount -t configfs none /sys/kernel/config
mkdir -p /tmp
say() {{ echo "maison: $*" >/dev/kmsg; }}
say "init up, $(uname -r)"

# 0. a sign without any console: the backlight blinks 5 times as soon as init runs
for _ in 1 2 3 4 5 6 7 8 9 10; do ls /sys/class/backlight/*/brightness >/dev/null 2>&1 && break; sleep 1; done
for b in /sys/class/backlight/*/brightness; do
	for _ in 1 2 3 4 5; do echo 10 >$b; sleep 1; echo 255 >$b; sleep 1; done
done

# 1. nothing is written to the eMMC: only read, to say what is there (the vendor's U-Boot
#    does not resume a recovery from misc: a power cycle is Android's boot, proved on the frame)
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -b /dev/mmcblk0 ] && break; sleep 1; done
[ -b /dev/mmcblk0 ] && say "eMMC: $(cat /sys/block/mmcblk0/size) sectors" || say "no eMMC"

# 2. a serial console on the USB-C port
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
# (until /tmp/stop-console says the system's own getty takes it over)
(while [ ! -e /tmp/stop-console ]; do [ -c /dev/ttyGS0 ] && mksh -i </dev/ttyGS0 >/dev/ttyGS0 2>&1; sleep 1; done) &
console=$!

# 3. what came up, on the panel
sleep 8
# (echo $(…) joins the lines: toybox has no tr)
say "display: $(echo $(cat /sys/class/drm/*/status 2>/dev/null))"
say "backlight: $(echo $(ls /sys/class/backlight 2>/dev/null))"
say "input: $(echo $(cat /sys/class/input/*/name 2>/dev/null))"
say "mmc: $(echo $(ls /sys/bus/mmc/devices 2>/dev/null))"
say "sdio: $(echo $(cat /sys/bus/sdio/devices/*/vendor /sys/bus/sdio/devices/*/device 2>/dev/null))"

# 4. the Wi-Fi driver: its board support, then the driver itself
insmod /lib/modules/aic8800_bsp.ko && insmod /lib/modules/aic8800_fdrv.ko
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -d /sys/class/net/wlan0 ] && break; sleep 1; done
say "net: $(echo $(ls /sys/class/net))"

# 5. the system, when Android's userdata holds one: its squashfs, read-only, a tmpfs over it, and
#    userdata at /android-data, read-write for the kiosk's own state (/android-data/maison: its
#    cookies, its cache); else this shell stays
data=$(grep -l "^PARTNAME=userdata$" /sys/class/block/*/uevent | head -1)
img=/android-data/local/tmp/maison/rootfs.squashfs
mkdir -p /android-data /lower /rw /newroot
if [ -n "$data" ] && mount -t ext4 -o rw,noatime /dev/$(grep "^DEVNAME=" $data | cut -d= -f2) /android-data && [ -f $img ] &&
	mount -t squashfs -o ro,loop $img /lower && mount -t tmpfs -o mode=755 tmpfs /rw &&
	mkdir -p /rw/upper /rw/work && mount -t overlay overlay -o lowerdir=/lower,upperdir=/rw/upper,workdir=/rw/work /newroot; then
	say "system: $img"
	mkdir -p /newroot/android-data
	mount --move /android-data /newroot/android-data
	# the system's own getty takes the USB console: the loop is told first, so the shell it runs,
	# ended now, is not started again
	# (never waited for: whatever happens to it, the system starts)
	touch /tmp/stop-console
	pkill -P $console
	pkill -f "mksh -i"
	for m in dev proc sys; do mount --move /$m /newroot/$m; done
	exec switch_root /newroot /sbin/init
fi
say "no system on userdata: the bring-up shell stays"
while :; do sleep 3600; done
"""


def build(tools: Path, firmware: Path) -> bytes:
    entries, ino = [], [300000]

    def add(name, data=b"", mode=0o100644):
        ino[0] += 1
        entries.append({"name": name, "data": data, "ino": ino[0], "mode": mode, "uid": 0, "gid": 0, "nlink": 1, "mtime": 0})

    for d in ("bin", "dev", "proc", "sys", "lib", "lib/modules", "lib/firmware", "lib/firmware/silead"):
        add(d, mode=0o40755)
    parts = AIC8800_FIRMWARE.split("/")
    for i in range(2, len(parts)):
        add("/".join(parts[: i + 1]), mode=0o40755)
    add("init", INIT.encode(), 0o100755)
    add("bin/mksh", (tools / "mksh").read_bytes(), 0o100755)
    add("bin/toybox", (tools / "toybox").read_bytes(), 0o100755)
    for applet in (tools / "applets.txt").read_text().split():
        if applet not in ("mksh", "toybox", "sh"):
            add(f"bin/{applet}", b"toybox", 0o120777)
    add("bin/sh", b"mksh", 0o120777)
    add(FIRMWARE, firmware.read_bytes())
    for ko in ("aic8800_bsp.ko", "aic8800_fdrv.ko"):
        add(f"lib/modules/{ko}", (AIC8800 / ko).read_bytes())
    for f in sorted((AIC8800 / "firmware").iterdir()):
        add(f"{AIC8800_FIRMWARE}/{f.name}", f.read_bytes())
    return gzip.compress(bootimg.write_cpio(entries), compresslevel=9, mtime=0)


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(sys.argv[3])
    out.write_bytes(build(Path(sys.argv[1]), Path(sys.argv[2])))
    print(f"{out}: {out.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""A mainline kernel to try in the recovery slot: the vendor's recovery image as U-Boot expects it
(its header's load addresses), with our zImage, our initramfs, and our device tree as the
`rk-kernel.dtb` of the resource it carries. U-Boot takes the device tree from the booted image's
own resource (proved on the frame), so Android's boot never sees ours: `adb reboot recovery` runs
the test, a power cycle (both cables: no battery) brings Android back.

    python3 frame/linux/image.py <zImage> <dtb> <initramfs.cpio.gz> <out.img>
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

BACKUP = Path.home() / "maison-frame/backup"
RECOVERY_SECTORS = 65536  # the recovery partition (layout.txt)
# the vendor's command line (its parameter, passed by U-Boot) says initrd=0x62000000,0x00800000:
# the kernel reads that many bytes there, so the initramfs is padded with zeros to exactly that
# (the kernel skips zeros between archives, not whatever memory held past ours)
INITRD_SIZE = 0x800000


def main() -> int:
    if len(sys.argv) != 5:
        print(__doc__, file=sys.stderr)
        return 2
    zimage, dtb, initramfs, out = (Path(a) for a in sys.argv[1:])
    image = bootimg.BootImage((BACKUP / "mmcblk0p6.img").read_bytes())
    image.kernel = zimage.read_bytes()
    ramdisk = initramfs.read_bytes()
    if len(ramdisk) > INITRD_SIZE:
        raise SystemExit(f"{initramfs}: {len(ramdisk)} bytes, the command line gives the kernel {INITRD_SIZE}")
    image.ramdisk = ramdisk + b"\0" * (INITRD_SIZE - len(ramdisk))
    image.second = bootimg.rsce_put(image.second, "rk-kernel.dtb", dtb.read_bytes())
    packed = image.pack()
    if len(packed) > RECOVERY_SECTORS * 512:
        raise SystemExit(f"{out}: {len(packed)} bytes, the recovery partition holds {RECOVERY_SECTORS * 512}")
    out.write_bytes(packed)
    print(f"{out}: {len(packed)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

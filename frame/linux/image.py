"""The kernel in the `boot` slot, what a power-up starts: the vendor's image as U-Boot expects it
(its header's load addresses), its resource kept with the vendor's device tree, which U-Boot needs
to draw, and our logo in it; our device tree after the zImage (ARM_APPENDED_DTB), our initramfs.

    python3 frame/linux/image.py <zImage> <dtb> <initramfs.cpio.gz> <out.img>
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import splash  # noqa: E402

BACKUP = Path.home() / "maison-frame/backup"
PARTITION, SECTORS = "mmcblk0p5", 28672  # layout.txt
# the device tree's `chosen` says the kernel reads this many bytes at 0x62000000, so the initramfs is
# padded with zeros to exactly that (the kernel skips zeros, not whatever memory held past ours)
INITRD_SIZE = 0x300000


def main() -> int:
    args = sys.argv[1:]
    if len(args) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    zimage, dtb, initramfs, out = (Path(a) for a in args)
    image = bootimg.BootImage((BACKUP / f"{PARTITION}.img").read_bytes())
    image.kernel = zimage.read_bytes() + dtb.read_bytes()
    ramdisk = initramfs.read_bytes()
    if len(ramdisk) > INITRD_SIZE:
        raise SystemExit(f"{initramfs}: {len(ramdisk)} bytes, the device tree gives the kernel {INITRD_SIZE}")
    image.ramdisk = ramdisk + b"\0" * (INITRD_SIZE - len(ramdisk))
    image.second = bootimg.rsce_put(image.second, "logo.bmp", splash.logo())
    packed = image.pack()
    if len(packed) > SECTORS * 512:
        raise SystemExit(f"{out}: {len(packed)} bytes, boot holds {SECTORS * 512}")
    out.write_bytes(packed)
    print(f"{out}: {len(packed)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

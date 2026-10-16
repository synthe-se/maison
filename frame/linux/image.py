"""The mainline kernel as a slot's image: the vendor's image of that slot as U-Boot expects it
(its header's load addresses), with our zImage, our initramfs, and our device tree as the
`rk-kernel.dtb` of the resource it carries. U-Boot takes the device tree from the booted image's
own resource (proved on the frame). `recovery` (the default) is the try slot: `adb reboot
recovery` runs it. `boot` is what a power-up starts: its 14 MiB hold ours only without the
vendor's logo in the resource (U-Boot draws its logo from the resource partition).

    python3 frame/linux/image.py [--slot boot|recovery] <zImage> <dtb> <initramfs.cpio.gz> <out.img>
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

BACKUP = Path.home() / "maison-frame/backup"
# each slot: its partition, its size in sectors (layout.txt)
SLOTS = {"recovery": ("mmcblk0p6", 65536), "boot": ("mmcblk0p5", 28672)}
# the vendor's command line (its parameter, passed by U-Boot) says initrd=0x62000000,0x00800000:
# the kernel reads that many bytes there, so the initramfs is padded with zeros to exactly that
# (the kernel skips zeros between archives, not whatever memory held past ours)
INITRD_SIZE = 0x800000


def main() -> int:
    args = sys.argv[1:]
    slot = "recovery"
    if args[:1] == ["--slot"] and len(args) > 1:
        slot, args = args[1], args[2:]
    if slot not in SLOTS or len(args) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    partition, sectors = SLOTS[slot]
    zimage, dtb, initramfs, out = (Path(a) for a in args)
    image = bootimg.BootImage((BACKUP / f"{partition}.img").read_bytes())
    image.kernel = zimage.read_bytes()
    ramdisk = initramfs.read_bytes()
    if len(ramdisk) > INITRD_SIZE:
        raise SystemExit(f"{initramfs}: {len(ramdisk)} bytes, the command line gives the kernel {INITRD_SIZE}")
    image.ramdisk = ramdisk + b"\0" * (INITRD_SIZE - len(ramdisk))
    image.second = bootimg.rsce_put(image.second, "rk-kernel.dtb", dtb.read_bytes())
    if slot == "boot":
        image.second = bootimg.rsce_only(image.second, "rk-kernel.dtb")
    packed = image.pack()
    if len(packed) > sectors * 512:
        raise SystemExit(f"{out}: {len(packed)} bytes, the {slot} partition holds {sectors * 512}")
    out.write_bytes(packed)
    print(f"{out}: {len(packed)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

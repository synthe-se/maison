"""The kernel in the `boot` slot, what a power-up starts: the vendor's image as U-Boot expects it
(its header's load addresses), its resource kept with the vendor's device tree, which U-Boot needs
to draw its logo; our device tree after a zImage that holds its initramfs (build.sh --initramfs).
The resource carries logo.bmp too, but U-Boot draws the `resource` partition's (`try.sh --logo`).

    python3 frame/linux/image.py <zImage.boot> <dtb> <out.img>
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "system"))
import bootimg  # noqa: E402

BACKUP = Path.home() / "maison-frame/backup"
PARTITION, SECTORS = "mmcblk0p5", 28672  # layout.txt


def main() -> int:
    args = sys.argv[1:]
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    zimage, dtb, out = (Path(a) for a in args)
    image = bootimg.BootImage((BACKUP / f"{PARTITION}.img").read_bytes())
    image.kernel = zimage.read_bytes() + dtb.read_bytes()
    # the initramfs is in the zImage: a page of zeros keeps the header what U-Boot expects
    image.ramdisk = b"\0" * 4096
    packed = image.pack()
    if len(packed) > SECTORS * 512:
        raise SystemExit(f"{out}: {len(packed)} bytes, boot holds {SECTORS * 512}")
    out.write_bytes(packed)
    print(f"{out}: {len(packed)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

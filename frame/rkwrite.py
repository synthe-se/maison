"""Writes an image to one partition of the frame's eMMC in Rockchip's loader mode (rkdeveloptool),
in 16 MiB chunks each read back, and resumes where it stopped when the loader drops (it did, on
this frame, halfway through a write): it waits for the loader to come back, proves the addressing
again, and goes on. Nothing is written until the addressing is proved against the backup: this
frame's loader counts from its `parameter` (kernel sector − 8192), and uboot's first sector says so.

    python3 frame/rkwrite.py <image> <partition>   e.g. frame/build/system.img mmcblk0p11

BACKUP (~/maison-frame/backup: layout.txt, mmcblk0p1.img), RKDEVELOPTOOL (rkdeveloptool).
"""

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

RK = os.environ.get("RKDEVELOPTOOL", "rkdeveloptool")
BACKUP = Path(os.environ.get("BACKUP", Path.home() / "maison-frame/backup"))
CHUNK = 32768  # sectors: 16 MiB
LOADER_WAIT = 1800  # seconds


def rk(*args) -> subprocess.CompletedProcess:
    return subprocess.run([RK, *map(str, args)], capture_output=True, text=True, timeout=600)


def kernel_start(partition: str) -> int:
    """Where the kernel sees `partition` (layout.txt, from /sys/block/mmcblk0)."""
    for line in (BACKUP / "layout.txt").read_text().split("\n"):
        name, _, rest = line.partition(" start=")
        if name == partition:
            return int(rest.split()[0])
    raise SystemExit(f"{partition}: not in {BACKUP / 'layout.txt'}")


def wait_for_loader() -> None:
    since = time.time()
    while "Loader" not in rk("ld").stdout:
        if time.time() - since > LOADER_WAIT:
            raise SystemExit("no frame in loader mode")
        time.sleep(1)


def offset() -> int | None:
    """The loader's sector 0 in kernel sectors: uboot's first sector found at one of the two."""
    head = (BACKUP / "mmcblk0p1.img").read_bytes()[:512]
    uboot = kernel_start("mmcblk0p1")
    for o in (8192, 0):
        Path("probe").unlink(missing_ok=True)
        rk("rl", uboot - o, 1, "probe")
        if Path("probe").exists() and Path("probe").read_bytes()[:512] == head:
            return o
    return None


def write(image: Path, partition: str) -> None:
    total = image.stat().st_size // 512
    size = int(next(l.split("size=")[1] for l in (BACKUP / "layout.txt").read_text().split("\n") if l.startswith(partition + " ")))
    if image.stat().st_size % 512 or total > size:
        raise SystemExit(f"{image}: {total} sectors, the partition has {size}")
    progress = image.with_suffix(image.suffix + f".{partition}.progress")
    done = int(progress.read_text()) if progress.exists() else 0
    with image.open("rb") as f:
        while done < total:
            wait_for_loader()
            o = offset()
            if o is None:
                print("addressing not proved (the loader just left?): waiting", flush=True)
                time.sleep(2)
                continue
            n = min(CHUNK, total - done)
            f.seek(done * 512)
            data = f.read(n * 512)
            Path("chunk").write_bytes(data)
            Path("back").unlink(missing_ok=True)
            at = kernel_start(partition) - o + done
            rk("wl", at, "chunk")
            rk("rl", at, n, "back")
            if not Path("back").exists() or Path("back").read_bytes() != data:
                print(f"{partition}: sectors {done}+{n} not confirmed, again when the loader is back", flush=True)
                time.sleep(2)
                continue
            done += n
            progress.write_text(str(done))
            print(f"{partition}: {done}/{total} sectors ({100 * done // total} %)", flush=True)
    progress.unlink()
    print(f"{partition}: {image.name} written and read back", flush=True)


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    image = Path(sys.argv[1]).resolve()
    # rkdeveloptool keeps a log/ in the current directory: a temporary one, never the repo
    os.chdir(tempfile.mkdtemp())
    write(image, sys.argv[2])
    return 0


if __name__ == "__main__":
    sys.exit(main())

"""The frame's own Android: the vendor's system image (the backup, never the repo), with Frameo,
Adups FOTA, SuperSU and the vendor's extras gone, Maison's kiosk APK the only home, the Maison boot
animation, French, Paris time and ADB over Wi-Fi. The image is changed in place with debugfs (no
mount, no Linux): everything else stays byte for byte the vendor's.

Its resource image too, with one change: U-Boot's logo (`logo.bmp`, shown before Android) becomes
the boot animation's still frame, in place, same size; the device tree beside it is untouched.

The vendor's boot image stays as it is by default: its U-Boot checks an id we cannot reproduce,
and everything Maison needs can be said from /system (the services whose binaries are gone simply
fail to start). `--boot` also builds a boot image of ours (bootimg.py: `adb root`, the ADB keys in
the ramdisk), for the day that check is understood.

    python3 frame/system/build.py --apk <MaisonFrame.apk> [--boot --adb-keys ~/.android/adbkey.pub]

Needs e2fsprogs (brew install e2fsprogs) and, for the boot animation, frame/bootanimation.sh's tools.
"""

import argparse
import os
import struct
import shutil
import subprocess
import sys
import tempfile
import zipfile
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import bootimg  # noqa: E402

E2FS = Path(os.environ.get("E2FSPROGS", "/opt/homebrew/opt/e2fsprogs/sbin"))
REPO = Path(__file__).resolve().parents[2]
SYSTEM_FILE = "u:object_r:system_file:s0"

# What leaves /system: the photo frame, its updater, root for apps, and what a wall screen never uses.
REMOVED = [
    "/priv-app/newFrameo",  # Frameo
    "/vendor/frameo",  # its device id and licence
    "/app/FotaUpdate",  # Adups FOTA (com.adups.fota)
    "/xbin/su",
    "/xbin/daemonsu",  # SuperSU (init's daemonsu service then has nothing to start)
    "/bin/whtest.sh",
    "/bin/preinstall_cleanup.sh",  # vendor scripts that remount /system writable
    "/bin/install-recovery.sh",
    "/etc/recovery-resource.dat",
    "/recovery-from-boot.p",  # flash_recovery: it would « repair » recovery from the boot image
    "/app/RkApkinstaller",
    "/app/DeviceTest",
    "/app/RkVideoPlayer",
    "/app/PicoTts",
    "/app/OpenWnn",
    "/app/LiveWallpapersPicker",
    "/app/NoiseField",
    "/app/PhaseBeam",
]

# build.prop: Frameo's defaults out; the house's language and time zone in, and ADB over Wi-Fi
# (adbd starts after /system's properties are loaded; its keys are still required).
PROPS_DROPPED = ("ro.frameo.", "ro.zeasn.", "ro.rk.homepage_base")
PROPS = {
    "ro.product.locale": "fr-FR",
    "persist.sys.locale": "fr-FR",
    "persist.sys.timezone": "Europe/Paris",
    "service.adb.tcp.port": "5555",
}


def debugfs(image: Path, *commands: str, write: bool = False) -> str:
    with tempfile.NamedTemporaryFile("w", suffix=".cmds", delete=False) as f:
        f.write("\n".join(commands) + "\n")
    try:
        r = subprocess.run([E2FS / "debugfs", *(["-w"] if write else []), "-f", f.name, str(image)], capture_output=True, text=True)
    finally:
        os.unlink(f.name)
    # debugfs reports a missing path on stderr and goes on: every line is checked by the caller
    return r.stdout + r.stderr


def entries(image: Path, path: str) -> list[tuple[str, bool]]:
    """The names in a directory, each with whether it is one (`ls -p`: /inode/mode/uid/gid/name/size/)."""
    out = []
    for line in debugfs(image, f"ls -p {path}").splitlines():
        parts = line.strip().split("/")
        if len(parts) >= 7 and parts[5] not in (".", ".."):
            out.append((parts[5], parts[2].startswith("04")))
    return out


def exists(image: Path, path: str) -> bool:
    return "File not found" not in debugfs(image, f"stat {path}")


def remove_tree(image: Path, path: str) -> list[str]:
    """debugfs has no `rm -r`: the commands that remove `path` and everything under it."""
    stat = debugfs(image, f"stat {path}")
    if "File not found" in stat:
        raise SystemExit(f"{path}: not in the image (a different vendor build?)")
    if "Type: directory" not in stat:
        return [f"rm {path}"]
    cmds = []
    for name, is_dir in entries(image, path):
        child = f"{path}/{name}"
        cmds += remove_tree(image, child) if is_dir else [f"rm {child}"]
    return cmds + [f"rmdir {path}"]


def label(path: str, context: Path) -> list[str]:
    """Root's, 0644 or 0755, the system_file label (SELinux is permissive here, but labelled right)."""
    return [f"sif {path} uid 0", f"sif {path} gid 0", f"ea_set -f {context} {path} security.selinux"]


def put(image: Path, local: Path, path: str, context: Path) -> list[str]:
    return [f"rm {path}"] * exists(image, path) + [f"write {local} {path}", f"sif {path} mode 0100644", *label(path, context)]


def build_prop(text: str) -> str:
    # their commented-out leftovers too (#ro.frameo.…)
    lines = [l for l in text.splitlines() if not l.strip().lstrip("#").startswith(PROPS_DROPPED)]
    return bootimg.set_props("\n".join(lines), PROPS)


def fsck(image: Path, fix: bool) -> None:
    r = subprocess.run([E2FS / "e2fsck", "-fy" if fix else "-fn", str(image)], capture_output=True, text=True)
    # 0: clean; 1: fixed (the journal the live system left, replayed)
    if r.returncode not in ((0, 1) if fix else (0,)):
        raise SystemExit(f"e2fsck {image}: {r.returncode}\n{r.stdout}{r.stderr}")


def build_system(backup: Path, apk: Path, out: Path, work: Path) -> Path:
    image = out / "system.img"
    shutil.copyfile(backup / "mmcblk0p11.img", image)
    fsck(image, fix=True)

    context = work / "context"
    context.write_bytes(SYSTEM_FILE.encode() + b"\0")
    anim = work / "bootanimation.zip"
    subprocess.run([REPO / "frame/bootanimation.sh", anim], check=True)
    prop = work / "build.prop"
    debugfs(image, f"dump /build.prop {prop}")
    prop.write_text(build_prop(prop.read_text()))

    cmds = [c for path in REMOVED for c in remove_tree(image, path)]
    cmds += ["mkdir /priv-app/MaisonFrame", "sif /priv-app/MaisonFrame mode 040755", *label("/priv-app/MaisonFrame", context)]
    cmds += [f"write {apk} /priv-app/MaisonFrame/MaisonFrame.apk", "sif /priv-app/MaisonFrame/MaisonFrame.apk mode 0100644"]
    cmds += label("/priv-app/MaisonFrame/MaisonFrame.apk", context)
    # Android 6 reads a system app's ABI from its lib/<isa> directory and loads its libraries
    # from there (never from the APK): extracted, as the build of a system image lays them out
    libs = work / "lib"
    with zipfile.ZipFile(apk) as z:
        names = [n for n in z.namelist() if n.startswith("lib/armeabi-v7a/") and n.endswith(".so")]
        if not names:
            raise SystemExit(f"{apk}: no armeabi-v7a libraries")
        for n in names:
            (libs / Path(n).name).parent.mkdir(parents=True, exist_ok=True)
            (libs / Path(n).name).write_bytes(z.read(n))
    for d in ("/priv-app/MaisonFrame/lib", "/priv-app/MaisonFrame/lib/arm"):
        cmds += [f"mkdir {d}", f"sif {d} mode 040755", *label(d, context)]
    for n in names:
        target = f"/priv-app/MaisonFrame/lib/arm/{Path(n).name}"
        cmds += [f"write {libs / Path(n).name} {target}", f"sif {target} mode 0100644", *label(target, context)]
    cmds += put(image, anim, "/media/bootanimation.zip", context)
    cmds += put(image, prop, "/build.prop", context)
    log = debugfs(image, *cmds, write=True)
    # debugfs echoes each command (« debugfs: … ») and the inodes it allocates, nothing else
    # unless one failed
    refused = [l for l in log.splitlines() if l.strip() and not l.startswith(("debugfs", "Allocated inode"))]
    if refused:
        raise SystemExit("debugfs refused:\n" + "\n".join(refused))
    fsck(image, fix=False)
    for path in REMOVED:
        if exists(image, path):
            raise SystemExit(f"{path}: still there")
    return image


def png_rows(png: bytes) -> tuple[int, int, list[bytes]]:
    """An 8-bit RGB or RGBA PNG's rows as RGB bytes (the five PNG filters undone)."""
    w, h, depth, colour = struct.unpack(">IIBB", png[16:26])
    if depth != 8 or colour not in (2, 6):
        raise SystemExit("logo: an 8-bit RGB(A) PNG expected")
    bpp = 3 if colour == 2 else 4
    data, i = b"", 8
    while i < len(png):
        n, kind = struct.unpack(">I4s", png[i : i + 8])
        if kind == b"IDAT":
            data += png[i + 8 : i + 8 + n]
        i += 12 + n
    raw, stride, prev, rows = zlib.decompress(data), w * bpp, bytearray(w * bpp), []
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for x in range(stride):
            a, b = line[x - bpp] if x >= bpp else 0, prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 0xFF
            elif f == 2:
                line[x] = (line[x] + b) & 0xFF
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 0xFF
            elif f == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 0xFF
        prev = line
        rows.append(bytes(line) if bpp == 3 else bytes(v for k, v in enumerate(line) if k % 4 != 3))
    return w, h, rows


def bmp24(w: int, h: int, rows: list[bytes]) -> bytes:
    """The 24-bit bottom-up BMP U-Boot reads (BGR, rows padded to 4 bytes)."""
    pad = b"\0" * (-(w * 3) % 4)
    pixels = b"".join(bytes(r[i + 2 - 2 * (i % 3)] for i in range(len(r))) + pad for r in reversed(rows))
    header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, w, h, 1, 24, 0, len(pixels), 3780, 3780, 0, 0)
    return header + info + pixels


def build_resource(backup: Path, out: Path, work: Path) -> Path:
    """The vendor's resource image with only logo.bmp replaced, in place and the same size. U-Boot
    draws it on the raw panel (800×1280), before any rotation: the panel is turned 90° from
    Android's landscape (ro.sf.hwrotation), and the frame stands at ROTATION: the still frame,
    upright, turned (1 + ROTATION) × 90°."""
    image = bytearray((backup / "mmcblk0p3.img").read_bytes())
    if image[:4] != b"RSCE":
        raise SystemExit("resource: not a Rockchip resource image")
    at = image.find(b"ENTRlogo.bmp\0")
    block, size = struct.unpack("<II", image[at + 260 : at + 268])
    old = image[block * 512 : block * 512 + size]
    w, h = struct.unpack("<ii", old[18:26])
    frames = work / "frames"
    subprocess.run(["bun", REPO / "frame/bootanimation.ts", frames], check=True, capture_output=True)
    still = work / "still.png"
    shutil.copyfile(frames / "part1/000.png", still)
    turn = (1 + int(os.environ.get("ROTATION", "1"))) * 90 % 360
    if turn:
        subprocess.run(["sips", "-r", str(turn), still], check=True, capture_output=True)
    logo = bmp24(*png_rows(still.read_bytes()))
    if len(logo) != size or (w, h) != struct.unpack("<ii", logo[18:26]):
        raise SystemExit(f"logo: {len(logo)} bytes for {size}, the vendor's is {w}×{h}")
    image[block * 512 : block * 512 + size] = logo
    path = out / "resource.img"
    path.write_bytes(bytes(image))
    return path


def build_boot(backup: Path, adb_keys: Path, out: Path) -> Path:
    boot = bootimg.BootImage((backup / "mmcblk0p5.img").read_bytes())
    bootimg.maison(boot, adb_keys.read_bytes())
    image = out / "boot.img"
    image.write_bytes(boot.pack())
    # the boot partition: 28672 sectors (layout.txt)
    if image.stat().st_size > 28672 * 512:
        raise SystemExit(f"{image}: larger than the boot partition")
    return image


def main() -> int:
    a = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    a.add_argument("--backup", type=Path, default=Path.home() / "maison-frame/backup")
    a.add_argument("--apk", type=Path, required=True)
    a.add_argument("--boot", action="store_true", help="also build a boot image of ours (see above)")
    a.add_argument("--adb-keys", type=Path, default=Path.home() / ".android/adbkey.pub")
    a.add_argument("--out", type=Path, default=REPO / "frame/build")
    args = a.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as work:
        system = build_system(args.backup, args.apk, args.out, Path(work))
        resource = build_resource(args.backup, args.out, Path(work))
    images = [system, resource]
    stale = args.out / "boot.img"
    if args.boot:
        images.append(build_boot(args.backup, args.adb_keys, args.out))
    elif stale.exists():
        stale.unlink()  # flash.sh writes what is there: no boot image of an earlier --boot
    for image in images:
        print(f"{image}: {image.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

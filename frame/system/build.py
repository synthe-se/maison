"""The frame's own Android: the vendor's system image (the backup, never the repo), with Frameo,
Adups FOTA, SuperSU and the vendor's extras gone, Maison's kiosk APK the only home, the Maison boot
animation, French, Paris time and ADB over Wi-Fi. The image is changed in place with debugfs (no
mount, no Linux): everything else stays byte for byte the vendor's.

The vendor's boot image stays as it is by default: its U-Boot checks an id we cannot reproduce,
and everything Maison needs can be said from /system (the services whose binaries are gone simply
fail to start). `--boot` also builds a boot image of ours (bootimg.py: `adb root`, the ADB keys in
the ramdisk), for the day that check is understood.

    python3 frame/system/build.py --apk <MaisonFrame.apk> [--boot --adb-keys ~/.android/adbkey.pub]

Needs e2fsprogs (brew install e2fsprogs) and, for the boot animation, frame/bootanimation.sh's tools.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
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
    images = [system]
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

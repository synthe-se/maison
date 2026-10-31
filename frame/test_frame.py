"""The frame's tooling, without the frame or its backup: synthetic images only.

    python3 -m unittest discover -s frame -p 'test_*.py'
"""

import gzip
import os
import re
import stat
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).parent
sys.path[:0] = [str(HERE / "system"), str(HERE), str(HERE / "linux")]
import bootimg  # noqa: E402
import build  # noqa: E402
import initramfs  # noqa: E402
import rkwrite  # noqa: E402


def entry(name, data=b"", ino=1, mode=0o100644):
    return {"name": name, "data": data, "ino": ino, "mode": mode, "uid": 0, "gid": 0, "nlink": 1, "mtime": 0}


def boot_image(ramdisk: bytes, page=2048) -> bytes:
    kernel, second = b"K" * 3000, b"S" * 100
    header = bootimg.HEADER.pack(
        bootimg.MAGIC, len(kernel), 0x60408000, len(ramdisk), 0x62000000, len(second), 0x60F00000, 0x60000100, page,
        0, 0, b"", b"", b"\0" * 32, b"",
    )
    pad = lambda b: b + b"\0" * (-len(b) % page)
    return pad(header) + pad(kernel) + pad(ramdisk) + pad(second)


class Cpio(unittest.TestCase):
    def test_round_trip_as_mkbootfs_writes_it(self):
        entries = [entry("init.rc", b"on boot\n", 7), entry("sbin", ino=8, mode=0o40755)]
        raw = bootimg.write_cpio(entries)
        self.assertEqual(len(raw) % 256, 0)
        back = bootimg.read_cpio(raw)
        self.assertEqual([(e["name"], e["data"], e["ino"], e["mode"]) for e in back], [("init.rc", b"on boot\n", 7, 0o100644), ("sbin", b"", 8, 0o40755)])
        self.assertEqual(bootimg.write_cpio(back), raw)
        # the trailer takes the next inode, 0644, as mkbootfs does
        self.assertIn(b"070701" + b"00000009" + b"000001a4", raw)

    def test_a_put_keeps_the_entry_or_adds_one_for_root(self):
        rd = bootimg.Ramdisk(gzip.compress(bootimg.write_cpio([entry("default.prop", b"a=1\n", 3)])))
        rd.put("default.prop", b"a=2\n")
        rd.put("adb_keys", b"key\n")
        self.assertEqual([(e["name"], e["ino"]) for e in rd.entries], [("default.prop", 3), ("adb_keys", 4)])
        self.assertEqual(rd.text("default.prop"), "a=2\n")


class BootImage(unittest.TestCase):
    def test_parsed_and_packed_byte_for_byte_with_its_own_id(self):
        image = boot_image(gzip.compress(bootimg.write_cpio([entry("init.rc")]), mtime=0))
        boot = bootimg.BootImage(image)
        self.assertEqual((len(boot.kernel), len(boot.second), boot.page), (3000, 100, 2048))
        self.assertEqual(boot.pack(boot.id), image)

    def test_the_id_is_mkbootimgs_sha1(self):
        boot = bootimg.BootImage(boot_image(b"RD"))
        import hashlib

        sha = hashlib.sha1()
        for part in (b"K" * 3000, b"RD", b"S" * 100):
            sha.update(part + struct.pack("<I", len(part)))
        self.assertEqual(boot.digest()[:20], sha.digest())
        self.assertEqual(bootimg.BootImage(boot.pack()).id[:20], sha.digest())

    def test_refuses_what_is_not_a_boot_image(self):
        with self.assertRaises(ValueError):
            bootimg.BootImage(b"\0" * 4096)


class Rescue(unittest.TestCase):
    def test_adb_behind_the_keys_and_a_root_shell_on_loopback_only(self):
        props = "ro.secure=1\nro.debuggable=0\nro.adb.secure =1\nro.debuggable=1\n"
        rc = "service adbd /sbin/adbd --root_seclabel=u:r:su:s0 --device_banner=recovery\n    disabled\n    write /s $ro.serialno\n    write /m ${ro.product.model}\n"
        ramdisk = bootimg.write_cpio([entry("default.prop", props.encode(), 5), entry("init.rc", rc.encode(), 6), entry("system", ino=7, mode=0o40755)])
        recovery = bootimg.BootImage(boot_image(gzip.compress(ramdisk, mtime=0)))
        bootimg.rescue(recovery, b"KEY me@mac\n", b"MKSH", b"TOYBOX", ["dd", "id", "sh", "su"])
        rd = bootimg.Ramdisk(recovery.ramdisk)
        lines = rd.text("default.prop").splitlines()
        # every occurrence: init keeps the first of a ro. property
        self.assertEqual([l for l in lines if l.startswith("ro.debuggable")], ["ro.debuggable=1", "ro.debuggable=1"])
        self.assertIn("ro.adb.secure=1", lines)
        self.assertEqual(rd.text("adb_keys"), "KEY me@mac\n")
        init = rd.text("init.rc")
        # adbd without the su domain this build's policy lacks (it would die on it)
        self.assertIn("service adbd /sbin/adbd --device_banner=recovery\n", init)
        self.assertIn("write /s ${ro.serialno}\n    write /m ${ro.product.model}\n", init)
        self.assertNotIn("nc -L", init)
        names = {e["name"]: e for e in rd.entries}
        # in /system/bin: /sbin is closed to adbd's `shell` user
        self.assertEqual((names["system/bin/mksh"]["data"], oct(names["system/bin/mksh"]["mode"])), (b"MKSH", "0o100755"))
        self.assertEqual((names["system/bin/dd"]["data"], oct(names["system/bin/dd"]["mode"])), (b"toybox", "0o120777"))
        self.assertEqual(names["system/bin/sh"]["data"], b"mksh")
        # root from init, one command at a time through fifos only the shell group opens
        self.assertIn("service rootsh /system/bin/mksh /system/bin/rootsh\n    class default\n    user root\n", init)
        # 0660 by chmod: mkfifo -m is cut by the umask
        self.assertIn("chmod 0660 $d/in $d/out", names["system/bin/rootsh"]["data"].decode())
        # init gives its services no /system/bin in PATH: the toybox links live there
        self.assertTrue(names["system/bin/rootsh"]["data"].startswith(b"export PATH=/system/bin:/sbin\n"))
        self.assertNotIn("system/bin/su", names)
        self.assertFalse([n for n in names if n.startswith("sbin/")])
        # every applet but a shell: sh is mksh
        self.assertIn("system/bin/id", names)
        self.assertEqual(names["system/bin/sh"]["data"], b"mksh")
        # a directory comes before what it holds
        order = [e["name"] for e in rd.entries]
        self.assertLess(order.index("system/bin"), order.index("system/bin/sh"))


class InitAndProps(unittest.TestCase):
    RC = "on boot\n    start x\n\nservice daemonsu /system/xbin/daemonsu\n   class main\n\nservice adbd /sbin/adbd\n    class core\n"

    def test_a_service_goes_with_its_options_and_nothing_else(self):
        out = bootimg.drop_service(self.RC, "daemonsu")
        self.assertNotIn("daemonsu", out)
        self.assertIn("service adbd /sbin/adbd\n    class core\n", out)
        self.assertIn("on boot\n    start x\n", out)

    def test_props_replaced_in_place_or_appended(self):
        self.assertEqual(bootimg.set_props("ro.a=1\nro.b=2", {"ro.b": "3", "ro.c": "4"}), "ro.a=1\nro.b=3\nro.c=4\n")

    def test_build_prop_loses_frameo_and_gets_the_house(self):
        prop = "ro.product.locale=en-US\n#ro.frameo.x=zh\nro.frameo.default.timezone=America/New_York\nro.rk.homepage_base=https://www.baidu.com/\nro.keep=1"
        out = build.build_prop(prop)
        self.assertNotIn("frameo", out)
        self.assertNotIn("baidu", out)
        for line in ("ro.keep=1", "ro.product.locale=fr-FR", "persist.sys.timezone=Europe/Paris", "service.adb.tcp.port=5555", "ro.adb.secure=1"):
            self.assertIn(line, out.splitlines())


class Logo(unittest.TestCase):
    def png(self, w, h, rgb_rows, filt=0):
        raw = b"".join(bytes([filt]) + r for r in rgb_rows)
        chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
        return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")

    def test_a_png_becomes_the_bottom_up_bgr_bmp_u_boot_reads(self):
        rows = [bytes([255, 0, 0, 0, 255, 0]), bytes([0, 0, 255, 10, 20, 30])]
        w, h, back = build.png_rows(self.png(2, 2, rows))
        self.assertEqual((w, h, back), (2, 2, rows))
        bmp = build.bmp24(w, h, back)
        self.assertEqual(struct.unpack("<ii", bmp[18:26]), (2, 2))
        self.assertEqual(struct.unpack("<H", bmp[28:30])[0], 24)
        # bottom row first, BGR, each row padded to 4 bytes
        self.assertEqual(bmp[54:], bytes([255, 0, 0, 30, 20, 10, 0, 0]) + bytes([0, 0, 255, 0, 255, 0, 0, 0]))

    def test_the_png_filters_are_undone(self):
        # Sub (1) then Up (2): the same pixels as unfiltered
        sub = [bytes([10, 20, 30, 5, 5, 5])]
        self.assertEqual(build.png_rows(self.png(2, 1, sub, 1))[2], [bytes([10, 20, 30, 15, 25, 35])])


class Resource(unittest.TestCase):
    def rsce(self):
        image = bytearray(512 * 12)
        image[:4] = b"RSCE"
        for i, (name, block, data) in enumerate([("rk-kernel.dtb", 3, b"DTB1"), ("logo.bmp", 8, b"LOGO")]):
            at = 512 * (1 + i)
            image[at : at + 4] = b"ENTR"
            image[at + 4 : at + 4 + len(name)] = name.encode()
            image[at + 260 : at + 268] = struct.pack("<II", block, len(data))
            image[512 * block : 512 * block + len(data)] = data
        return bytes(image)

    def test_an_entry_is_replaced_in_its_slot_and_nothing_else_moves(self):
        image = bootimg.rsce_put(self.rsce(), "rk-kernel.dtb", b"D" * 2000)
        self.assertEqual(bootimg.rsce_get(image, "rk-kernel.dtb"), b"D" * 2000)
        self.assertEqual(bootimg.rsce_get(image, "logo.bmp"), b"LOGO")
        self.assertEqual(len(image), 512 * 12)

    def test_one_entry_kept_alone_the_others_and_their_data_gone(self):
        image = bytearray(self.rsce())
        # the vendor's header: its size, the index's first block, an entry's size, the count
        image[8:11] = bytes([1, 1, 1])
        image[12:16] = struct.pack("<I", 2)
        only = bootimg.rsce_only(bytes(image), "rk-kernel.dtb")
        self.assertEqual(bootimg.rsce_get(only, "rk-kernel.dtb"), b"DTB1")
        self.assertEqual(struct.unpack("<I", only[12:16])[0], 1)
        self.assertEqual(only.find(b"logo.bmp"), -1)
        self.assertEqual(len(only), 512 * 4, "cut after the kept entry's data")
        with self.assertRaises(ValueError):
            bootimg.rsce_only(bytes(image), "nope")

    def test_an_entry_larger_than_its_slot_is_refused(self):
        with self.assertRaises(ValueError):
            bootimg.rsce_put(self.rsce(), "rk-kernel.dtb", b"D" * (512 * 5 + 1))


class Rkwrite(unittest.TestCase):
    def test_partitions_come_from_the_kernels_layout(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "layout.txt").write_text("mmcblk0p1 start=16384 size=8192\nmmcblk0p11 start=618496 size=2097152\n")
            old, rkwrite.BACKUP = rkwrite.BACKUP, Path(d)
            try:
                self.assertEqual(rkwrite.kernel_start("mmcblk0p11"), 618496)
                with self.assertRaises(SystemExit):
                    rkwrite.kernel_start("mmcblk0p5")
            finally:
                rkwrite.BACKUP = old

    def test_an_image_larger_than_its_partition_is_refused_before_the_loader(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "layout.txt").write_text("mmcblk0p5 start=90112 size=2\n")
            image = Path(d, "boot.img")
            image.write_bytes(b"\0" * 512 * 3)
            old, rkwrite.BACKUP = rkwrite.BACKUP, Path(d)
            try:
                with self.assertRaises(SystemExit):
                    rkwrite.write(image, "mmcblk0p5")
            finally:
                rkwrite.BACKUP = old


class Initramfs(unittest.TestCase):
    def test_a_boot_waits_on_what_it_says_and_blinks_only_in_bring_up(self):
        boot, debug = initramfs.init(False), initramfs.init(True)
        # every boot: no blinks, no fixed 8 s; the display's state waited for, bounded
        self.assertNotIn("echo 10 >$b", boot)
        self.assertNotIn("sleep 8", boot)
        self.assertIn("ls /sys/class/drm/*/status", boot)
        self.assertIn("echo 10 >$b", debug)
        self.assertIn("sleep 8", debug)
        # the braces of the shell survive the template
        self.assertIn('say() { echo "maison: $*" >/dev/kmsg; }', boot)

    def test_the_address_is_adopted_once_into_roots_and_never_read_from_the_shells(self):
        boot = initramfs.init(False)
        self.assertIn("kept=/android-data/maison/target", boot)
        self.assertIn("left=/android-data/local/tmp/maison/target", boot)
        adopt = boot[boot.index("kept=") :]
        # root's copy, 0600, only while root keeps none; the shell's copy always removed
        self.assertIn("chown 0:0 $kept.new && chmod 600 $kept.new && sync && mv $kept.new $kept && sync", adopt)
        self.assertLess(adopt.index("if [ -s $kept ]"), adopt.index("cp $left"))
        self.assertIn("rm -f $left", adopt)
        # before the system starts
        self.assertLess(boot.index("rm -f $left"), boot.index("switch_root"))
        units = HERE / "linux/rootfs/etc/systemd/system"
        text = (units / "maison-ui.service").read_text()
        self.assertIn("LoadCredential=target:/run/maison-target\n", text)
        self.assertNotIn("local/tmp", text.split("LoadCredential=")[1])

    def test_no_root_console_but_in_a_bring_up_image(self):
        boot, debug = initramfs.init(False), initramfs.init(True)
        for shell in ("ttyGS0", "usb_gadget", "mksh -i"):
            self.assertNotIn(shell, boot)
            self.assertIn(shell, debug)

    def test_a_try_is_counted_before_it_starts_and_a_spent_update_goes_back(self):
        boot = initramfs.init(False)
        # the slots' words are one file's, the system's too, and the shell gets their escapes
        self.assertIn(initramfs.SLOTS, boot)
        self.assertIn("printf 'active=%s\\npending=%s\\ntries=%s\\nboots=%s\\n'", boot)
        self.assertIn(r"sed -n 's/.*maison\.slot=\([ab]\).*/\1/p' /proc/cmdline", boot)
        # counted, then chosen; an update out of tries, or not whole, leaves the active slot
        self.assertIn('[ "${tries:-0}" -gt 0 ] && whole "$pending" && remember "$active" "$pending" $((tries - 1)) "$boots"', boot)
        self.assertIn('remember "$active" "" 0 "$boots"', boot)
        self.assertIn('echo "$pending" >$slots/failed', boot)
        # the active slot's boots counted, the other slot tried, a slot for ever bad stops
        self.assertIn('[ "${boots:-0}" -ge 4 ] && whole "$(other $active)"', boot)
        self.assertIn('[ "${boots:-0}" -lt 8 ]', boot)
        # the slot taken instead is checked too: neither whole, nothing is mounted
        self.assertIn('whole "$active" || return 0', boot)
        # the state written whole: to a new file, synced, renamed
        self.assertIn("sync && mv $state.new $state && sync", boot)
        # a slot's kernel before the Wi-Fi holds its hardware, armed, userdata let go; a failed
        # kexec never leaves a trial running on another kernel
        self.assertLess(boot.index("kexec -l"), boot.index("insmod"))
        self.assertLess(boot.index("maison-guard arm"), boot.index("kexec -l"))
        self.assertLess(boot.index("umount /android-data"), boot.index("kexec -e"))
        self.assertIn('[ -z "$trial" ] || remember "$active" "$pending" 0 "$boots"', boot)
        self.assertLess(boot.index("maison-guard 240"), boot.index("switch_root"))
        # what cannot start spends its count
        self.assertLess(boot.index("switch_root"), boot.index('[ -z "$slot" ] || reboot -f'))

    def test_a_power_cut_finds_userdata_looked_at_and_closed_to_devices_and_programs(self):
        boot = initramfs.init(False)
        self.assertLess(boot.index("e2fsck -p"), boot.index("mount -t ext4"))
        self.assertIn("-o rw,noatime,nosuid,nodev,noexec", boot)
        self.assertNotIn("legacy", boot)

    def test_the_boot_shows_maison_and_its_ring_as_soon_as_the_panel_is_up(self):
        boot = initramfs.init(False)
        self.assertNotIn("synthese", boot)
        self.assertLess(boot.index("/splash/splash.raw"), boot.index("maison-loader "))
        self.assertLess(boot.index("maison-loader "), boot.index("data=$(grep"))

    def test_the_journal_is_one_directory_across_boots(self):
        boot = initramfs.init(False)
        self.assertIn("sha256sum /sys/block/mmcblk0/device/cid", boot)
        self.assertIn('echo "$id" >/newroot/etc/machine-id', boot)


def stubs(root: Path, **scripts) -> None:
    """Commands of the system, each a /bin/sh body, first in PATH."""
    bin_dir = root / "stubs"
    bin_dir.mkdir(exist_ok=True)
    for name, body in scripts.items():
        path = bin_dir / name.replace("_", "-")
        path.write_text(f"#!/bin/sh\nROOT={root}\nLOG=$ROOT/log\n{body}\n")
        path.chmod(0o755)


def rooted(text: str, root: Path, *directories: str) -> str:
    """A script of the frame with its absolute paths under `root`."""
    names = "|".join(re.escape(d) for d in directories)
    return re.sub(rf"(?<![\w.$}}/])/({names})\b", rf"{root}/\1", text)


class BootInit(unittest.TestCase):
    """The generated /init, run: the frame's commands stubbed, its paths in a temporary directory."""

    PATHS = ("sys", "proc", "dev", "android-data", "lower", "rw", "newroot", "tmp", "splash")
    STATE = "active=a\npending=\ntries=0\nboots=0\n"
    TRIAL = "active=a\npending=b\ntries=2\nboots=0\n"

    def boot(self, state, kernels="", broken=""):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        root = Path(d.name)
        (root / "log").touch()
        for name in self.PATHS:
            (root / name).mkdir()
        slots = root / "android-data/maison/slots"
        for slot in "ab":
            (slots / slot).mkdir(parents=True)
            (slots / slot / "sums").write_text("x\n")
            if slot in kernels:
                (slots / slot / "zImage").write_text("z")
            if slot in broken:
                (slots / slot / "bad").touch()
        (slots / "state").write_text(state)
        (root / "sys/class/block/p").mkdir(parents=True)
        (root / "sys/class/block/p/uevent").write_text("PARTNAME=userdata\nDEVNAME=p\n")
        (root / "proc/cmdline").write_text("root=x console=tty0 y\n")
        (root / "newroot/etc").mkdir()
        logged = 'echo "$(basename "$0") $*" >>$LOG'
        wait = "for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do [ -e $ROOT/%s ] && break; /bin/sleep 0.02; done"
        stubs(
            root,
            mount=logged,
            umount=logged,
            e2fsck=logged,
            kexec=logged,
            losetup=f'if [ "$1" = -f ]; then echo loop; else {logged}; fi',
            switch_root=logged,
            maison_guard=":",
            maison_loader=":",
            chown=":",
            sync=":",
            sleep=":",
            reboot=f"{logged}\nkill -9 $PPID",
            # each waits for the other, long enough to tell that one never overlaps the other
            insmod=f"""case "$1" in *fdrv*) {wait % "began"} ;; esac
{logged}
case "$1" in *fdrv*) : >$ROOT/loaded ;; esac""",
            sha256sum=f"""[ "$1" = -c ] || {{ echo 0123; exit 0; }}
echo "sums $(basename "$PWD") begin" >>$LOG
: >$ROOT/began
{wait % "loaded"}
echo "sums $(basename "$PWD") end" >>$LOG
[ ! -e bad ]""",
        )
        init = root / "init"
        init.write_text(rooted(initramfs.init(False), root, *self.PATHS).replace("export PATH=/bin", f"export PATH={root}/stubs:$PATH"))
        subprocess.run(["sh", str(init)], capture_output=True, timeout=30)
        return (root / "log").read_text().splitlines(), (slots / "state").read_text()

    @staticmethod
    def started(log):
        return [line.split("/")[-2] for line in log if line.startswith("losetup")]

    @staticmethod
    def loads(log):
        return [i for i, line in enumerate(log) if line.startswith("insmod")]

    def test_it_parses(self):
        for debug in (False, True):
            subprocess.run(["sh", "-n"], input=initramfs.init(debug).encode(), check=True)

    def test_without_a_kernel_the_driver_loads_during_the_sums_then_the_slot_starts(self):
        log, state = self.boot(self.STATE)
        self.assertEqual(len(self.loads(log)), 2, "loaded once")
        self.assertLess(log.index("sums a begin"), self.loads(log)[1])
        self.assertLess(self.loads(log)[1], log.index("sums a end"))
        self.assertLess(log.index("sums a end"), next(i for i, line in enumerate(log) if line.startswith("losetup")))
        self.assertEqual(self.started(log), ["a"])
        self.assertIn("boots=1", state)

    def test_a_slot_that_fails_its_sums_is_never_mounted(self):
        log, state = self.boot(self.TRIAL, broken="b")
        self.assertIn("sums b end", log)
        self.assertEqual(self.started(log), ["a"])
        self.assertEqual(state, "active=a\npending=\ntries=0\nboots=1\n")

    def test_a_try_is_counted_before_the_system_starts(self):
        log, state = self.boot(self.TRIAL)
        self.assertEqual(self.started(log), ["b"])
        self.assertEqual(state, "active=a\npending=b\ntries=1\nboots=0\n")

    def test_a_slot_with_its_kernel_starts_it_with_the_hardware_free(self):
        log, _ = self.boot(self.STATE, kernels="a")
        self.assertTrue([line for line in log if line.startswith("kexec -l")])
        self.assertEqual((self.loads(log), self.started(log)), ([], []))

    def test_a_kernel_a_failure_could_reach_keeps_the_driver_for_after_the_sums(self):
        log, _ = self.boot(self.STATE, kernels="b")
        self.assertLess(log.index("sums a end"), self.loads(log)[0])
        log, _ = self.boot(self.STATE, kernels="b", broken="a")
        self.assertTrue([line for line in log if line.startswith("kexec -l")])
        self.assertEqual(self.loads(log), [])

    def test_e2fsck_believes_no_clock_and_repairs_without_optimising(self):
        self.assertEqual(initramfs.E2FSCK_CONF, "[options]\nbroken_system_clock = 1\n")
        self.assertIn("e2fsck -p -E fixes_only $device", initramfs.init(False))


class WifiWatch(unittest.TestCase):
    """maison-wifi, maison-wifi-watch and maison-hotspot, run over a timeline of 15 s steps."""

    SBIN = HERE / "linux/rootfs/usr/local/sbin"
    PATHS = ("run", "sys", "android-data", "etc/wpa_supplicant", "etc/maison-target")
    OPEN = ["systemctl restart wpa_supplicant@wlan0", "systemctl --no-block start maison-setup", "networkctl reload", "systemctl restart maison-ui"]
    BACK = ["systemctl stop maison-setup", "systemctl restart wpa_supplicant@wlan0", "networkctl reload", "systemctl restart maison-ui"]
    OPERSTATE = "echo %s >$ROOT/sys/class/net/wlan0/operstate"
    # the power key held
    HELD = "maison-hotspot open"
    # a request on the page: maison-setup touches the file
    REQUEST = "touch -t 203001010000 $ROOT/run/maison-setup"

    def run_watch(self, until, events=None, state="active=a\npending=\ntries=0\nboots=0\n", target="https://a.b", wifi="network={\n}\n", built=None):
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        root = self.root = Path(d.name)
        maison = root / "android-data/maison"
        (maison / "slots").mkdir(parents=True)
        (maison / "slots/state").write_text(state)
        if wifi:
            (maison / "wifi.conf").write_text(wifi)
        if target is not None:
            (maison / "target").write_text(target)
        (root / "run").mkdir()
        (root / "etc").mkdir()
        if built:
            (root / "etc/maison-target").write_text(built)
        (root / "sys/class/net/wlan0").mkdir(parents=True)
        (root / "sys/class/net/wlan0/operstate").write_text("dormant\n")
        (root / "log").touch()
        (root / "clock").write_text("0")
        (root / "at").mkdir()
        for at, command in (events or {}).items():
            (root / f"at/{at}").write_text(command)
        slots = (self.SBIN.parent / "lib/maison-slots").read_text()
        (root / "maison-slots").write_text(rooted(slots, root, *self.PATHS))
        for name in ("maison-wifi", "maison-wifi-watch", "maison-hotspot"):
            script = rooted((self.SBIN / name).read_text(), root, *self.PATHS)
            (root / name).write_text(script.replace("/usr/local/lib/maison-slots", f"{root}/maison-slots"))
        stubs(
            root,
            sha256sum="echo 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            sleep=f"""t=$(($(cat $ROOT/clock) + $1))
echo $t >$ROOT/clock
[ "$t" -le {until} ] || exit 1
[ ! -e $ROOT/at/$t ] || . $ROOT/at/$t""",
            networkctl='echo "$(cat $ROOT/clock) networkctl $*" >>$LOG',
            maison_hotspot='sh $ROOT/maison-hotspot "$@"',
            # wpa_supplicant@wlan0 requires maison-wifi
            systemctl="""echo "$(cat $ROOT/clock) systemctl $*" >>$LOG
[ "$*" != "restart wpa_supplicant@wlan0" ] || sh $ROOT/maison-wifi""",
        )
        env = {**os.environ, "PATH": f"{root}/stubs:{os.environ['PATH']}"}
        subprocess.run(["sh", str(root / "maison-wifi")], env=env, check=True, capture_output=True)
        (root / "log").write_text("")
        self.done = subprocess.run(["sh", str(root / "maison-wifi-watch")], env=env, capture_output=True, timeout=30)
        return (root / "log").read_text().splitlines()

    @staticmethod
    def at(time, lines):
        return [f"{time} {line}" for line in lines]

    def conf(self):
        return (self.root / "etc/wpa_supplicant/wpa_supplicant-wlan0.conf").read_text()

    def lost(self):
        return (self.root / "run/maison-lost").exists()

    def test_ten_minutes_without_a_network_tell_the_interface_and_open_nothing(self):
        self.assertEqual(self.run_watch(585), [])
        self.assertFalse(self.lost())
        self.assertEqual(self.run_watch(4000), [], "no access point, however long")
        self.assertTrue(self.lost())
        self.assertNotIn("mode=2", self.conf())
        self.assertFalse((self.root / "run/maison-setup").exists())

    def test_an_association_says_the_network_is_back(self):
        self.run_watch(800, {705: self.OPERSTATE % "up"})
        self.assertFalse(self.lost())

    def test_a_lost_link_counts_from_its_loss(self):
        events = {300: self.OPERSTATE % "up", 1500: self.OPERSTATE % "dormant"}
        self.run_watch(1500 + 570, events)
        self.assertFalse(self.lost())
        self.run_watch(1500 + 600, events)
        self.assertTrue(self.lost())

    def test_never_while_an_update_is_on_trial(self):
        self.assertEqual(self.run_watch(1000, state="active=a\npending=b\ntries=1\nboots=0\n"), [])
        self.assertFalse(self.lost())
        self.assertEqual(self.done.returncode, 1, "stopped by the timeline, not by itself")

    def test_a_frame_with_no_network_scans_with_no_access_point(self):
        self.assertEqual(self.run_watch(100, wifi=None, target=None), [])
        self.assertEqual(self.conf(), "ctrl_interface=/run/wpa_supplicant\ncountry=FR\n\n".replace("/run", f"{self.root}/run"))
        self.assertEqual(self.done.returncode, 1, "it keeps watching")

    def test_the_address_is_roots_else_the_images_else_none(self):
        address = lambda: (self.root / "run/maison-target").read_text()
        self.run_watch(15, target="https://a.b", built="https://built")
        self.assertEqual(address(), "https://a.b")
        self.run_watch(15, target=None, built="https://built")
        self.assertEqual(address(), "https://built")
        self.assertFalse((self.root / "android-data/maison/target").exists(), "root's stays root's to write")
        self.run_watch(15, target="", built="https://built")
        self.assertEqual(address(), "https://built", "an older frame's empty file says none")
        self.run_watch(15, target=None)
        self.assertEqual(address(), "")
        self.assertEqual(stat.S_IMODE((self.root / "run/maison-target").stat().st_mode), 0o600)

    def test_the_power_key_held_opens_the_frames_own_wifi(self):
        self.assertEqual(self.run_watch(1100, {990: self.HELD}), self.at(990, self.OPEN))
        self.assertIn("mode=2", self.conf())
        setup = (self.root / "run/maison-setup").read_text()
        self.assertIn("MAISON_SETUP_SSID=Maison-0123\n", setup)
        self.assertIn("MAISON_SETUP_PASSWORD=456789abcdef\n", setup)
        self.assertIn("MAISON_SETUP_URL=http://192.168.4.1\n", setup)
        network = self.root / "run/systemd/network"
        self.assertEqual(stat.S_IMODE(network.stat().st_mode), 0o755, "networkd's user reads it")
        self.assertEqual(stat.S_IMODE((network / "40-maison-setup.network").stat().st_mode), 0o644)

    def test_held_twice_it_opens_once(self):
        self.assertEqual(self.run_watch(1100, {990: self.HELD, 1050: self.HELD}), self.at(990, self.OPEN))

    def test_thirty_minutes_later_it_returns_to_its_network(self):
        back = 990 + 1800
        self.assertEqual(self.run_watch(back + 60, {990: self.HELD}), self.at(990, self.OPEN) + self.at(back, self.BACK))
        self.assertNotIn("mode=2", self.conf())
        self.assertFalse((self.root / "run/maison-setup").exists())
        self.assertFalse((self.root / "run/maison-hotspot").exists())
        self.assertFalse((self.root / "run/systemd/network/40-maison-setup.network").exists())

    def test_someone_at_the_page_holds_the_way_back(self):
        self.assertEqual(self.run_watch(4300, {990: self.HELD, 2400: self.REQUEST}), self.at(990, self.OPEN) + self.at(2400 + 1800, self.BACK))

    def test_it_says_how_it_is_used(self):
        done = subprocess.run(["sh", str(self.SBIN / "maison-hotspot")], capture_output=True)
        self.assertEqual(done.returncode, 2)

    def test_its_ten_minutes_come_after_maison_commit_is_done(self):
        watch = (self.SBIN / "maison-wifi-watch").read_text()
        commit = (self.SBIN / "maison-commit").read_text()
        self.assertIn("patience=600\n", watch)
        self.assertIn('[ "$waited" -lt 150 ]', commit)

    def test_only_the_power_key_opens_the_access_point(self):
        for name in ("maison-wifi-watch", "maison-commit", "maison-wifi"):
            self.assertNotIn("hotspot open", (self.SBIN / name).read_text())
        self.assertNotRegex((self.SBIN / "maison-wifi-watch").read_text(), r": >\"\$hotspot\"")

    def test_the_image_holds_all_of_it_and_the_power_key_shuts_nothing_down(self):
        linux = HERE / "linux"
        post = (linux / "buildroot/post-build.sh").read_text()
        for unit in ("maison-wifi-watch", "maison-wifid", "maison-power-key"):
            self.assertIn(f"enable {unit}.service\n", post)
        for tool in ("maison-wifid", "maison-power-key", "maison-setup"):
            self.assertIn(f"--bin {tool}", (linux / "buildroot.sh").read_text())
        self.assertIn("maison-setup maison-wifid maison-power-key", post)
        self.assertIn("BR2_PACKAGE_WPA_SUPPLICANT_AP_SUPPORT=y\n", (linux / "buildroot/maison_defconfig").read_text())
        units = linux / "rootfs/etc/systemd/system"
        self.assertNotIn("RemainAfterExit=", (units / "maison-wifi.service").read_text())
        self.assertIn("EnvironmentFile=-/run/maison-setup\n", (units / "maison-ui.service").read_text())
        for script in ("maison-wifi-watch", "maison-hotspot"):
            self.assertTrue(os.access(self.SBIN / script, os.X_OK))
        logind = (linux / "rootfs/etc/systemd/logind.conf.d/maison.conf").read_text()
        self.assertIn("HandlePowerKey=ignore\n", logind)
        self.assertIn("HandlePowerKeyLongPress=ignore\n", logind)

    def test_the_interface_reaches_the_service_by_its_group_alone(self):
        unit = (HERE / "linux/rootfs/etc/systemd/system/maison-wifid.service").read_text()
        self.assertIn("Group=kiosk\n", unit)
        self.assertIn("RuntimeDirectoryMode=0750\n", unit)
        self.assertIn('pub const SOCKET: &str = "/run/maison-wifi/socket";', (HERE / "tools/src/wifi.rs").read_text())

    def test_a_default_address_at_build_is_https(self):
        script = (HERE / "linux/buildroot.sh").read_text()
        self.assertIn('"" | https://?*) ;;', script)
        self.assertIn('printf "%s" "$ADDRESS" >/tmp/maison-target', script)
        self.assertIn('cp /tmp/maison-target "$root/etc/maison-target"', (HERE / "linux/buildroot/post-build.sh").read_text())


if __name__ == "__main__":
    unittest.main()

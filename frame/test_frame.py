"""The frame's tooling, without the frame or its backup: synthetic images only.

    python3 -m unittest discover -s frame -p 'test_*.py'
"""

import gzip
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).parent
sys.path[:0] = [str(HERE / "system"), str(HERE)]
import bootimg  # noqa: E402
import build  # noqa: E402
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
        for line in ("ro.keep=1", "ro.product.locale=fr-FR", "persist.sys.timezone=Europe/Paris", "service.adb.tcp.port=5555"):
            self.assertIn(line, out.splitlines())


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


if __name__ == "__main__":
    unittest.main()

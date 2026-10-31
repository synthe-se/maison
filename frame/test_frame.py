"""The frame's tooling, without the frame or its backup: synthetic images only.

    python3 -m unittest discover -s frame -p 'test_*.py'
"""

import gzip
import os
import struct
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
        self.assertIn("LoadCredential=target:/android-data/maison/target\n", text)
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

    def test_the_journal_is_one_directory_across_boots(self):
        boot = initramfs.init(False)
        self.assertIn("sha256sum /sys/block/mmcblk0/device/cid", boot)
        self.assertIn('echo "$id" >/newroot/etc/machine-id', boot)


if __name__ == "__main__":
    unittest.main()

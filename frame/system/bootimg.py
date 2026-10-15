"""The frame's boot image (Android 6 boot.img, header v0), unpacked, its ramdisk changed, packed again.

What mkbootimg would write: the kernel and the second stage (Rockchip's resource) untouched, the
ramdisk a newc cpio rebuilt from the original's entries (modes, owners and inodes kept), the
header's id the SHA-1 AOSP's mkbootimg computes. The vendor's own id matches no SHA-1 of the image
we could find (its ramdisk was likely patched after packing, SuperSU's daemon), and its U-Boot
boots it anyway: the standard id is the best one to write. `python3 bootimg.py check <boot.img>`
proves the packing against the vendor's image: repacked with its own ramdisk and id, the image
comes out byte for byte, and its cpio rewrites byte for byte.
"""

import gzip
import hashlib
import struct
import sys

MAGIC = b"ANDROID!"
# magic, kernel size/addr, ramdisk size/addr, second size/addr, tags addr, page size, 2 unused,
# name, cmdline, id (8 words), extra cmdline
HEADER = struct.Struct("<8s10I16s512s32s1024s")


class BootImage:
    def __init__(self, data: bytes):
        fields = HEADER.unpack_from(data)
        if fields[0] != MAGIC:
            raise ValueError("not an Android boot image")
        (_, ks, self.kernel_addr, rs, self.ramdisk_addr, ss, self.second_addr, self.tags_addr, self.page, *unused) = fields[:11]
        self.unused = unused
        self.name, self.cmdline, self.id, self.extra = fields[11:]
        o = self.page
        self.kernel = data[o : o + ks]
        o += self._pad(ks)
        self.ramdisk = data[o : o + rs]
        o += self._pad(rs)
        self.second = data[o : o + ss]

    def _pad(self, n: int) -> int:
        return (n + self.page - 1) // self.page * self.page

    def digest(self) -> bytes:
        """mkbootimg's id: SHA-1 over each part followed by its size (little-endian 32 bits)."""
        sha = hashlib.sha1()
        for part in (self.kernel, self.ramdisk, self.second):
            sha.update(part)
            sha.update(struct.pack("<I", len(part)))
        return sha.digest().ljust(32, b"\0")

    def pack(self, id: bytes | None = None) -> bytes:
        header = HEADER.pack(
            MAGIC, len(self.kernel), self.kernel_addr, len(self.ramdisk), self.ramdisk_addr, len(self.second),
            self.second_addr, self.tags_addr, self.page, *self.unused, self.name, self.cmdline, id or self.digest(), self.extra,
        )
        out = bytearray()
        for part in (header, self.kernel, self.ramdisk, self.second):
            out += part + b"\0" * (self._pad(len(part)) - len(part))
        return bytes(out)


# ---- the ramdisk: a gzipped newc cpio

NEWC = "070701"
FIELDS = ("ino", "mode", "uid", "gid", "nlink", "mtime", "size", "devmajor", "devminor", "rdevmajor", "rdevminor", "namesize", "check")


def read_cpio(data: bytes) -> list[dict]:
    entries, o = [], 0
    while True:
        if data[o : o + 6].decode() != NEWC:
            raise ValueError(f"not a newc cpio at {o}")
        e = {k: int(data[o + 6 + 8 * i : o + 14 + 8 * i], 16) for i, k in enumerate(FIELDS)}
        o += 110
        e["name"] = data[o : o + e["namesize"] - 1].decode()
        o = (o + e["namesize"] + 3) & ~3
        if e["name"] == "TRAILER!!!":
            return entries
        e["data"] = data[o : o + e["size"]]
        o = (o + e["size"] + 3) & ~3
        entries.append(e)


def write_cpio(entries: list[dict]) -> bytes:
    """As AOSP's mkbootfs writes it: inodes in order, a trailer that takes the next inode with mode
    0644, the archive padded to 256 bytes."""
    trailer = {"name": "TRAILER!!!", "data": b"", "ino": max(e["ino"] for e in entries) + 1, "mode": 0o644, "nlink": 1}
    out = bytearray()
    for e in [*entries, trailer]:
        name = e["name"].encode() + b"\0"
        values = {k: e.get(k, 0) for k in FIELDS} | {"size": len(e["data"]), "namesize": len(name), "check": 0}
        out += (NEWC + "".join(f"{values[k]:08x}" for k in FIELDS)).encode() + name
        out += b"\0" * (-len(out) % 4) + e["data"]
        out += b"\0" * (-len(out) % 4)
    return bytes(out + b"\0" * (-len(out) % 256))


class Ramdisk:
    def __init__(self, gz: bytes):
        self.entries = read_cpio(gzip.decompress(gz))

    def _entry(self, name: str) -> dict | None:
        return next((e for e in self.entries if e["name"] == name), None)

    def text(self, name: str) -> str:
        return self._entry(name)["data"].decode()

    def put(self, name: str, data: bytes, mode: int = 0o100644):
        """Replaces a file's content (its mode, owner and inode kept), or adds it, root's."""
        e = self._entry(name)
        if e is None:
            ino = max(x["ino"] for x in self.entries) + 1
            self.entries.append({"name": name, "data": data, "ino": ino, "mode": mode, "uid": 0, "gid": 0, "nlink": 1, "mtime": 0})
        else:
            e["data"] = data

    def pack(self) -> bytes:
        return gzip.compress(write_cpio(self.entries), compresslevel=9, mtime=0)


# ---- what Maison changes in it

def drop_service(rc: str, name: str) -> str:
    """An init service gone: its `service` line and the indented options under it."""
    out, skipping = [], False
    for line in rc.splitlines(keepends=True):
        if line.startswith("service "):
            skipping = line.split()[1] == name
        elif line.strip() and not line[0].isspace():
            skipping = False
        if not skipping:
            out.append(line)
    return "".join(out)


def set_props(props: str, values: dict[str, str]) -> str:
    """Each `key=value` set (replaced where it is, else appended)."""
    lines, seen = [], set()
    for line in props.splitlines():
        key = line.split("=", 1)[0].strip()
        if key in values:
            line, _ = f"{key}={values[key]}", seen.add(key)
        lines.append(line)
    lines += [f"{k}={v}" for k, v in values.items() if k not in seen]
    return "\n".join(lines) + "\n"


def maison(boot: BootImage, adb_keys: bytes) -> None:
    rd = Ramdisk(boot.ramdisk)
    rc = rd.text("init.rc")
    # no root for apps: SuperSU's daemon and the set-uid bits it relied on go
    rc = drop_service(rc, "daemonsu")
    rc = "".join(l for l in rc.splitlines(keepends=True) if l.strip() not in ("chmod 06755 system/xbin/su", "chmod 06755 system/xbin/daemonsu"))
    # it would « repair » recovery from the vendor's boot image, which is no longer this one
    rc = drop_service(rc, "flash_recovery")
    rd.put("init.rc", rc.encode())
    board = drop_service(rd.text("init.rk30board.rc"), "startup_rington")  # the boot chime
    rd.put("init.rk30board.rc", board.encode())
    # `adb root` instead of su; ADB over Wi-Fi (keys still required: only these)
    props = set_props(rd.text("default.prop"), {"ro.debuggable": "1", "persist.sys.usb.config": "adb", "service.adb.tcp.port": "5555"})
    rd.put("default.prop", props.encode())
    rd.put("adb_keys", adb_keys, mode=0o100644)
    boot.ramdisk = rd.pack()


def main(argv: list[str]) -> int:
    if argv[:1] == ["check"] and len(argv) == 2:
        image = open(argv[1], "rb").read()
        boot = BootImage(image)
        packed = boot.pack(boot.id)
        ok = packed == image[: len(packed)] and not image[len(packed) :].strip(b"\0")
        print(f"{argv[1]}: repack {'identical' if ok else 'differs'}")
        rd = Ramdisk(boot.ramdisk)
        same = gzip.decompress(boot.ramdisk) == write_cpio(rd.entries)
        print(f"ramdisk: {len(rd.entries)} entries, cpio rewrite {'identical' if same else 'differs'}")
        return 0 if ok and same else 1
    if argv[:1] == ["maison"] and len(argv) == 4:
        src, adb_keys, out = argv[1:]
        boot = BootImage(open(src, "rb").read())
        maison(boot, open(adb_keys, "rb").read())
        open(out, "wb").write(boot.pack())
        print(f"{out}: {len(boot.pack())} bytes")
        return 0
    print("usage: bootimg.py check <boot.img> | maison <boot.img> <adb_keys> <out.img>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

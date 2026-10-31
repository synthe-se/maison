#!/usr/bin/env bash
# The rescue recovery's tools, built here from their sources, static, for the frame's ARMv7: mksh
# (Android's own shell) and toybox (Android's own userland). The vendor recovery has no shell, and
# its adbd never runs as root (a user build): bootimg.py `rescue` puts these in its ramdisk.
# Sources pinned by version and SHA-256; nothing built is kept in the repo.
# Needs zig (mksh) and Docker with ARMv7 emulation (toybox's build wants GNU tools: an Alpine).
# Usage: frame/system/rescue-tools.sh <out dir>     → <out>/mksh, <out>/toybox, <out>/applets.txt, <out>/maison-loader, <out>/maison-guard, <out>/kexec, <out>/e2fsck
set -euo pipefail

OUT="$(mkdir -p "$1" && cd "$1" && pwd)"
MKSH=R59c
MKSH_SHA256=77ae1665a337f1c48c61d6b961db3e52119b38e58884d1c89684af31f87bc506
TOYBOX=0.8.15
TOYBOX_SHA256=44e2bab43dfe2c171743297631ce558c6d9ed38b87a66fd751cef617574b375e
E2FSPROGS=1.47.4
E2FSPROGS_SHA256=fd5bf388cbdbe006a3d3b318d983b2948382440acc85a87f1e7d108653e8db0b
KEXEC=2.0.32
KEXEC_SHA256=8f81422a5fd2362cf6cb001b511e535565ed0f32c2f4451fb5eb68fed6710a5d
ALPINE=alpine:3.24.3
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fetch() { # fetch <url> <sha256>
	curl -sfL "$1" -o "$TMP/$(basename "$1")"
	echo "$2  $TMP/$(basename "$1")" | shasum -a 256 -c - >/dev/null
	tar xzf "$TMP/$(basename "$1")" -C "$TMP"
}

fetch "https://mbsd.evolvis.org/MirOS/dist/mir/mksh/mksh-$MKSH.tgz" "$MKSH_SHA256"
(cd "$TMP/mksh" && CC="zig cc -target arm-linux-musleabihf -mcpu=cortex_a7" CFLAGS=-Os LDFLAGS=-static TARGET_OS=Linux sh Build.sh -r >build.log 2>&1)
cp "$TMP/mksh/mksh" "$OUT/mksh"

fetch "https://landley.net/toybox/downloads/toybox-$TOYBOX.tar.gz" "$TOYBOX_SHA256"
# Alpine's fortify headers clash with toybox's own string helpers
docker run --rm --platform linux/arm/v7 -v "$TMP/toybox-$TOYBOX:/src" -w /src "$ALPINE" sh -c \
	'apk add -q build-base linux-headers bash >/dev/null && make defconfig >/dev/null &&
	 CFLAGS="-Os -U_FORTIFY_SOURCE -D_FORTIFY_SOURCE=0" LDFLAGS=--static make toybox >/dev/null'
cp -f "$TMP/toybox-$TOYBOX/toybox" "$OUT/toybox"
# the boot's loader (frame/linux/loader.c, the mainline initramfs's): nothing to download
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
zig cc -target arm-linux-musleabihf -mcpu=cortex_a7 -O2 -s -static -o "$OUT/maison-loader" "$REPO/frame/linux/loader.c" -lm
zig cc -target arm-linux-musleabihf -mcpu=cortex_a7 -O2 -s -static -o "$OUT/maison-guard" "$REPO/frame/linux/guard.c"
# kexec, static: the initramfs starts a slot's kernel
fetch "https://cdn.kernel.org/pub/linux/utils/kernel/kexec/kexec-tools-$KEXEC.tar.xz" "$KEXEC_SHA256"
docker run --rm --platform linux/arm/v7 -v "$TMP/kexec-tools-$KEXEC:/src" -w /src "$ALPINE" sh -c \
	'apk add -q build-base linux-headers >/dev/null && LDFLAGS=-static ./configure --without-zlib --without-lzma >/dev/null && make -j8 build/sbin/kexec >/dev/null 2>&1 && strip build/sbin/kexec'
cp "$TMP/kexec-tools-$KEXEC/build/sbin/kexec" "$OUT/kexec"
# e2fsck, static: a power cut's userdata looked at before the system mounts it
fetch "https://cdn.kernel.org/pub/linux/kernel/people/tytso/e2fsprogs/v$E2FSPROGS/e2fsprogs-$E2FSPROGS.tar.xz" "$E2FSPROGS_SHA256"
docker run --rm --platform linux/arm/v7 -v "$TMP/e2fsprogs-$E2FSPROGS:/src" -w /src "$ALPINE" sh -c \
	'apk add -q build-base linux-headers >/dev/null && ./configure --disable-nls --disable-fuse2fs --disable-defrag LDFLAGS=-static >/dev/null && make -j8 libs >/dev/null 2>&1 && make -j8 -C e2fsck e2fsck LDFLAGS=-static >/dev/null 2>&1 && strip e2fsck/e2fsck'
cp -f "$TMP/e2fsprogs-$E2FSPROGS/e2fsck/e2fsck" "$OUT/e2fsck"
# what it can do, one name a line: each gets its link beside it in the rescue shell
docker run --rm --platform linux/arm/v7 -v "$OUT:/t" "$ALPINE" /t/toybox | tr -s ' \n' '\n' | grep . >"$OUT/applets.txt"
file "$OUT/mksh" "$OUT/toybox" | sed "s|$OUT/||"

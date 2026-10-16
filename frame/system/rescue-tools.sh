#!/usr/bin/env bash
# The rescue recovery's tools, built here from their sources, static, for the frame's ARMv7: mksh
# (Android's own shell) and toybox (Android's own userland). The vendor recovery has no shell, and
# its adbd never runs as root (a user build): bootimg.py `rescue` puts these in its ramdisk.
# Sources pinned by version and SHA-256; nothing built is kept in the repo.
# Needs zig (mksh) and Docker with ARMv7 emulation (toybox's build wants GNU tools: an Alpine).
# Usage: frame/system/rescue-tools.sh <out dir>     → <out>/mksh, <out>/toybox, <out>/applets.txt
set -euo pipefail

OUT="$(mkdir -p "$1" && cd "$1" && pwd)"
MKSH=R59c
MKSH_SHA256=77ae1665a337f1c48c61d6b961db3e52119b38e58884d1c89684af31f87bc506
TOYBOX=0.8.15
TOYBOX_SHA256=44e2bab43dfe2c171743297631ce558c6d9ed38b87a66fd751cef617574b375e
ALPINE=alpine:3.24
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
cp "$TMP/toybox-$TOYBOX/toybox" "$OUT/toybox"
# what it can do, one name a line: each gets its link beside it in the rescue shell
docker run --rm --platform linux/arm/v7 -v "$OUT:/t" "$ALPINE" /t/toybox | tr -s ' \n' '\n' | grep . >"$OUT/applets.txt"
file "$OUT/mksh" "$OUT/toybox" | sed "s|$OUT/||"

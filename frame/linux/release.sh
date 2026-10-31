#!/usr/bin/env bash
# A release for the frames over the air: what frame/linux/buildroot.sh last built, and with --kernel
# what frame/linux/build.sh and initramfs.py built, under frame/build/release/, signed with the
# release key (~/maison-frame/keys, never in the repo). Upload that directory's content as it is,
# `manifest.json` and `manifest.sig` last: a frame reads them first.
# Usage: frame/linux/release.sh [--kernel]
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD="$REPO/frame/build"
KEY="$HOME/maison-frame/keys/release.pk8"
OUT="$BUILD/release"
[ -s "$KEY" ] || { echo "no $KEY: the release key" >&2; exit 1; }
version=$(cat "$BUILD/buildroot/version")
sign() { cargo run -q --release --manifest-path "$REPO/frame/tools/Cargo.toml" --bin maison-sign -- "$@"; }

rm -rf "$OUT"
mkdir -p "$OUT/$version"
cp "$BUILD/buildroot/rootfs.squashfs" "$OUT/$version/"
if [ "${1:-}" = --kernel ]; then
	cp "$BUILD/linux/zImage" "$OUT/$version/"
	cp "$BUILD/linux/rk3126-amaboo-gbzk-y1000.dtb" "$OUT/$version/board.dtb"
	cp "$BUILD/linux-initramfs.cpio.gz" "$OUT/$version/initramfs.cpio.gz"
fi
sign describe "$version" "$OUT/$version"
mv "$OUT/$version/manifest.json" "$OUT/manifest.json"
sign sign "$KEY" "$OUT/manifest.json"
ls -la "$OUT" "$OUT/$version"

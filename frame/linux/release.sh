#!/usr/bin/env bash
# A release for the frames over the air: what frame/linux/buildroot.sh last built, and with --kernel
# what frame/linux/build.sh and initramfs.py built, under frame/build/release/, signed with the
# release key (~/maison-frame/keys, never in the repo). With --publish, on to updates.synthe.se: the
# files first, `manifest.json` and `manifest.sig` last, which a frame reads first.
# Usage: frame/linux/release.sh [--kernel] [--publish]
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
kernel=false publish=false
for a in "$@"; do
	case "$a" in --kernel) kernel=true ;; --publish) publish=true ;; *) echo "unknown: $a" >&2; exit 2 ;; esac
done
if $kernel; then
	cp "$BUILD/linux/zImage" "$OUT/$version/"
	cp "$BUILD/linux/rk3126-amaboo-gbzk-y1000.dtb" "$OUT/$version/board.dtb"
	cp "$BUILD/linux-initramfs.cpio.gz" "$OUT/$version/initramfs.cpio.gz"
fi
sign describe "$version" "$OUT/$version"
mv "$OUT/$version/manifest.json" "$OUT/manifest.json"
sign sign "$KEY" "$OUT/manifest.json"
ls -la "$OUT" "$OUT/$version"
if $publish; then
	HOST="${FRAME_UPDATE_HOST:-apps@yuki.synthe.se}"
	rsync -a "$OUT/$version" "$HOST:/srv/updates/frame/"
	rsync -a "$OUT/manifest.json" "$OUT/manifest.sig" "$HOST:/srv/updates/frame/"
	echo "published: $version"
fi

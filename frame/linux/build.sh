#!/usr/bin/env bash
# The frame's mainline kernel: Linux (pinned, checked against kernel.org's sums), our patches, our
# board's device tree, multi_v7_defconfig + maison.config, built with LLVM in an arm64 Alpine (the
# Mac's own architecture: no emulation) for 32-bit ARM. Sources and build stay off the repo, in
# ~/maison-frame/src and a Docker volume; the outputs land in frame/build/linux.
# Usage: frame/linux/build.sh
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION=7.2.11
SRC="$HOME/maison-frame/src"
OUT="$REPO/frame/build/linux"
VOLUME=maison-linux-$VERSION
DTB=rk3126-amaboo-gbzk-y1000
ALPINE=alpine:3.24

mkdir -p "$SRC" "$OUT"
tarball="$SRC/linux-$VERSION.tar.xz"
[ -f "$tarball" ] || curl -sfL "https://cdn.kernel.org/pub/linux/kernel/v${VERSION%%.*}.x/linux-$VERSION.tar.xz" -o "$tarball"
curl -sfL "https://cdn.kernel.org/pub/linux/kernel/v${VERSION%%.*}.x/sha256sums.asc" | grep " linux-$VERSION.tar.xz$" |
	(cd "$SRC" && shasum -a 256 -c - >/dev/null)

docker volume create "$VOLUME" >/dev/null
docker run --rm -v "$VOLUME:/build" -v "$SRC:/src:ro" -v "$REPO/frame/linux:/frame:ro" -v "$OUT:/out" \
	"$ALPINE" sh -euc '
	apk add -q build-base clang lld llvm bison flex bc perl findutils linux-headers openssl-dev elfutils-dev python3 xz patch >/dev/null
	cd /build
	if [ ! -f .maison-tree ]; then
		tar xJf /src/linux-'"$VERSION"'.tar.xz --strip-components=1
		echo "dtb-\$(CONFIG_ARCH_ROCKCHIP) += '"$DTB"'.dtb" >>arch/arm/boot/dts/rockchip/Makefile
		touch .maison-tree
	fi
	# the patches as they are now: a changed one gets its files back from the tarball, then all
	# apply again on them
	sum=$(cat /frame/patches/*.patch | sha256sum | cut -c1-64)
	if [ "$(cat .maison-patches 2>/dev/null)" != "$sum" ]; then
		files=$(grep -h "^+++ b/" /frame/patches/*.patch | awk "{print \$2}" | cut -c3- | sort -u)
		tar xJf /src/linux-'"$VERSION"'.tar.xz --strip-components=1 $(for f in $files; do echo "linux-'"$VERSION"'/$f"; done)
		for p in /frame/patches/*.patch; do patch -p1 -s <"$p"; done
		echo "$sum" >.maison-patches
	fi
	cp /frame/'"$DTB"'.dts arch/arm/boot/dts/rockchip/
	m="make ARCH=arm LLVM=1 -j$(nproc)"
	$m multi_v7_defconfig >/dev/null
	scripts/kconfig/merge_config.sh -m -O . .config /frame/maison.config >/dev/null
	$m olddefconfig >/dev/null
	# the fragment must have been taken whole: a symbol kconfig dropped is a silent difference
	missing=$(grep -E "^CONFIG_[A-Z0-9_]+=" /frame/maison.config | while read -r l; do grep -qx "$l" .config || echo "$l"; done)
	[ -z "$missing" ] || { echo "not taken by kconfig: $missing" >&2; exit 1; }
	# the build fails the script; its warnings are shown, not hidden
	if ! $m zImage rockchip/'"$DTB"'.dtb >build.log 2>&1; then grep -E "error" build.log | head -20; exit 1; fi
	grep -E "warning: " build.log | head -20 || true
	# U-Boot loads the initramfs at 0x62000000 (the vendor parameter: initrd=0x62000000,…): the
	# decompressed kernel up to its end, and the copy the decompressor makes of itself past it,
	# must end below, with a margin
	end=$(( 0x$(grep -E " _end$" System.map | cut -c1-8) - 0xc0000000 + 0x60000000 ))
	zsize=$(stat -c %s arch/arm/boot/zImage)
	limit=$(( 0x62000000 - 0x200000 ))
	[ $(( end + zsize )) -lt $limit ] || { printf "too big: the kernel ends at 0x%x, +%d of zImage, past 0x%x\n" $end $zsize $limit >&2; exit 1; }
	printf "fits: the kernel ends at 0x%x, +%d of zImage, below 0x%x\n" $end $zsize $limit
	# the display modules alone, not multi_v7'"'"'s two thousand
	# in the order they load (initramfs.py keeps it): helpers, the PHY, the panel, the driver
	mods="drivers/gpu/drm/drm_dma_helper.ko drivers/gpu/drm/bridge/synopsys/dw-mipi-dsi.ko drivers/phy/rockchip/phy-rockchip-inno-dsidphy.ko drivers/gpu/drm/panel/panel-jadard-jd9365da-h3.ko drivers/gpu/drm/rockchip/rockchipdrm.ko"
	if ! $m $mods >>build.log 2>&1; then grep -E "error" build.log | tail -20; exit 1; fi
	mkdir -p /out/modules && rm -f /out/modules/*.ko
	n=0; for k in $mods; do n=$((n+1)); llvm-strip --strip-debug -o /out/modules/$n-$(basename $k) $k; done
	cp arch/arm/boot/zImage arch/arm/boot/dts/rockchip/'"$DTB"'.dtb .config /out/
	echo "built $(make -s kernelrelease)"
'
ls -la "$OUT"

#!/usr/bin/env bash
# The frame's mainline kernel: Linux (pinned, checked against kernel.org's sums), our patches, our
# board's device tree, allnoconfig + maison.config, built with LLVM in an arm64 Alpine (the
# build host's architecture: no emulation) for 32-bit ARM; and the AIC8800's Wi-Fi driver (Radxa's
# package of AIC's source, pinned, with its own patches: no driver in mainline), its firmware too,
# and wireless-regdb for cfg80211.
# Sources and build stay off the repo, in ~/maison-frame/src and a Docker volume; the outputs land
# in frame/build/linux (aic8800/: the modules and the firmware the initramfs ships).
# With --initramfs <file.cpio.gz> (initramfs.py's, itself built on these modules) the kernel is
# linked again with it inside, as frame/build/linux/zImage.boot: U-Boot puts an image's initramfs
# where only its own device tree says, which the kernel in `boot` does not read.
# Usage: frame/linux/build.sh [--initramfs <file.cpio.gz>]
set -euo pipefail
. "$(dirname "$0")/../docker.sh"

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION=7.3.1
SRC="$HOME/maison-frame/src"
OUT="$REPO/frame/build/linux"
VOLUME=maison-linux-$VERSION
DTB=rk3126-amaboo-gbzk-y1000
ALPINE=alpine:3.24.3
AIC8800=https://github.com/radxa-pkg/aic8800.git
AIC8800_REV=fce88024ed40279def034e203128e0892fdbcac3
REGDB=2026.09.03

mkdir -p "$SRC" "$OUT"
if [ "${1:-}" = --initramfs ]; then
	gzip -dc "$2" >"$OUT/initramfs.cpio"
	docker run --rm --platform linux/arm64 -v "$VOLUME:/build" -v "$OUT:/out" "$ALPINE" sh -euc '
		apk add -q build-base clang lld llvm bison flex bc perl findutils linux-headers openssl-dev elfutils-dev python3 xz lz4 lzop patch bash >/dev/null
		cd /build
		cp /out/initramfs.cpio usr/maison-initramfs.cpio
		m="make ARCH=arm LLVM=1 KCFLAGS=-mtune=cortex-a7 -j$(nproc)"
		scripts/config --set-str INITRAMFS_SOURCE usr/maison-initramfs.cpio --enable INITRAMFS_COMPRESSION_GZIP
		$m olddefconfig >/dev/null
		if ! $m zImage >embed.log 2>&1; then grep -E "error|Error" embed.log | head -20; exit 1; fi
		if grep -E "warning:|Warning" embed.log; then echo "warnings in the build" >&2; exit 1; fi
		grep -q "^CONFIG_INITRAMFS_SOURCE=\"usr/maison-initramfs.cpio\"$" .config
		cp arch/arm/boot/zImage /out/zImage.boot
		scripts/config --set-str INITRAMFS_SOURCE ""
		$m olddefconfig >/dev/null
		echo "zImage.boot: $(stat -c %s /out/zImage.boot) bytes"
	'
	rm -f "$OUT/initramfs.cpio"
	exit 0
fi
tarball="$SRC/linux-$VERSION.tar.xz"
[ -f "$tarball" ] || curl -sfL "https://cdn.kernel.org/pub/linux/kernel/v${VERSION%%.*}.x/linux-$VERSION.tar.xz" -o "$tarball"
curl -sfL "https://cdn.kernel.org/pub/linux/kernel/v${VERSION%%.*}.x/sha256sums.asc" | grep " linux-$VERSION.tar.xz$" |
	(cd "$SRC" && shasum -a 256 -c - >/dev/null)
[ -d "$SRC/aic8800/.git" ] || git clone -q "$AIC8800" "$SRC/aic8800"
git -C "$SRC/aic8800" cat-file -e "$AIC8800_REV^{commit}" 2>/dev/null || git -C "$SRC/aic8800" fetch -q origin
git -C "$SRC/aic8800" -c advice.detachedHead=false checkout -q "$AIC8800_REV"
# the Wi-Fi's law (FR, not the world's lowest denominator), signed: cfg80211 checks it
regdb="$SRC/wireless-regdb-$REGDB.tar.xz"
[ -f "$regdb" ] || curl -sfL "https://mirrors.edge.kernel.org/pub/software/network/wireless-regdb/wireless-regdb-$REGDB.tar.xz" -o "$regdb"
curl -sfL https://mirrors.edge.kernel.org/pub/software/network/wireless-regdb/sha256sums.asc | grep " wireless-regdb-$REGDB.tar.xz$" |
	(cd "$SRC" && shasum -a 256 -c - >/dev/null)

docker volume create "$VOLUME" >/dev/null
docker run --rm --platform linux/arm64 -v "$VOLUME:/build" -v "$SRC:/src:ro" -v "$REPO/frame/linux:/frame:ro" -v "$OUT:/out" \
	"$ALPINE" sh -euc '
	apk add -q build-base clang lld llvm bison flex bc perl findutils linux-headers openssl-dev elfutils-dev python3 xz lz4 lzop patch patchutils >/dev/null
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
		for p in /frame/patches/*.patch; do patch -p1 -s -F0 <"$p"; done
		echo "$sum" >.maison-patches
	fi
	cp /frame/'"$DTB"'.dts arch/arm/boot/dts/rockchip/
	m="make ARCH=arm LLVM=1 KCFLAGS=-mtune=cortex-a7 -j$(nproc)"
	$m KCONFIG_ALLCONFIG=/frame/maison.config allnoconfig >config.log 2>&1
	# the file must have been taken whole: a symbol kconfig dropped is a silent difference
	missing=$(grep -E "^CONFIG_[A-Za-z0-9_]+=" /frame/maison.config | while read -r l; do
		case "$l" in *=n) grep -qx "# ${l%=n} is not set" .config || ! grep -q "^${l%=n}=" .config || echo "$l" ;; *) grep -qx "$l" .config || echo "$l" ;; esac
	done)
	[ -z "$missing" ] || { echo "not taken by kconfig: $missing" >&2; exit 1; }
	# a warning from the build, ours or the kernel'"'"'s, stops it: nothing is left to scroll by
	if ! $m zImage rockchip/'"$DTB"'.dtb >build.log 2>&1; then grep -E "error|Error" build.log | head -20; exit 1; fi
	if grep -E "warning:|Warning" build.log; then echo "warnings in the build" >&2; exit 1; fi
	# U-Boot loads the initramfs at 0x62000000 (the vendor parameter: initrd=0x62000000,…): the
	# decompressed kernel up to its end, and the copy the decompressor makes of itself past it,
	# must end below, with a margin
	offset=$(sed -n "s/^CONFIG_PAGE_OFFSET=//p" .config)
	end=$(( 0x$(grep -E " _end$" System.map | cut -c1-8) - offset + 0x60000000 ))
	zsize=$(stat -c %s arch/arm/boot/zImage)
	limit=$(( 0x62000000 - 0x200000 ))
	[ $(( end + zsize )) -lt $limit ] || { printf "too big: the kernel ends at 0x%x, +%d of zImage, past 0x%x\n" $end $zsize $limit >&2; exit 1; }
	printf "fits: the kernel ends at 0x%x, +%d of zImage, below 0x%x\n" $end $zsize $limit
	cp arch/arm/boot/zImage arch/arm/boot/dts/rockchip/'"$DTB"'.dtb .config /out/
	echo "built $(make -s kernelrelease)"
	# the Wi-Fi driver, out of tree: its patches (debian/patches/series) on a fresh copy, their SDIO
	# part only (the USB and PCIe sources, CRLF, are not built), then its SDIO modules against this
	# kernel (no in-tree module is built: the kernel'"'"'s own symbols are all there is to link against)
	cp vmlinux.symvers Module.symvers
	$m modules_prepare >/dev/null
	rm -rf aic8800 && cp -a /src/aic8800 aic8800 && cd aic8800
	while read -r p; do [ -n "$p" ] && filterdiff -p1 -i "src/SDIO/*" debian/patches/$p | patch -p1 -s; done <debian/patches/series
	for p in /frame/aic8800/*.patch; do patch -p1 -s -F0 <"$p"; done
	drv=$PWD/src/SDIO/driver_fw/driver/aic8800
	cd /build
	if ! $m M=$drv CONFIG_AIC8800_BTLPM_SUPPORT=n modules >aic8800.log 2>&1; then grep -E "error" aic8800.log | head -20; exit 1; fi
	rm -rf /out/aic8800 && mkdir -p /out/aic8800
	for k in aic8800_bsp aic8800_fdrv; do llvm-strip --strip-debug -o /out/aic8800/$k.ko $drv/$k/$k.ko; done
	cp -r aic8800/src/SDIO/driver_fw/fw/aic8800DC /out/aic8800/firmware
	mkdir -p /tmp/regdb && tar xJf /src/wireless-regdb-'"$REGDB"'.tar.xz -C /tmp/regdb --strip-components=1
	cp /tmp/regdb/regulatory.db /tmp/regdb/regulatory.db.p7s /out/
'
ls -la "$OUT"

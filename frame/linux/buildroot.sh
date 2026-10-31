#!/usr/bin/env bash
# The frame's system from Buildroot (buildroot/maison_defconfig): the toolchain and what the app
# links, the app against them, then all of it as a squashfs. Built in an arm64 Debian (the build
# host's architecture), in Docker volumes; the sources are pinned and checked. Out:
# frame/build/buildroot/rootfs.squashfs, the same system frame/linux/install.sh sends.
# Usage: frame/linux/buildroot.sh
# FRAME_TARGET=https://… : Maison's address for a frame with none of its own (/etc/maison-target).
set -euo pipefail
. "$(dirname "$0")/../docker.sh"

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$HOME/maison-frame/src"
OUT="$REPO/frame/build/buildroot"
VERSION=2026.08.1
SHA256=34fb7bea8a213ffde39cc02e61703391560f26e0850f03138c722d0b09880f2b
RUST=$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' "$REPO/frame/ui/Cargo.toml")
TARGET=armv7-unknown-linux-gnueabihf
SSH_CA="$HOME/maison-frame/keys/ssh_ca.pub"
[ -s "$SSH_CA" ] || "$REPO/frame/linux/ssh-cert.sh"
# an image number that only goes up, and where frames look for updates (FRAME_UPDATE_URL= for none)
IMAGE_VERSION=$(date -u +%Y%m%d%H%M)
UPDATE_URL="${FRAME_UPDATE_URL-https://updates.synthe.se/frame}"
ADDRESS="${FRAME_TARGET-}"
case "$ADDRESS" in
"" | https://?*) ;;
*) echo "FRAME_TARGET=$ADDRESS: an https:// address" >&2; exit 1 ;;
esac
[ -n "$RUST" ] || { echo "frame/ui/Cargo.toml says no rust-version" >&2; exit 1; }

tarball="$SRC/buildroot-$VERSION.tar.xz"
[ -f "$tarball" ] || curl -sfL "https://buildroot.org/downloads/buildroot-$VERSION.tar.xz" -o "$tarball"
echo "$SHA256  $tarball" | shasum -a 256 -c - >/dev/null
mkdir -p "$OUT"
docker volume create maison-buildroot >/dev/null
docker volume create maison-frame-ui >/dev/null
docker run --rm --platform linux/arm64 -v maison-buildroot:/build -v maison-frame-ui:/cache -v "$SRC:/src:ro" \
	-v "$REPO:/repo:ro" -v "$REPO/frame:/frame:ro" -v "$OUT:/out" -v "$SSH_CA:/ssh-ca.pub:ro" \
	-e RUST="$RUST" -e TARGET="$TARGET" -e VERSION="$VERSION" -e IMAGE_VERSION="$IMAGE_VERSION" -e UPDATE_URL="$UPDATE_URL" -e ADDRESS="$ADDRESS" debian:trixie sh -euc '
	export DEBIAN_FRONTEND=noninteractive FORCE_UNSAFE_CONFIGURE=1 BR2_DL_DIR=/build/dl
	apt-get -qq update
	apt-get -qq install -y --no-install-recommends build-essential file wget cpio unzip rsync bc python3 perl \
		git curl cmake pkg-config ca-certificates xz-utils bzip2 >/dev/null 2>&1
	cd /build
	# the tree as our patch says: a changed patch starts all over (no stale package left in the image)
	sum=$(cat /frame/linux/buildroot/*.patch /frame/linux/buildroot/maison_defconfig | sha256sum | cut -c1-64)
	if [ "$(cat .maison-tree 2>/dev/null)" != "$sum" ]; then
		rm -rf buildroot-$VERSION out
		tar xJf /src/buildroot-$VERSION.tar.xz
		for p in /frame/linux/buildroot/*.patch; do patch -d buildroot-$VERSION -p1 -s -F0 <"$p"; done
		echo "$sum" >.maison-tree
	fi
	cd buildroot-$VERSION
	m="make O=/build/out"
	$m BR2_DEFCONFIG=/frame/linux/buildroot/maison_defconfig defconfig >/dev/null
	# the file must have been taken whole, as the kernel'"'"'s
	missing=$(grep -E "^BR2_[A-Za-z0-9_]+=" /frame/linux/buildroot/maison_defconfig | while read -r l; do grep -qx "$l" /build/out/.config || echo "$l"; done)
	[ -z "$missing" ] || { echo "not taken by kconfig: $missing" >&2; exit 1; }
	date; $m toolchain libdrm wayland libxkbcommon mesa3d systemd >/build/deps.log 2>&1 || { tail -30 /build/deps.log; exit 1; }
	# the app and the tools, against what the frame runs
	date
	H=/build/out/host; S=$H/arm-buildroot-linux-gnueabihf/sysroot
	export CARGO_HOME=/cache/cargo RUSTUP_HOME=/cache/rustup CARGO_TARGET_DIR=/cache/target-buildroot
	[ -x /cache/cargo/bin/rustup ] || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain none
	export PATH=/cache/cargo/bin:$H/bin:$PATH
	rustup toolchain install $RUST --target $TARGET --profile minimal >/dev/null 2>&1
	export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=/frame/linux/buildroot/link-gcc
	export CC_armv7_unknown_linux_gnueabihf=arm-buildroot-linux-gnueabihf-gcc CXX_armv7_unknown_linux_gnueabihf=arm-buildroot-linux-gnueabihf-g++
	export CMAKE_TOOLCHAIN_FILE_armv7_unknown_linux_gnueabihf=$H/share/buildroot/toolchainfile.cmake
	export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR=$S PKG_CONFIG_LIBDIR=$S/usr/lib/pkgconfig:$S/usr/share/pkgconfig
	cd /repo/frame/ui
	cargo +$RUST build --locked --release --target $TARGET
	cp /cache/target-buildroot/$TARGET/release/maison-frame-ui /tmp/
	cd /repo/frame/tools
	cargo +$RUST build --locked --release --target $TARGET --bin maison-update --bin maison-setup --bin maison-wifid --bin maison-power-key
	for tool in maison-update maison-setup maison-wifid maison-power-key; do cp /cache/target-buildroot/$TARGET/release/$tool /tmp/; done
	echo $IMAGE_VERSION >/tmp/maison-version
	printf "%s" "$UPDATE_URL" >/tmp/maison-update-url
	printf "%s" "$ADDRESS" >/tmp/maison-target
	date; cd /build/buildroot-$VERSION
	$m >/build/make.log 2>&1 || { tail -30 /build/make.log; exit 1; }
	cp /build/out/images/rootfs.squashfs /out/
	cp /build/out/.config /out/buildroot.config
	cp /tmp/maison-version /out/version
	date; du -sh /build/out/target; ls -la /out/rootfs.squashfs
'

#!/usr/bin/env bash
# The frame's native interface, cross-built for its Cortex-A7 (armv7-unknown-linux-gnueabihf) in an
# arm64 Debian trixie (the Mac's architecture: no emulation) against trixie's armhf libraries
# (multiarch), the system it runs on; SDL 3 and SDL3_ttf from their latest sources (the sdl3
# crate's `build-from-source`), with Wayland for cage. The repository is mounted read-only: the
# build reads the API crate, the web app's tokens, icons and fonts, and the words. The cargo
# cache stays in a Docker volume; the binary lands in frame/build/ui.
# Usage: frame/ui/build.sh
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$REPO/frame/build/ui"
RUST=1.99.1
TARGET=armv7-unknown-linux-gnueabihf
VOLUME=maison-frame-ui

mkdir -p "$OUT"
docker volume create "$VOLUME" >/dev/null
# the platform said: the local debian:trixie may be the armhf one rootfs.sh pulled (emulated, slow,
# and rustup would take it for the host)
# the web app's fonts come from its node_modules (Fraunces): installed on the Mac first
[ -f "$REPO/web/node_modules/@fontsource-variable/fraunces/files/fraunces-latin-wght-normal.woff2" ] ||
	{ echo "the web app's fonts are missing: run \`bun install\` in web/" >&2; exit 1; }
docker run --rm --platform linux/arm64 -v "$VOLUME:/cache" -v "$REPO:/repo:ro" -v "$OUT:/out" debian:trixie sh -euc '
	export DEBIAN_FRONTEND=noninteractive CARGO_HOME=/cache/cargo RUSTUP_HOME=/cache/rustup
	dpkg --add-architecture armhf
	apt-get -qq update
	# the cross compiler, and what SDL builds its Wayland, EGL/GLES and input against
	apt-get -qq install -y --no-install-recommends ca-certificates curl cmake make pkg-config gcc libc6-dev \
		gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf libc6-dev:armhf wayland-protocols \
		libwayland-dev:armhf libxkbcommon-dev:armhf libegl-dev:armhf libgles-dev:armhf \
		libdrm-dev:armhf libgbm-dev:armhf libdecor-0-dev:armhf libudev-dev:armhf >/dev/null
	[ -x /cache/cargo/bin/rustup ] || curl -sSf https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain none
	export PATH=/cache/cargo/bin:$PATH
	rustup toolchain install '"$RUST"' --target '"$TARGET"' --profile minimal
	export CARGO_TARGET_DIR=/cache/target
	export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
	export CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc CXX_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++
	export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_LIBDIR=/usr/lib/arm-linux-gnueabihf/pkgconfig:/usr/share/pkgconfig
	cd /repo/frame/ui
	cargo +'"$RUST"' build --locked --release --target '"$TARGET"'
	cp /cache/target/'"$TARGET"'/release/maison-frame-ui /out/
	arm-linux-gnueabihf-readelf -d /out/maison-frame-ui | grep NEEDED
'
ls -la "$OUT"

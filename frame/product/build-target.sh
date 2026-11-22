#!/bin/sh
# Maison's programs for the frame, built against the frame's own libraries inside frame-kernel's
# build (bin/system.sh: the toolchain, cargo and the cross variables are its): the interface and
# the page of the frame's own Wi-Fi, laid out under $1 as the frame's root.
set -eu
out=$1
strip=arm-buildroot-linux-gnueabihf-strip
mkdir -p "$out/usr/local/bin"
for crate in ui tools; do
	cd "/product-repo/frame/$crate"
	rust=$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' Cargo.toml)
	rustup toolchain install "$rust" --target "$TARGET" --profile minimal >/dev/null 2>&1
	cargo "+$rust" build --locked --release --target "$TARGET"
done
for program in maison-frame-ui maison-setup; do
	cp "$CARGO_TARGET_DIR/$TARGET/release/$program" "$out/usr/local/bin/"
	"$strip" "$out/usr/local/bin/$program"
done

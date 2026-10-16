#!/usr/bin/env bash
# The frame's boot animation (bootanimation.ts draws it from the app's own sources), turned and
# zipped as Android 6's bootanimation reads it. At boot, before the window manager applies the
# locked rotation, bootanimation draws in the panel's own orientation (landscape on this frame), so
# the frames are turned as the system will turn everything else: ROTATION is the one provision.sh
# locks (1, the frame standing in portrait), and Surface.ROTATION_<n×90> means the drawing turned
# n×90° clockwise. (A preview after boot proves nothing: the rotation is applied by then.)
# Needs bun, the web app's dependencies (Playwright's Chromium) and sips (macOS).
# Usage: frame/bootanimation.sh <out.zip>
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
ROTATION="${ROTATION:-1}"
ROTATE=$((ROTATION * 90))
PETROL="#0E5E68" # web/src/styles/tokens.css --raw-petrol: what shows between frames
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

bun "$REPO/frame/bootanimation.ts" "$TMP" >/dev/null
[ "$ROTATE" = 0 ] || find "$TMP/part0" "$TMP/part1" -name '*.png' -exec sips -r "$ROTATE" {} \; >/dev/null
W=$(sips -g pixelWidth "$TMP/part1/000.png" | awk '/pixelWidth/ {print $2}')
H=$(sips -g pixelHeight "$TMP/part1/000.png" | awk '/pixelHeight/ {print $2}')
# width height fps; the fade once, then the still frame looped until the system is up
printf '%s %s 12\np 1 0 part0 %s\np 0 0 part1 %s\n' "$W" "$H" "$PETROL" "$PETROL" >"$TMP/desc.txt"
# bootanimation reads the archive stored, never compressed
rm -f "$OUT"
(cd "$TMP" && zip -q -0 -X -r "$OUT" desc.txt part0 part1)
echo "boot animation: $OUT (${W}×${H}, turned $ROTATE°)"

#!/usr/bin/env bash
# Every icon of the web app from its two drawings, said once: web/static/brand.svg (the mark,
# its rounded tile) and web/brand/maskable.svg (the same, full bleed, for launchers that crop
# and for iOS, which rounds the corners itself). Needs rsvg-convert (brew install librsvg).
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
MARK="$REPO/web/static/brand.svg"
MASK="$REPO/web/brand/maskable.svg"
ICONS="$REPO/web/static/icons"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

png() { rsvg-convert -w "$2" -h "$2" "$1" -o "$3"; }

for size in 72 96 128 144 152 192 384 512; do png "$MARK" "$size" "$ICONS/icon-${size}x${size}.png"; done
for size in 192 512; do png "$MASK" "$size" "$ICONS/icon-maskable-${size}x${size}.png"; done
png "$MASK" 180 "$ICONS/apple-touch-icon.png"

# The frame's APK (frame/android): the launcher icon at every density
RES="$REPO/frame/android/app/src/main/res"
for density in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
	mkdir -p "$RES/mipmap-${density%%:*}"
	png "$MARK" "${density##*:}" "$RES/mipmap-${density%%:*}/ic_launcher.png"
done

# favicon.ico: 16, 32 and 48 px PNGs in one ICO (every browser reads PNG entries)
for size in 16 32 48; do png "$MARK" "$size" "$TMP/$size.png"; done
python3 - "$TMP" "$REPO/web/static/favicon.ico" <<'PY'
import struct, sys
tmp, out = sys.argv[1], sys.argv[2]
images = [open(f"{tmp}/{s}.png", "rb").read() for s in (16, 32, 48)]
header = struct.pack("<HHH", 0, 1, len(images))
offset = 6 + 16 * len(images)
entries, data = b"", b""
for size, image in zip((16, 32, 48), images):
    entries += struct.pack("<BBBBHHII", size, size, 0, 0, 1, 32, len(image), offset + len(data))
    data += image
open(out, "wb").write(header + entries + data)
PY
echo "icons: $(ls "$ICONS" | wc -l | tr -d ' ') files + favicon.ico + the frame's launcher icon from brand.svg"

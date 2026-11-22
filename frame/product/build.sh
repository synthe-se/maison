#!/usr/bin/env bash
# What Maison's image takes from this machine (frame-kernel's bin/system.sh calls this): the
# address of the house for a frame that keeps none of its own, when FRAME_TARGET says one.
set -euo pipefail
OUT="${1:?usage: frame/product/build.sh <root>}"
ADDRESS="${FRAME_TARGET-}"
case "$ADDRESS" in
"") ;;
https://?*) mkdir -p "$OUT/etc" && printf '%s' "$ADDRESS" >"$OUT/etc/maison-target" ;;
*) echo "FRAME_TARGET=$ADDRESS: an https:// address" >&2; exit 1 ;;
esac

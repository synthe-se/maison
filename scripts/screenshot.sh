#!/usr/bin/env bash
# The README's screenshot, from an invented house, never the real one: no login to keep
# alive, no profile, the same picture every time. It builds what the e2e harness needs, then
# runs e2e/readme.ts (the e2e house plus every family the dashboard shows, signed in by a
# fresh invitation on a throwaway backend) and writes a JPEG.
#
# Usage:
#   scripts/screenshot.sh                       # screenshots/maison.jpg, README cache-busted
#   scripts/screenshot.sh --out screenshots/x.jpg --no-readme
#
# When the image is referenced in README.md, its URL gets a fresh `?v=<timestamp>` so GitHub
# busts its image cache and shows the new screenshot.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$REPO/screenshots/maison.jpg"
BUMP_README=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"; shift 2 ;;
    --no-readme) BUMP_README=0; shift ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

mkdir -p "$(dirname "$OUT")"
cargo build --quiet --manifest-path "$REPO/backend/Cargo.toml" --bin maison-backend
(cd "$REPO/web" && bun run build >/dev/null)
(cd "$REPO/e2e" && OUT="$OUT" ./run.sh readme.ts)

# Cache-bust: refresh `?v=<timestamp>` on this image's URL in README.md.
README="$REPO/README.md"
REL="${OUT#"$REPO"}"   # e.g. /screenshots/maison.jpg
if [[ "$BUMP_README" == 1 && -f "$README" ]] && grep -q "$REL" "$README"; then
  TS="$(date +%s)"
  perl -i -pe "s{\Q$REL\E(\?v=\d+)?}{$REL?v=$TS}g" "$README"
  echo "Bumped README image cache -> $REL?v=$TS"
fi

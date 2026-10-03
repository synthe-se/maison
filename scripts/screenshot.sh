#!/usr/bin/env bash
# The README's screenshot, from an invented house, never the real one: no login to keep
# alive, no profile, the same picture every time. It builds what the e2e harness needs, then
# runs e2e/readme.ts (the e2e house plus every family the dashboard shows, signed in by a
# fresh invitation on a throwaway backend) and writes a JPEG.
#
# Usage:
#   scripts/screenshot.sh                       # screenshots/maison-<hash>.jpg, in README.md
#   scripts/screenshot.sh --out screenshots/x.jpg --no-readme
#
# The README's picture is named after its content (`maison-<hash>.jpg`), the previous one
# removed: GitHub redirects a README image to raw.githubusercontent.com without its query
# string and caches that address (CDN and browsers), so a `?v=` never reached it; a new name
# is a new address no cache has seen.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""
BUMP_README=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"; shift 2 ;;
    --no-readme) BUMP_README=0; shift ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

SHOTS="$REPO/screenshots"
SHOT="${OUT:-$(mktemp -t maison-shot).jpg}"
mkdir -p "$(dirname "$SHOT")" "$SHOTS"
cargo build --quiet --manifest-path "$REPO/backend/Cargo.toml" --bin maison-backend
(cd "$REPO/web" && bun run build >/dev/null)
(cd "$REPO/e2e" && OUT="$SHOT" ./run.sh readme.ts)
[[ -n "$OUT" ]] && exit 0

# The README's picture: named after its content, the previous ones removed, README pointed at it.
HASH="$(shasum -a 256 "$SHOT" | cut -c1-12)"
NAME="maison-$HASH.jpg"
find "$SHOTS" -maxdepth 1 -name 'maison*.jpg' ! -name "$NAME" -delete
mv "$SHOT" "$SHOTS/$NAME"
README="$REPO/README.md"
if [[ "$BUMP_README" == 1 && -f "$README" ]]; then
  perl -i -pe "s{/screenshots/maison[^)\s]*\.jpg(\?v=\d+)?}{/screenshots/$NAME}g" "$README"
  echo "README picture -> /screenshots/$NAME"
fi

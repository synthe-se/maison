#!/usr/bin/env bash
set -euo pipefail

# Cross-compile cloudflared for Raspberry Pi 1 (ARMv6)
#
# Expects the cloudflared source repo at ../cloudflared relative to this repo,
# or override with CLOUDFLARED_SRC.
#
# The compiled binary lands in this repo at:
#   cloudflared-arm (gitignored)
#
# Usage:
#   ./scripts/build-cloudflared-armv6.sh           # build from ../cloudflared
#   CLOUDFLARED_SRC=/path/to/cloudflared ./scripts/build-cloudflared-armv6.sh

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "${SCRIPT_DIR}")"
CLOUDFLARED_SRC="${CLOUDFLARED_SRC:-${REPO_ROOT}/../cloudflared}"
OUTPUT="${REPO_ROOT}/cloudflared-arm"

if [ ! -d "${CLOUDFLARED_SRC}" ]; then
  printf 'Error: cloudflared source not found at %s\n' "${CLOUDFLARED_SRC}" >&2
  printf 'Clone it with: git clone https://github.com/cloudflare/cloudflared.git %s\n' "${CLOUDFLARED_SRC}" >&2
  exit 1
fi

if ! command -v go >/dev/null 2>&1; then
  printf 'Error: go is not installed. Install Go first.\n' >&2
  exit 1
fi

# The latest release unless SKIP_PULL is set, or the tag CLOUDFLARED_VERSION names: a
# release tag, never whatever the default branch holds (and a checkout parked on an old
# tag would otherwise rebuild that tag forever).
if [ "${SKIP_PULL:-0}" != "1" ]; then
  printf '==> Fetching cloudflared releases\n'
  git -C "${CLOUDFLARED_SRC}" fetch --quiet --tags origin
  TAG="${CLOUDFLARED_VERSION:-$(git -C "${CLOUDFLARED_SRC}" tag --list '20*' --sort=-v:refname | head -n1)}"
  git -C "${CLOUDFLARED_SRC}" checkout --quiet "${TAG}"
fi

# Determine version and commit from git
VERSION="$(git -C "${CLOUDFLARED_SRC}" describe --tags --exact-match 2>/dev/null || git -C "${CLOUDFLARED_SRC}" describe --tags --abbrev=0 2>/dev/null || echo "unknown")"
COMMIT="$(git -C "${CLOUDFLARED_SRC}" rev-parse --short HEAD)"
DATE="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

printf '==> Cross-compiling cloudflared %s (%s) for linux/arm (ARMv6)\n' "${VERSION}" "${COMMIT}"

cd "${CLOUDFLARED_SRC}"

CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=6 \
  go build \
    -ldflags "-X main.Version=${VERSION} -X main.BuildTime=${DATE} -X main.GitCommit=${COMMIT}" \
    -o "${OUTPUT}" \
    ./cmd/cloudflared

printf '==> Built: %s (%s)\n' "${OUTPUT}" "$(du -h "${OUTPUT}" | cut -f1)"
printf '==> Version: %s\n' "${VERSION}"

#!/usr/bin/env bash
# Refresh the Matter device-attestation roots from connectedhomeip's mirror of
# the CSA Distributed Compliance Ledger: the production PAA roots that anchor
# a device's DAC chain, and the CSA keys that sign Certification Declarations.
# Commissioning refuses a device whose chain ends outside these, so rerun this
# when a newly bought device fails attestation.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRUST_DIR="${ROOT_DIR}/matter-trust"
REPO="project-chip/connectedhomeip"
REF="${CHIP_REF:-master}"

fetch_dir() {
  local remote="$1" local_dir="$2"
  rm -rf "${local_dir}"
  mkdir -p "${local_dir}"
  gh api "repos/${REPO}/contents/${remote}?ref=${REF}" --jq '.[] | select(.name | endswith(".der")) | .name' |
    while read -r name; do
      curl -fsSL "https://raw.githubusercontent.com/${REPO}/${REF}/${remote}/${name}" -o "${local_dir}/${name}"
    done
  printf '%s: %s certificates\n' "${local_dir#"${ROOT_DIR}/"}" "$(find "${local_dir}" -name '*.der' | wc -l | tr -d ' ')"
}

fetch_dir credentials/production/paa-root-certs "${TRUST_DIR}/paa"
fetch_dir credentials/production/cd-certs "${TRUST_DIR}/cd"

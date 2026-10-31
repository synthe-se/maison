#!/usr/bin/env bash
# The frames' SSH: a CA on this machine (~/maison-frame/keys, never in the repo; the system holds
# its public half), and a certificate for your key, good for 8 hours. No key is installed on any
# frame. `frame_ssh` from frame-ssh.sh calls this when the certificate has run out.
# Usage: frame/linux/ssh-cert.sh
set -euo pipefail

KEYS="$HOME/maison-frame/keys"
KEY="${FRAME_SSH_KEY:-$HOME/.ssh/id_ed25519.pub}"
[ -s "$KEY" ] || { echo "no $KEY" >&2; exit 1; }
mkdir -p "$KEYS"
[ -s "$KEYS/ssh_ca" ] || ssh-keygen -q -t ed25519 -N "" -C maison-frames-ca -f "$KEYS/ssh_ca"
cp "$KEY" "$KEYS/frame.pub"
ssh-keygen -q -s "$KEYS/ssh_ca" -I "$(whoami)@$(hostname -s)" -n root -V +8h "$KEYS/frame.pub"

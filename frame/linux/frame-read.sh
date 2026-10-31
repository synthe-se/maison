#!/usr/bin/env bash
# What a command prints on the frame, into a local file, over SSH (frame-ssh.sh).
# Usage: frame/linux/frame-read.sh '<command>' <file>
set -euo pipefail
. "$(dirname "$0")/frame-ssh.sh"
frame_ssh "$1" >"$2"

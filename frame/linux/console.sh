#!/usr/bin/env bash
# Runs a command on the frame's USB console (the initramfs's mksh or the system's root bash) and
# prints its output, without the shell's echo or colours. The end is a marker the command's own
# echo cannot match (printf builds it); no marker before the deadline fails the script.
# Usage: frame/linux/console.sh '<command>' [seconds, 30]
set -euo pipefail

CMD="${1:?usage: frame/linux/console.sh '<command>' [seconds]}"
SECS="${2:-30}"
dev=$(compgen -G '/dev/cu.usbmodem*' | head -1) || { echo "no frame console" >&2; exit 1; }
out=$(mktemp)
trap 'kill "$reader" 2>/dev/null || true; rm -f "$out"' EXIT
stty -f "$dev" raw -echo 115200
# the console is open for reading before the command goes: what the frame answers waits in the
# tty for the reader, however late it starts
exec 3<"$dev"
cat <&3 >"$out" &
reader=$!
printf 'stty -echo; export TERM=dumb SYSTEMD_COLORS=0 SYSTEMD_PAGER=; printf "__%%s__\\n" BEGIN; %s; printf "__%%s__\\n" END\r' "$CMD" >"$dev"
for ((i = 0; i < SECS * 4; i++)); do
	grep -q '^__END__' <(tr -d '\r' <"$out") && break
	sleep 0.25
done
grep -q '^__END__' <(tr -d '\r' <"$out") || { echo "timeout: no end from the frame after ${SECS}s" >&2; exit 1; }
tr -d '\r' <"$out" | sed 's/\x1b\[[0-9;?]*[a-zA-Z]//g' | awk '/^__BEGIN__/{f=1;next} /^__END__/{f=0} f'

#!/usr/bin/env bash
# Runs a command on the frame as root and prints its output: over its USB console when the
# USB-C is on this machine (a --debug image's mksh, without the shell's echo
# or colours; the end is a marker the command's own echo cannot match), else over SSH
# (root@maison-frame.local by a certificate, frame-ssh.sh; FRAME_HOST=<address> without mDNS). No answer
# before the deadline fails the script.
# Usage: frame/linux/console.sh '<command>' [seconds, 30]
set -euo pipefail

CMD="${1:?usage: frame/linux/console.sh '<command>' [seconds]}"
SECS="${2:-30}"
. "$(dirname "$0")/frame-ssh.sh"
# the frame's own port, by the name its gadget says (initramfs.py), never any other USB serial device
dev=$(ioreg -r -c IOUSBHostDevice -l -w0 2>/dev/null |
	LC_ALL=C awk -F'"' -v want='Maison frame' '/"USB Product Name"/ { product = $4 } /"IOCalloutDevice"/ && product == want { print $4; exit }')
if [ -z "$dev" ]; then
	# no cable: the system's SSH (the initramfs has none)
	frame_ready
	timeout "$SECS" ssh "${frame_options[@]}" "root@$FRAME" "$CMD" || {
		rc=$?
		[ $rc = 124 ] && echo "timeout: no end from the frame after ${SECS}s" >&2 || echo "no frame console nor SSH (rc $rc)" >&2
		exit 1
	}
	exit 0
fi
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

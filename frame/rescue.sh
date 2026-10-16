#!/usr/bin/env bash
# Root on the frame without a button: into the rescue recovery (build.py's recovery.img), whose
# init runs each command written to /dev/rootsh/in as root and answers on /dev/rootsh/out (only
# ADB's user may open them, so only this computer's key). From there: dd any partition, fix
# /data, read logs. One command per call (no state between them: say `cd /x && …`); without
# one, a line-by-line prompt. `exit` there, then `adb -s <serial> reboot` brings Maison back
# (`reboot loader`: the loader).
# Every wait has a deadline: the recovery 150 s, its root service 60 s once there.
# Usage: frame/rescue.sh [command]    ANDROID_SERIAL: the frame on Android (the recovery says
#                                      another serial, found by its state)
set -euo pipefail

command -v timeout >/dev/null || { echo "timeout is needed (coreutils: brew install coreutils)" >&2; exit 1; }
in_recovery() { adb devices | awk '$2 == "recovery" {print $1; exit}'; }

if [ -z "$(in_recovery)" ]; then
	adb reboot recovery
	# any device in recovery: its serial is not the one ANDROID_SERIAL names on Android
	env -u ANDROID_SERIAL timeout 150 adb wait-for-recovery || { echo "the frame did not come up in recovery within 150 s" >&2; exit 1; }
fi
serial=$(in_recovery)
[ -n "$serial" ] || { echo "the frame did not come up in recovery" >&2; exit 1; }
# the command goes in base64: no quoting survives two shells intact. Android 6's adb shell says
# no exit status: a root service that never comes up is said by a marker, turned into a failure
root() {
	adb -s "$serial" shell "i=0; until [ -p /dev/rootsh/in ]; do i=\$((i + 1)); [ \$i -le 60 ] || { echo __no_root__; exit; }; sleep 1; done
		echo $(printf '%s' "$1" | base64) | base64 -d >/dev/rootsh/in && cat /dev/rootsh/out" | tr -d '\r' |
		awk '$0 == "__no_root__" { print "the recovery'"'"'s root service never came up (60 s)" >"/dev/stderr"; bad = 1; next } { print } END { exit bad }'
}
if [ $# -gt 0 ]; then
	root "$*"
else
	echo "root on the frame, one command a line (exit, then: adb -s $serial reboot)"
	while IFS= read -r -p "# " line && [ "$line" != exit ]; do root "$line"; done
fi

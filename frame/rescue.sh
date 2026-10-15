#!/usr/bin/env bash
# Root on the frame without a button: into the rescue recovery (build.py's recovery.img), whose
# init runs each command written to /dev/rootsh/in as root and answers on /dev/rootsh/out (only
# ADB's user may open them, so only this computer's key). From there: dd any partition, fix
# /data, read logs. One command per call (no state between them: say `cd /x && …`); without
# one, a line-by-line prompt. `exit` there, then `adb -s <serial> reboot` brings Maison back
# (`reboot loader`: the loader).
# Usage: frame/rescue.sh [command]    ANDROID_SERIAL: the frame on Android (the recovery says
#                                      another serial, found by its state)
set -euo pipefail

in_recovery() { adb devices | awk '$2 == "recovery" {print $1; exit}'; }

if [ -z "$(in_recovery)" ]; then
	adb reboot recovery
	for _ in $(seq 75); do [ -n "$(in_recovery)" ] && break; sleep 2; done
fi
serial=$(in_recovery)
[ -n "$serial" ] || { echo "the frame did not come up in recovery" >&2; exit 1; }
# the command goes in base64: no quoting survives two shells intact
root() {
	adb -s "$serial" shell "until [ -p /dev/rootsh/in ]; do sleep 1; done
		echo $(printf '%s' "$1" | base64) | base64 -d >/dev/rootsh/in && cat /dev/rootsh/out" | tr -d '\r'
}
if [ $# -gt 0 ]; then
	root "$*"
else
	echo "root on the frame, one command a line (exit, then: adb -s $serial reboot)"
	while IFS= read -r -p "# " line && [ "$line" != exit ]; do root "$line"; done
fi

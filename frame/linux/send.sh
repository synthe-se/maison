#!/usr/bin/env bash
# A file from this machine to the frame over Wi-Fi, from Linux up: scp (key only, the host key
# checked), then the frame checks the SHA-256 said over the console and only then renames it into
# place (mode 0600). Fails loudly on any step: nothing half written replaces
# anything. install.sh (the system) and try.sh --flash (the boot image) send through it.
# Usage: frame/linux/send.sh <local file> <path on the frame>
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="${1:?usage: send.sh <local file> <path on the frame>}"
DST="${2:?usage: send.sh <local file> <path on the frame>}"
c() { "$REPO/frame/linux/console.sh" "$@"; }

[ -f "$SRC" ] || { echo "send: no $SRC" >&2; exit 1; }
c true 10 >/dev/null || { echo "send: the frame answers neither on its console nor over SSH (Linux up?)" >&2; exit 1; }
sum=$(shasum -a 256 "$SRC" | cut -c1-64)
ip=$(c "ip -4 -o addr show wlan0 | sed -E 's/.* inet ([0-9.]+).*/\1/'" 20)
case "$ip" in *.*.*.*) ;; *) echo "send: the frame says no Wi-Fi address: $ip" >&2; exit 1 ;; esac
echo "send: $(basename "$SRC") to $ip"
scp -q -o BatchMode=yes -o ConnectTimeout=10 -o HostKeyAlias=maison-frame.local "$SRC" "root@$ip:$DST.new" || {
	echo "send: the copy to $ip failed (scp: $?)" >&2
	exit 1
}
there=$(c "sync; sha256sum $DST.new | cut -c1-64" 120)
if [ "$there" != "$sum" ]; then
	c "rm -f $DST.new" 20
	echo "send: the copy on the frame differs ($there for $sum): nothing replaced" >&2
	exit 1
fi
c "chmod 600 $DST.new && mv $DST.new $DST && sync && echo in place" 30
echo "send: $DST ($sum)"

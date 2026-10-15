#!/usr/bin/env bash
# Once after a flash (and after any wipe): what a wall screen wants, set over ADB. Animations off
# (Gecko then reports prefers-reduced-motion, which Maison honours), the rotation locked, the kiosk
# made device owner (no status bar, no keyguard), then Maison's address and the screen's service
# token (`maison-backend token create <name>` on the Pi). The token is read from the terminal,
# never an argument: it stays out of the shell's history.
# Usage: frame/provision.sh https://home.example     ROTATION=1 (portrait, as the frame stands)
set -euo pipefail

URL="${1:?usage: frame/provision.sh <Maison URL>}"
ROTATION="${ROTATION:-1}"
SERIAL="${ANDROID_SERIAL:-}"
a() { adb ${SERIAL:+-s "$SERIAL"} "$@"; }

a wait-for-device
for scale in animator_duration_scale transition_animation_scale window_animation_scale; do
	a shell settings put global "$scale" 0
done
a shell settings put system accelerometer_rotation 0
a shell settings put system user_rotation "$ROTATION"

# a device owner is set once, on a frame with no account: « already » is fine, anything else is said
owner=$(a shell dpm set-device-owner maison.frame/.Admin 2>&1 | tr -d '\r')
case "$owner" in
*Success* | *already*) echo "device owner: maison.frame" ;;
*) echo "device owner refused: $owner" >&2; exit 1 ;;
esac

read -rs -p "Service token (maison_…): " TOKEN
echo
a shell am broadcast -a maison.frame.PROVISION -n maison.frame/.ProvisionReceiver --es url "$URL" --es token "$TOKEN" |
	grep -q "result=-1" || { echo "provisioning refused (an HTTPS URL and a maison_ token)" >&2; exit 1; }
echo "provisioned: $URL"

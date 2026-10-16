#!/usr/bin/env bash
# The armv7 binary itself (frame/build/ui, from build.sh) on the frame's system, emulated: Debian
# trixie armhf in Docker, under cage with a headless output of the panel's 800×1280, its GLES 2
# from Mesa's llvmpipe. The same Wayland, EGL, renderer, fonts and HTTP path as on the frame;
# only the GPU differs. stdin and stdout are the app's (sim.rs): e2e/frame.ts drives it
# (`FRAME_SIM=armv7`). The environment's MAISON_* reach the app (and MAISON_ALLOW_HTTP=1: the host
# is host.docker.internal over http, which the frame itself refuses); its state stays in the
# container; shots land in e2e/shots (mounted at the same path).
# The frame's own launcher runs it (rootfs/usr/local/sbin/maison-ui): the output turned by the
# compositor (MAISON_TURN, 180 as on the frame), else by the app.
# Usage: MAISON_TARGET=http://host.docker.internal:PORT frame/ui/simulate.sh
set -euo pipefail
. "$(dirname "$0")/../docker.sh"

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$REPO/frame/build/ui/maison-frame-ui"
IMAGE=maison-frame-sim
[ -x "$BIN" ] || { echo "no armv7 binary: run frame/ui/build.sh" >&2; exit 1; }
# built once, quietly (its output would mix with the app's)
docker image inspect "$IMAGE" >/dev/null 2>&1 ||
	docker build -q -t "$IMAGE" "$REPO/frame/ui/sim" >&2
mkdir -p "$REPO/e2e/shots"
LAUNCHER="$REPO/frame/linux/rootfs/usr/local/sbin/maison-ui"
# the image is armhf alone (its --platform said again here, Docker no longer finds it locally)
exec docker run --rm -i \
	-v "$BIN:/usr/local/bin/maison-frame-ui:ro" -v "$LAUNCHER:/usr/local/sbin/maison-ui:ro" \
	-v "$REPO/e2e/shots:$REPO/e2e/shots" \
	-e MAISON_TARGET -e MAISON_LANG -e MAISON_STDIN -e MAISON_STATS -e MAISON_ROTATE -e MAISON_SCALE \
	-e MAISON_ALLOW_HTTP=1 \
	-e MAISON_TURN="${MAISON_TURN:-180}" \
	"$IMAGE" sh -euc '
		export XDG_RUNTIME_DIR=/tmp/xdg MAISON_STATE=/tmp/state WLR_BACKENDS=headless WLR_RENDERER=pixman \
			WLR_LIBINPUT_NO_DEVICES=1 LIBGL_ALWAYS_SOFTWARE=1 SDL_VIDEO_DRIVER=wayland
		mkdir -m 700 -p "$XDG_RUNTIME_DIR"
		# the panel'"'"'s mode (a DSI panel has its own), then the frame'"'"'s launcher
		exec cage -- sh -c "wlr-randr --output HEADLESS-1 --custom-mode 800x1280 >&2 && exec /usr/local/sbin/maison-ui"
	'

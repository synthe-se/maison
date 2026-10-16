# The frame's system for a simulation: Debian trixie armhf (emulated on the Mac), the packages
# rootfs.sh installs for the native app (cage, wlr-randr, the Wayland and EGL libraries, Noto),
# and Mesa's software GLES (llvmpipe) in place of Lima.
FROM --platform=linux/arm/v7 debian:trixie
RUN apt-get -qq update && DEBIAN_FRONTEND=noninteractive apt-get -qq install -y --no-install-recommends \
	cage wlr-randr ca-certificates fonts-noto-core libwayland-client0 libwayland-egl1 libwayland-cursor0 \
	libxkbcommon0 libegl1 libgles2 libegl-mesa0 libgl1-mesa-dri >/dev/null && rm -rf /var/lib/apt/lists/*

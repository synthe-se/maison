#!/usr/bin/env bash
# The frame's Linux system: Debian trixie armhf from one snapshot of the archive (every package at
# the version it had that day), systemd, the Wi-Fi tools, cog (WPE WebKit) for Maison, and our
# files (frame/linux/rootfs) over it; packed read-only as squashfs (zstd: fast to read on the
# Cortex-A7). The initramfs mounts it from Android's userdata with a tmpfs over it: a power cut
# (no battery) never leaves it half written. Built in an armhf container (emulated on the Mac).
# Usage: frame/linux/rootfs.sh        out: frame/build/linux/rootfs.squashfs
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$REPO/frame/build/linux"
SNAPSHOT=20261015T000000Z
MIRROR="http://snapshot.debian.org/archive/debian/$SNAPSHOT"
PACKAGES=(
	systemd systemd-sysv systemd-timesyncd systemd-resolved udev dbus kmod iproute2 # the system, its network, DNS, NTP
	wpasupplicant iw                                               # the Wi-Fi
	cog libgl1-mesa-dri libegl-mesa0 libgles2 libgbm1              # the browser on KMS, Lima
	ca-certificates fonts-noto-core                                # HTTPS, the page's fallback fonts
	locales                                                        # fr_FR.UTF-8: WebKit reads its language with setlocale()
	cage libpam-systemd wlr-randr kbd                              # the native interface's spike (frame/ui): its compositor, its session, chvt
	libwayland-client0 libwayland-egl1 libwayland-cursor0 libxkbcommon0 libegl1   # what its SDL opens at run time
)

mkdir -p "$OUT"
mkdir -p "$REPO/frame/build/ui"
docker run --rm --platform linux/arm/v7 -v "$REPO/frame/linux/rootfs:/overlay:ro" -v "$OUT:/out" \
	-v "$REPO/frame/build/ui:/ui:ro" \
	debian:trixie sh -euc '
	echo "deb [check-valid-until=no] '"$MIRROR"' trixie main" >/etc/apt/sources.list
	rm -f /etc/apt/sources.list.d/*
	apt-get -qq update && apt-get -qq install -y --no-install-recommends mmdebstrap squashfs-tools >/dev/null
	rm -rf /tmp/root
	mmdebstrap --quiet --variant=minbase --include='"$(IFS=,; echo "${PACKAGES[*]}")"' \
		--aptopt="Acquire::Check-Valid-Until \"false\"" --dpkgopt=path-exclude=/usr/share/doc/* \
		--dpkgopt=path-exclude=/usr/share/man/* --dpkgopt=path-exclude=/usr/share/locale/* --dpkgopt=path-include=/usr/share/locale/locale.alias \
		trixie /tmp/root "deb '"$MIRROR"' trixie main"
	cp -a /overlay/. /tmp/root/
	# the native interface, when frame/ui/build.sh made it
	[ ! -f /ui/maison-frame-ui ] || install -m 0755 /ui/maison-frame-ui /tmp/root/usr/local/bin/
	chroot /tmp/root sh -euc "
		echo maison-frame >/etc/hostname
		ln -sf /usr/share/zoneinfo/Europe/Paris /etc/localtime
		ln -sf ../run/systemd/resolve/stub-resolv.conf /etc/resolv.conf
		echo \"fr_FR.UTF-8 UTF-8\" >/etc/locale.gen && locale-gen >/dev/null
		echo LANG=fr_FR.UTF-8 >/etc/default/locale
		useradd --system --create-home --groups video,render,input kiosk
		# root logs in on the bring-up console only (autologin, gated by a file: the getty drop-in), never by a password: none is set, the account is locked
		passwd -l root
		systemctl enable systemd-networkd systemd-resolved systemd-timesyncd wpa_supplicant@wlan0 maison-kiosk serial-getty@ttyGS0
		systemctl mask systemd-firstboot
	"
	rm -rf /tmp/root/var/cache/apt/* /tmp/root/var/lib/apt/lists/*
	mksquashfs /tmp/root /out/rootfs.squashfs -comp zstd -noappend -quiet
	ls -la /out/rootfs.squashfs
'

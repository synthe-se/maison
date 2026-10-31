#!/bin/sh
# What the overlay cannot say: the programs, who may come in, the journal kept on userdata, the
# names OpenSSH uses, the units enabled. $1 is the target's root; buildroot.sh leaves what it built
# in /tmp and mounts the CA's public key.
set -eu
root=$1
install -D -m 0755 /tmp/maison-frame-ui "$root/usr/local/bin/maison-frame-ui"
"$HOST_DIR/bin/arm-buildroot-linux-gnueabihf-strip" "$root/usr/local/bin/maison-frame-ui"
install -D -m 0755 /tmp/maison-update "$root/usr/local/bin/maison-update"
for tool in maison-setup maison-wifid maison-power-key; do
	install -D -m 0755 "/tmp/$tool" "$root/usr/local/bin/$tool"
done
"$HOST_DIR/bin/arm-buildroot-linux-gnueabihf-strip" "$root/usr/local/bin/maison-update" "$root/usr/local/bin/maison-setup" \
	"$root/usr/local/bin/maison-wifid" "$root/usr/local/bin/maison-power-key"
# who may come in: the CA's certificates, no key on the frame (frame/linux/ssh-cert.sh)
install -m 0644 /ssh-ca.pub "$root/etc/ssh/maison-ca.pub"
# what this image is, and where its updates live (none without FRAME_UPDATE_URL)
cp /tmp/maison-version "$root/etc/maison-version"
if [ -s /tmp/maison-update-url ]; then cp /tmp/maison-update-url "$root/etc/maison-update"; fi
# Maison's address for a frame that has none of its own (FRAME_TARGET; maison-wifi reads it)
if [ -s /tmp/maison-target ]; then cp /tmp/maison-target "$root/etc/maison-target"; fi
# one known board: no table of the world's hardware (the option alone leaves what systemd installed)
rm -f "$root/usr/lib/udev/hwdb.bin"
rm -rf "$root/usr/lib/udev/hwdb.d"
# the initramfs gives a root that writes (a tmpfs over the system): nothing remounts it
sed -i '\|^/dev/root|d' "$root/etc/fstab"
# the kernel has no SysRq to set
sed -i '/kernel.sysrq/d' "$root/usr/lib/sysctl.d/50-default.conf"
# systemd already starts dbus-daemon as its user: asked again, it fails to drop groups and warns
sed -i '/<user>dbus<\/user>/d' "$root/usr/share/dbus-1/system.conf"
# the journal on userdata, kept across power cuts (maison-timesync.service makes the directory)
mkdir -p "$root/var/log"
rm -rf "$root/var/log/journal"
ln -s /android-data/maison/journal "$root/var/log/journal"
ln -sf /dev/null "$root/etc/tmpfiles.d/journal-nocow.conf"
# OpenSSH's own unit and configuration: ours in a drop-in, the host keys made on the frame
rm -rf "$root/etc/systemd/system/sshd.service.d"
mv "$root/etc/systemd/system/ssh.service.d" "$root/etc/systemd/system/sshd.service.d"
grep -q '^Include /etc/ssh/sshd_config.d/' "$root/etc/ssh/sshd_config" ||
	sed -i '1i Include /etc/ssh/sshd_config.d/*.conf' "$root/etc/ssh/sshd_config"
rm -f "$root"/etc/ssh/ssh_host_*
"$HOST_DIR/bin/systemctl" --root="$root" set-default graphical.target
"$HOST_DIR/bin/systemctl" --root="$root" mask systemd-firstboot
# what runs is said as a preset: Buildroot's `preset-all` comes after this script and would undo an
# `enable`, its own preset disabling wpa_supplicant@ (the generic unit wants a D-Bus we do not build)
cat >"$root/usr/lib/systemd/system-preset/10-maison.preset" <<PRESET
enable systemd-networkd.service
enable systemd-resolved.service
enable systemd-timesyncd.service
enable sshd.service
enable maison-ui.service
enable maison-mac.service
enable maison-wifi-watch.service
enable maison-wifid.service
enable maison-power-key.service
enable wpa_supplicant@.service wlan0
enable maison-commit.service
enable maison-update.timer
disable wpa_supplicant.service
# no login prompt on a product, no network file systems, no firmware crash store on this board
disable getty@.service
disable serial-getty@.service
disable remote-fs.target
disable systemd-pstore.service
PRESET
# the clock's floor without its file on userdata: never before this image was made
touch "$root/usr/lib/clock-epoch"

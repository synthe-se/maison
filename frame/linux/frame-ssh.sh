# Sourced by the frame's tools: ssh and scp with a live certificate (ssh-cert.sh), the host key
# checked as the frame's whatever address it answers on.
FRAME="${FRAME_HOST:-maison-frame.local}"
frame_cert="$HOME/maison-frame/keys/frame-cert.pub"
frame_ready() {
	local left
	left=$(ssh-keygen -L -f "$frame_cert" 2>/dev/null | sed -n 's/.*Valid: from .* to //p')
	if [ -z "$left" ] || [ "$(date +%Y-%m-%dT%H:%M:%S)" \> "$left" ]; then
		"$(dirname "${BASH_SOURCE[0]}")/ssh-cert.sh"
	fi
}
frame_options=(-o BatchMode=yes -o ConnectTimeout=5 -o HostKeyAlias=maison-frame.local -o "CertificateFile=$frame_cert")
frame_ssh() { frame_ready; ssh "${frame_options[@]}" "root@$FRAME" "$@"; }
frame_scp() { frame_ready; scp -q "${frame_options[@]}" "$@"; }

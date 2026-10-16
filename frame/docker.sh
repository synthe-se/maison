# Sourced by the frame's Docker builds: Docker must answer before anything waits on it (a hung
# Docker Desktop makes every docker command wait forever, silently).
docker_answers() {
	docker info >/dev/null 2>&1 &
	local probe=$! waited=0
	while kill -0 "$probe" 2>/dev/null; do
		if [ "$waited" -ge 30 ]; then
			kill "$probe" 2>/dev/null || true
			echo "docker does not answer within 15 s: restart Docker Desktop" >&2
			exit 1
		fi
		sleep 0.5
		waited=$((waited + 1))
	done
	wait "$probe" || { echo "docker is not running: start Docker Desktop" >&2; exit 1; }
}
docker_answers

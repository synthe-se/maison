.PHONY: help backend web web-build test deny deploy deploy-push deploy-start deploy-stop deploy-status deploy-logs

LOG_TARGET ?= stack

help:
	@printf "Development (this machine):\n"
	@printf "  make backend             Run the Rust backend in foreground (cargo run)\n"
	@printf "  make web                 Run the SvelteKit dev server\n"
	@printf "  make web-build           Build the web bundle into web/build\n"
	@printf "  make test                fmt, clippy and tests of the four crates, web checks and tests\n"
	@printf "  make deny                cargo deny on the backend (licenses, bans, sources, advisories)\n"
	@printf "\nThe host (deploy.env, docs/deploy.md):\n"
	@printf "  make deploy              Build, push, restart\n"
	@printf "  make deploy-push         Push already-built artifacts and settings\n"
	@printf "  make deploy-start        Restart the stack and wait for /health\n"
	@printf "  make deploy-stop         Stop the stack until the next start\n"
	@printf "  make deploy-status       The host's account of its services\n"
	@printf "  make deploy-logs         Follow logs (LOG_TARGET=stack|backend|mosquitto)\n"

backend:
	cargo run --manifest-path backend/Cargo.toml

web:
	bun run --cwd web dev

web-build:
	bun run --cwd web build

# The four crates (each its own Cargo.lock): fmt, clippy (the backend without Bluetooth too, as
# deployed), tests (api's keeps web/src/lib/paces.ts, codes.ts and events.ts in step), then the
# web app.
CRATES := api backend frame/ui frame/tools

test:
	for crate in $(CRATES); do cargo fmt --manifest-path $$crate/Cargo.toml --check || exit 1; done
	for crate in $(CRATES); do cargo clippy --manifest-path $$crate/Cargo.toml --all-targets -- -D warnings || exit 1; done
	cargo clippy --manifest-path backend/Cargo.toml --all-targets --no-default-features -- -D warnings
	for crate in $(CRATES); do cargo test --manifest-path $$crate/Cargo.toml || exit 1; done
	bun run --cwd web check
	bun run --cwd web test

# What the backend depends on (backend/deny.toml), for a Linux host and for this machine.
deny:
	cargo deny --manifest-path backend/Cargo.toml -t arm-unknown-linux-musleabihf check
	cargo deny --manifest-path backend/Cargo.toml -t aarch64-apple-darwin check

deploy:
	./deploy.sh all

deploy-push:
	./deploy.sh push

deploy-start:
	./deploy.sh start

deploy-stop:
	./deploy.sh stop

deploy-status:
	./deploy.sh status

deploy-logs:
	./deploy.sh logs $(LOG_TARGET)

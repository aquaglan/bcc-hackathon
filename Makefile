# One gate for the whole repository: `make check` must pass before every commit.

.PHONY: check check-rust check-py check-web smoke fmt server harness web review

check: check-rust check-py check-web smoke

check-rust:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

check-py:
	cd harness && uv run ruff format --check . ../scripts && uv run ruff check . ../scripts && uv run pytest -q

check-web:
	cd web && npm install --no-audit --no-fund --silent && npm run check && npm run build

# The only test that crosses the Rust/Python boundary; uses a fake model, no keys.
smoke:
	cargo build -p anyq-server
	python3 scripts/smoke.py

fmt:
	cargo fmt
	cd harness && uv run ruff format . ../scripts && uv run ruff check --fix . ../scripts

server:
	cargo run --release -p anyq-server

# The web client on :5173; it talks to `make server` on :8080.
web:
	cd web && npm install --no-audit --no-fund --silent && npm run dev

harness:
	cd harness && uv run uvicorn anyq_harness.app:app --port 8090

# One run from a requirements file to a report: make review IN=spec.pdf OUT=report.md
review:
	cargo run --release -q -p anyq-server -- review "$(IN)" "$(OUT)"

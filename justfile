# parastats justfile — cross-cutting tasks. Run `just --list` to see all.

default:
    @just --list

# ---- Environment ----

# Enter the nix dev shell explicitly (normally direnv handles this).
shell:
    nix develop

# ---- Rust ----

# Format everything (rust + web).
fmt:
    cargo fmt
    @command -v nixpkgs-fmt >/dev/null && nixpkgs-fmt flake.nix || true

# Check formatting without writing.
fmt-check:
    cargo fmt --check

# Lint (clippy with -D warnings across all targets).
lint:
    cargo clippy --all-targets -- -D warnings

# Run all tests.
test:
    cargo test

# Accept all pending insta snapshots without review. Use sparingly.
snap-accept:
    cargo insta accept

# ---- Ingest / serve ----

# Ingest a folder of .igc files into the default db (./parastats.db).
ingest folder='flights':
    cargo run --quiet -- ingest {{folder}}

# Run the backend only (migrations run on start; serves API + web/dist).
dev-server:
    cargo run -- serve

# ---- Web ----

# Install/reinstall web deps (after pulling changes that touched package.json).
install-web:
    pnpm -C web install

# Start the Vite dev server on :5173 (proxies /api to :8080).
dev-web:
    pnpm -C web dev

# Build the web bundle for production (output: web/dist/, served by axum).
build-web:
    pnpm -C web build

# Type-check the web sources.
check-web:
    pnpm -C web check

# ---- Full stack ----

# One-shot dev: build web once, then run backend + vite dev server together.
# Open http://localhost:5173.
dev:
    #!/usr/bin/env bash
    set -e
    just build-web
    echo "backend on :8080, vite on :5173 — open http://localhost:5173"
    cargo run -- serve > /tmp/parastats-serve.log 2>&1 &
    SERVER_PID=$!
    trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
    sleep 1
    pnpm -C web dev

# Release build: rust binary + web bundle (embedded at compile time).
build:
    just build-web
    cargo build --release

# ---- Docker ----

# Build the docker image.
docker:
    docker compose build

# Build + run (app on :8080, db in named volume, ./flights mounted read-only
# and ingested on start).
docker-up:
    docker compose up --build

# ---- CI helpers ----

# Everything CI runs, locally.
ci:
    just fmt-check
    just lint
    just test
    just build-web
    just check-web

# Verify the flake builds cleanly.
flake-check:
    nix flake check --no-build

# Wipe build artefacts.
clean:
    cargo clean
    rm -rf web/dist web/node_modules

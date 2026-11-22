# parastats justfile — the handful of commands you actually need.
# (Everything else is a raw cargo/pnpm/docker command away.)

default:
    @just --list

# Run the app for development: installs web deps if needed, builds the web
# bundle once, starts the backend on :8080 and the vite dev server on :5173
# (proxies /api). Open http://localhost:5173.
dev:
    #!/usr/bin/env bash
    set -e
    pnpm -C web install
    pnpm -C web build
    echo "backend on :8080, vite on :5173 — open http://localhost:5173"
    cargo run -- serve > /tmp/parastats-serve.log 2>&1 &
    SERVER_PID=$!
    trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
    sleep 1
    pnpm -C web dev

# Ingest a folder of .igc files (recursive; idempotent via content hashes).
ingest folder:
    cargo run --quiet -- ingest {{folder}}

# Release build: web bundle + rust binary (bundle embedded at compile time).
build:
    pnpm -C web install
    pnpm -C web build
    cargo build --release

# Run all tests.
test:
    cargo test

# Clippy gate (what CI enforces).
lint:
    cargo clippy --all-targets -- -D warnings

# Everything CI runs, locally.
# Web install+build must come first: rust-embed needs web/dist/ to compile.
ci:
    pnpm -C web install
    pnpm -C web build
    cargo fmt --check
    just lint
    just test
    pnpm -C web check

# Enable the versioned git hooks (.githooks/) for this checkout — run once
# per clone. pre-commit checks formatting, pre-push runs `just ci`.
hooks:
    git config core.hooksPath .githooks

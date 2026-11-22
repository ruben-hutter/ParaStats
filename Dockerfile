# Multi-stage build: pnpm web build → cargo release → slim runtime.
#
# The web bundle is embedded into the binary at compile time (rust-embed), so
# the runtime image carries exactly one artefact: /usr/local/bin/parastats.
# SQLite is bundled (libsqlite3-sys) — no runtime DB dependencies.

# ---- 1. Web build (must precede the rust build: rust-embed needs web/dist) ----
FROM node:22-bookworm-slim AS web-build
WORKDIR /web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# ---- 2. Rust build ----
FROM rust:1-bookworm AS rust-build
WORKDIR /build
# Cache deps across flights-only changes: a stub build populates the cargo
# cache before the real sources arrive.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && cargo build --release && rm -rf src
# rust-embed embeds web/dist at compile time — bring in the real bundle.
COPY --from=web-build /web/dist web/dist
COPY src/ src/
COPY migrations/ migrations/
RUN touch src/main.rs src/lib.rs && cargo build --release --bin parastats

# ---- 3. Runtime ----
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=rust-build /build/target/release/parastats /usr/local/bin/parastats
ENV PARASTATS_ADDR=0.0.0.0:8080 \
    PARASTATS_DB=/data/db/parastats.db
VOLUME /data/db
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/parastats"]
CMD ["serve"]

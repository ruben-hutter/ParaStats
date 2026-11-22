# ADR-0001: Rust + axum + SQLite backend, Svelte 5 SPA frontend

- Status: Accepted
- Date: 2026-11-22
- Context: ParaStats task `feat/web-app` (turn the Python CLI scripts into a
  self-hosted web app and absorb the flightmap repo).

## Decision

- **Backend: single Rust crate `parastats`** — library (igc parsing, stats,
  climb detection, simplification, Mercator binning) + one binary with
  `serve` / `ingest` subcommands.
  - HTTP via **axum**.
  - Storage in **SQLite via sqlx**, runtime-checked queries, migrations
    embedded via `sqlx::migrate!` and run on startup.
- **Frontend: Svelte 5 + Vite + TypeScript SPA** in `web/`, rendered with
  **MapLibre GL + deck.gl** (`MapboxOverlay` adapter), basemap OpenFreeMap
  Liberty (free, no API key). No SSR; axum serves `web/dist` statically.
- **Web bundle embedded into the binary** with rust-embed (release embeds,
  debug reads `web/dist` from disk).
- **Times UTC everywhere**, display timezone Europe/Zurich converted
  client-side via `Intl`.

## Rationale

- Reuse, don't re-prove: flightmap's core (IGC parser verified against 245
  real flights, climb detection, Douglas–Peucker simplification, Mercator
  binning) is ported nearly verbatim. Porting that logic to TS/Python would
  re-test proven code for zero gain; flightmap's own plan had already chosen
  axum for its server phase.
- SQLite over Postgres: single pilot, single file, trivial volume mount +
  backup. sqlx with runtime-checked queries (not macros) keeps `cargo build`
  hermetic — no build-time DATABASE_URL against a migrated dev database.
- deck.gl + MapLibre over Leaflet: flightmap had already proven the
  `MapboxOverlay` integration and the layer patterns (skyway PathLayer with
  altitude-band colours, density-canvas BitmapLayer, kk7 TMS overlay).
- Svelte over React: tiny bundle, runes map cleanly onto "rebuild layer
  props when inputs change", and the app is three views — no framework
  weight needed.
- rust-embed over disk serving: one artefact to deploy (the binary), and
  debug-mode disk reads keep dev iterations fast.

## Consequences

- Two toolchains (Rust + Node) in CI and the Docker build (multi-stage).
- The thermal layer endpoint serves binned cells, not per-climb points —
  season/time-of-day filters apply to the tracks layer only (documented in
  the README).
- Old Python scripts are deleted; their stats are a strict subset of the new
  pipeline (git history preserves them). XContest ingest (the old
  `webscrape.py` idea) is filed as a follow-up issue.

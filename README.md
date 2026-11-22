# parastats

Self-hosted paragliding flight log: every flight from your IGC files on a
map, with stats, climb detection, a scrubable track replay and a personal
thermal map. Single binary, single SQLite file, no accounts, no API keys.

Born from the old ParaStats Python scripts and absorbing the (retired)
[flightmap](https://github.com/ruben-hutter/flightmap) heatmap project — its
battle-tested IGC parser, climb detection, track simplification and thermal
rendering live on here (see the module doc comments for attribution).

## What you get

- **Map** (`#/`) — all flights as tracks (plain or altitude-coloured), a
  thermal-core density layer (coloured by peak climb rate), kk7 comparison
  overlay, season / time-of-day / year filters, searchable + sortable flight
  list.
- **Flight detail** (`#/flight/:id`) — stat cards (takeoff/landing, duration,
  distance, max/min altitude, total climb, max climb/sink), full-resolution
  track with a **progress slider** (marker + flown segment), a synced
  altitude profile (hover it to scrub), and the detected climbs list.
- **Stats** (`#/stats`) — totals (flights, airtime, km, max altitude, total
  climb) and per-year bars.

## Quick start (dev)

```sh
# direnv + nix provide rust, node, pnpm; or install your own equivalents
direnv allow

mkdir flights          # put your .igc files here
just ingest            # parse + store them (idempotent, content-hash keyed)
just dev               # backend :8080, vite dev server :5173
                       # → open http://localhost:5173
```

Everything relevant: `just --list`.

## Production (docker)

```sh
docker compose up --build   # app on :8080
```

`docker-compose.yml` mounts `./flights` read-only and ingests it once on
startup (idempotent — dropping new files in and restarting only adds the new
ones). The SQLite database lives in the `parastats-db` named volume. The web
bundle is embedded in the binary at compile time, so the container ships one
artefact.

## CLI

```
parastats serve [--addr 0.0.0.0:8080] [--db ./parastats.db]
    Run migrations, optionally ingest $PARASTATS_INGEST_DIR, serve API + SPA.

parastats ingest <folder> [--db ./parastats.db] [--rebuild]
    Scan a folder of .igc files (recursive) → upsert into the DB.
    --rebuild re-processes files whose content hash already exists.

parastats resolve-sites [--db ./parastats.db] [--refresh]
    Backfill missing flight sites from the external takeoff-site source
    (OpenStreetMap via Overpass, cached in the DB; --refresh forces a
    refetch of the site list).
```

Ingest is idempotent by design: the flight id is the blake3 hash of the file
content, so re-ingesting is a no-op unless the file changed.

**Takeoff sites.** The site name comes from the IGC headers when present
(Skytraxx's `HFSITSITE:`, XCTrack's `HOSITSite:`). Flights without a site
header get one from the **nearest cached OpenStreetMap takeoff site within
300 m** of the takeoff fix (Swiss launches are mapped as
`sport=free_flying`). The OSM site list is fetched via Overpass at most once
a week (`$PARASTATS_OVERPASS_URL` overrides the endpoint) and cached in the
`takeoff_sites` table — there is never a per-flight network call. Unmatched
flights keep `site = NULL` and the UI falls back to the filename. Site names
resolved externally are © [OpenStreetMap contributors](https://www.openstreetmap.org/copyright)
(ODbL).

## API

All JSON, all under `/api`:

| Endpoint                 | Returns                                                   |
| ------------------------ | --------------------------------------------------------- |
| `GET /api/flights?year=` | flight summaries (newest first)                           |
| `GET /api/flights/{id}`  | summary + detected climbs                                 |
| `GET /api/flights/{id}/track` | full-resolution track `[t, lat, lon, alt]` rows      |
| `GET /api/stats`         | totals + per-year breakdown                               |
| `GET /api/layers/skyway` | GeoJSON of simplified tracks (vertices `[lon, lat, alt]`) |
| `GET /api/layers/thermals` | GeoJSON of Mercator-binned thermal cells                |
| `POST /api/ingest`       | `{folder, rebuild?}` → ingest summary                     |

Non-API paths serve the SPA (embedded `web/dist`); unknown paths fall back
to `index.html` for hash routing.

## How it works

1. **Ingest** (`parastats ingest`) walks a folder of `.igc` files in
   parallel, parses each with the lenient fixed-width B-record parser
   (handles both `HFDTE` and `HFDTEDATE:` date forms, Skytraxx's
   `HFSITSITE`/`HFPLTPILOT` and XCTrack's `HOSITSite`/`HFPLTPILOTINCHARGE`
   headers), and stores everything SQLite needs: summary stats, climb
   segments, the full track and a Douglas–Peucker-simplified copy (~5 m
   tolerance). Flights whose file carried no site header get one from the
   cached OSM takeoff-site list (see *Takeoff sites* above;
   `flights.site_source` records where each name came from).
2. **Serve** runs migrations on startup and serves the API + the SPA.
3. **Frontend** (Svelte 5 + MapLibre GL + deck.gl) renders tracks as a
   `PathLayer`, the thermal layer as a client-rendered density canvas
   (`BitmapLayer` — no per-frame work), and the kk7 tiles as a TMS raster
   overlay. Times are stored UTC everywhere; display converts to
   `Europe/Zurich` via `Intl` (DST-safe).

Notes on layer semantics: the **thermal layer** is an all-time overview —
climbs are Mercator-binned into 150 m cells (`count`, avg/peak climb, total
gain), which erases per-climb timestamps; season/time-of-day filters
therefore apply to the **tracks** layer, not the thermal cells.

## Development

```
src/            Rust crate (lib + parastats bin): igc, stats, climb,
                simplify, geo, aggregate, db, ingest, api
migrations/     sqlx migrations (embedded at build, run on startup)
tests/          insta snapshot tests against two real IGC fixtures
web/            Svelte 5 + Vite + TypeScript SPA
flights/        your .igc files (gitignored)
```

- `just lint` / `just test` / `just ci` — clippy `-D warnings`, cargo tests,
  web build + type-check.
- Git hooks: run `just hooks` once per clone — pre-commit checks
  `cargo fmt --check`, pre-push runs `just ci` (everything CI runs, locally;
  bypass with `git push --no-verify`).
- Snapshots: after a deliberate IGC parser change, `cargo insta review`.
- Queries are runtime-checked (no build-time `DATABASE_URL`) — the build
  stays hermetic; migrations live in `migrations/` and are embedded via
  `sqlx::migrate!`.

Stack decisions are recorded in [`docs/adr/`](docs/adr/).

## License

Apache-2.0

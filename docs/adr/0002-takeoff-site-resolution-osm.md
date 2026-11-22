# ADR-0002: Takeoff site resolution — IGC headers first, OSM Overpass cache second

- Status: Accepted
- Date: 2026-11-22
- Context: ParaStats task `feat/takeoff-sites` (issue #5): 159 of 276 flights
  had `site = NULL` because XCTrack (unlike Skytraxx) doesn't write the
  `HFSITSITE` IGC header, so the UI showed raw filenames.

## Decision

Resolve `flights.site` in a fixed fallback order:

1. **IGC headers** — `HFSITSITE:` (Skytraxx) *and* XCTrack's O-record form
   `HOSITSite:` (case-insensitive mnemonic; a bare `?` value is ignored).
   `flights.site_source = 'igc'`.
2. **External takeoff-site lookup** — nearest OpenStreetMap takeoff site
   within **300 m** of the flight's first fix. `flights.site_source = 'osm'`.
3. **Stay NULL** — no wrong names; the UI keeps its filename fallback.

The external source is a **pluggable `SiteSource` trait**
(`src/sites.rs`); the first implementation queries Overpass for the Swiss
bbox (`sport=free_flying` + legacy `paragliding=takeoff`/`free_flying=takeoff`
tags; landing-tagged elements excluded). Results are **cached wholesale in
SQLite** (`takeoff_sites` table, 7-day TTL, mirror-fallback endpoints,
`PARASTATS_OVERPASS_URL` override) — the ingest hot path never does network
I/O per flight. Backfill of existing rows is a dedicated
`parastats resolve-sites` subcommand (no IGC re-hashing); it only touches
`site IS NULL` rows, so a device-provided name always wins.

## Rationale

- External DB over nearest-known-flight matching: our own 117 labeled
  flights are a biased, incomplete sample (34 sites, mostly Ticino);
  OSM is independently maintained and names map cleanly onto the device
  names (validated: nearest-site ≤ 300 m name-agrees with the header name
  on all labeled flights that have a nearby OSM site, zero wrong-site
  matches; the disagreements are the same site under a different name,
  e.g. `Grindelwald First` vs `First`).
- 300 m radius: takeoff GPS jitter on phones is ~10–30 m, launch areas are
  mapped as ways (matched by centroid), and the match count is flat between
  150–500 m (88 → 90 of 159) — 300 m comfortably covers both without
  picking up neighbouring launches.
- Wholesale cache over per-flight queries: one Overpass call per week
  instead of N calls per ingest; Overpass instances are rate-limited
  (observed 406/502/timeouts), so we also try mirrors in order.
- `HOSITSite` parsing: 95 of the 159 header-less flights actually carry it
  (XCTrack writes the site as an observation record) — a free, more
  trustworthy win than any external source for those flights.
- Alternatives rejected: SHV/FVSL airspace API (only serves
  airspaces/wildlife, no sites), ParaglidingEarth (down/unstable),
  burnair/XContest (private, no public API — and scraping is off the table).

## Consequences

- Coverage jumps to ~89 % of real flights named (209 header + 36 OSM of
  276); the rest stay NULL — mostly unmapped launches (e.g. Monte Bar,
  Cimetta Cardada, Aiguille du Midi) and flights outside the Swiss bbox.
  Improving those means mapping the sites in OSM, which anyone can do.
- Two header forms mean two `site_source = 'igc'` provenances collapsed
  into one value — good enough; the file name is recoverable via
  `flights.filename`.
- OSM site names arrive under ODbL: the README attributes them to
  OpenStreetMap contributors.
- A new dependency (`ureq`, blocking, rustls) is the first HTTP client in
  the crate; it is only exercised by `Overpass::fetch` inside
  `spawn_blocking`.
- Site-cache freshness is best-effort: a failed refresh keeps the stale
  cache (with a warning); resolution only fails closed when the cache is
  empty and the fetch fails — flights then stay NULL until the next run.
- Provenance asymmetry between the two fill paths: `resolve-sites` can only
  see DB rows, so a pre-existing NULL row whose file actually carries
  `HOSITSite` gets an OSM name (`'osm'`), while a `--rebuild` re-ingest
  would later give it the header name (`'igc'`). Same file, different
  provenance depending on path — acceptable because both names identify
  the same launch; re-ingesting with `--rebuild` is the canonical way to
  re-derive everything after a parser change.

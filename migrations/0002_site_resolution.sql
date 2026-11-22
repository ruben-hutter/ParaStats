-- Takeoff site resolution from external sources (issue #5).
--
-- flights.site stays the display name; flights.site_source records where it
-- came from so UI/quality decisions stay possible:
--   'igc' — the file itself (HFSITSITE header, or XCTrack's HOSITSite form)
--   'osm' — nearest external takeoff site (OpenStreetMap via Overpass)
-- All pre-existing non-NULL sites came from the IGC header.
ALTER TABLE flights ADD COLUMN site_source TEXT;
UPDATE flights SET site_source = 'igc' WHERE site IS NOT NULL;

-- Cache of external takeoff sites, replaced wholesale on refresh. Never
-- fetched per-flight: ingest and the `resolve-sites` backfill read from here.
CREATE TABLE IF NOT EXISTS takeoff_sites (
  osm_type TEXT NOT NULL,   -- 'node' | 'way' | 'relation'
  osm_id   INTEGER NOT NULL,
  name     TEXT NOT NULL,   -- OSM name tag (name / name:de / name:fr / name:it)
  lat      REAL NOT NULL,
  lon      REAL NOT NULL,
  PRIMARY KEY (osm_type, osm_id)
);

-- Tiny kv store for cache bookkeeping (e.g. 'sites_fetched_at' ISO instant).
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

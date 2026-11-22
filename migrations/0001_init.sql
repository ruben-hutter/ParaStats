-- ParaStats schema v1. Single pilot, single SQLite file.
--
-- flights.id is the blake3 hash of the raw IGC file content: ingest is
-- idempotent by construction (re-ingesting a file overwrites its row, an
-- identical file is skipped). climbs cascade on flight replacement.

CREATE TABLE IF NOT EXISTS flights (
  id                 TEXT PRIMARY KEY,   -- blake3 of file content
  source             TEXT NOT NULL,      -- 'local' (later 'xcontest')
  filename           TEXT NOT NULL,
  date_utc           TEXT NOT NULL,      -- local calendar date (Europe/Zurich) of takeoff, for year filters
  takeoff_time_utc   TEXT NOT NULL,
  landing_time_utc   TEXT NOT NULL,
  duration_s         INTEGER NOT NULL,
  takeoff_lat        REAL,
  takeoff_lon        REAL,
  landing_lat        REAL,
  landing_lon        REAL,
  takeoff_alt_m      INTEGER,
  site               TEXT,
  pilot              TEXT,
  distance_track_m   REAL NOT NULL,
  max_alt_m          INTEGER NOT NULL,
  min_alt_m          INTEGER NOT NULL,
  alt_gain_m         INTEGER NOT NULL,
  max_climb_ms       REAL NOT NULL,
  max_sink_ms        REAL NOT NULL,
  track_simplified   BLOB NOT NULL,      -- msgpack [[t,lat,lon,alt],…] DP-simplified (~5 m)
  track_full         BLOB NOT NULL       -- msgpack [[t,lat,lon,alt],…] full resolution
);

CREATE INDEX IF NOT EXISTS idx_flights_date ON flights(date_utc);
CREATE INDEX IF NOT EXISTS idx_flights_site ON flights(site);

CREATE TABLE IF NOT EXISTS climbs (
  flight_id      TEXT NOT NULL REFERENCES flights(id) ON DELETE CASCADE,
  start_time_utc TEXT NOT NULL,
  end_time_utc   TEXT NOT NULL,
  avg_climb_ms   REAL NOT NULL,
  peak_climb_ms  REAL NOT NULL,
  gain_m         INTEGER NOT NULL,
  centroid_lat   REAL NOT NULL,
  centroid_lon   REAL NOT NULL,
  PRIMARY KEY (flight_id, start_time_utc)
);

CREATE INDEX IF NOT EXISTS idx_climbs_flight ON climbs(flight_id);

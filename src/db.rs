//! SQLite via sqlx: pool setup, embedded migrations (run on startup), row
//! structs and the queries the API + ingest need.
//!
//! Queries are runtime-checked strings (not the `query!` macros) — the build
//! must not need a `DATABASE_URL` pointing at a migrated dev database
//! (deliberate decision: keep `cargo build` hermetic, migrations live in
//! `migrations/` and are embedded via `sqlx::migrate!`).

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, Row, SqlitePool};
use std::str::FromStr;

use crate::aggregate::TrackRow;

/// Open (creating if needed) the SQLite database and run migrations.
pub async fn open_db(path: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(path)
        .with_context(|| format!("parsing db path {path}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal) // readers don't block the writer
        .synchronous(SqliteSynchronous::Normal) // safe with WAL
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .with_context(|| format!("opening sqlite db {path}"))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("running migrations")?;
    Ok(pool)
}

/// One `flights` row minus the track blobs — the summary the list + detail
/// views show. Field names match the API JSON.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct FlightRow {
    pub id: String,
    pub source: String,
    pub filename: String,
    /// Local (Europe/Zurich) calendar date of takeoff, `YYYY-MM-DD`.
    pub date_utc: String,
    pub takeoff_time_utc: String,
    pub landing_time_utc: String,
    pub duration_s: i64,
    pub takeoff_lat: Option<f64>,
    pub takeoff_lon: Option<f64>,
    pub landing_lat: Option<f64>,
    pub landing_lon: Option<f64>,
    pub takeoff_alt_m: Option<i64>,
    pub site: Option<String>,
    /// Where [`FlightRow::site`] came from: `'igc'` (device header) or
    /// `'osm'` (external takeoff-site lookup, see `crate::sites`).
    pub site_source: Option<String>,
    pub pilot: Option<String>,
    pub distance_track_m: f64,
    pub max_alt_m: i64,
    pub min_alt_m: i64,
    pub alt_gain_m: i64,
    pub max_climb_ms: f64,
    pub max_sink_ms: f64,
}

/// One `climbs` row.
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ClimbRow {
    pub flight_id: String,
    pub start_time_utc: String,
    pub end_time_utc: String,
    pub avg_climb_ms: f64,
    pub peak_climb_ms: f64,
    pub gain_m: i64,
    pub centroid_lat: f64,
    pub centroid_lon: f64,
}

/// Everything ingest needs to write for one flight.
pub struct FlightInsert {
    pub row: FlightRow,
    pub track_simplified: Vec<u8>,
    pub track_full: Vec<u8>,
    pub climbs: Vec<ClimbRow>,
}

/// All flight summaries, newest first. Optional `year` filters on the local
/// (Europe/Zurich) takeoff date.
pub async fn flights(pool: &SqlitePool, year: Option<i32>) -> Result<Vec<FlightRow>> {
    let sql = match year {
        Some(_) => {
            "SELECT * FROM flights WHERE substr(date_utc, 1, 4) = ? ORDER BY takeoff_time_utc DESC"
        }
        None => "SELECT * FROM flights ORDER BY takeoff_time_utc DESC",
    };
    let mut q = sqlx::query_as::<_, FlightRow>(sql);
    if let Some(y) = year {
        q = q.bind(y.to_string());
    }
    Ok(q.fetch_all(pool).await?)
}

pub async fn flight(pool: &SqlitePool, id: &str) -> Result<Option<FlightRow>> {
    Ok(
        sqlx::query_as::<_, FlightRow>("SELECT * FROM flights WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn climbs_for(pool: &SqlitePool, flight_id: &str) -> Result<Vec<ClimbRow>> {
    Ok(sqlx::query_as::<_, ClimbRow>(
        "SELECT * FROM climbs WHERE flight_id = ? ORDER BY start_time_utc",
    )
    .bind(flight_id)
    .fetch_all(pool)
    .await?)
}

/// All climbs across all flights (thermal layer input).
pub async fn all_climbs(pool: &SqlitePool) -> Result<Vec<ClimbRow>> {
    Ok(
        sqlx::query_as::<_, ClimbRow>("SELECT * FROM climbs ORDER BY start_time_utc")
            .fetch_all(pool)
            .await?,
    )
}

/// Track blob for one flight: `(column, blob)` where column is
/// `track_full` or `track_simplified`.
pub async fn track_blob(pool: &SqlitePool, id: &str, column: &str) -> Result<Option<Vec<u8>>> {
    // column is a compile-time constant at the call sites, never user input.
    let sql = format!("SELECT {column} AS blob FROM flights WHERE id = ?");
    let row = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;
    Ok(row.and_then(|r| r.try_get::<Option<Vec<u8>>, _>("blob").ok().flatten()))
}

/// `(id, date_utc, takeoff_time_utc, simplified_track)` for every flight —
/// the skyway layer input. Decoding to `TrackRow`s happens in the caller.
pub async fn simplified_tracks(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String, Vec<u8>)>> {
    let rows = sqlx::query(
        "SELECT id, date_utc, takeoff_time_utc, track_simplified AS blob FROM flights ORDER BY takeoff_time_utc",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get::<String, _>("id"),
                r.get::<String, _>("date_utc"),
                r.get::<String, _>("takeoff_time_utc"),
                r.get::<Vec<u8>, _>("blob"),
            )
        })
        .collect())
}

/// Idempotent upsert of one flight: overwrite the `flights` row, replace its
/// climbs. One transaction so a reader never sees a half-written flight.
pub async fn upsert_flight(pool: &SqlitePool, insert: &FlightInsert) -> Result<()> {
    let mut tx = pool.begin().await?;
    let r = &insert.row;
    sqlx::query(
        "INSERT INTO flights (
            id, source, filename, date_utc, takeoff_time_utc, landing_time_utc, duration_s,
            takeoff_lat, takeoff_lon, landing_lat, landing_lon, takeoff_alt_m, site, site_source, pilot,
            distance_track_m, max_alt_m, min_alt_m, alt_gain_m, max_climb_ms, max_sink_ms,
            track_simplified, track_full
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        ON CONFLICT(id) DO UPDATE SET
            source=excluded.source, filename=excluded.filename, date_utc=excluded.date_utc,
            takeoff_time_utc=excluded.takeoff_time_utc, landing_time_utc=excluded.landing_time_utc,
            duration_s=excluded.duration_s, takeoff_lat=excluded.takeoff_lat,
            takeoff_lon=excluded.takeoff_lon, landing_lat=excluded.landing_lat,
            landing_lon=excluded.landing_lon, takeoff_alt_m=excluded.takeoff_alt_m,
            site=excluded.site, site_source=excluded.site_source, pilot=excluded.pilot,
            distance_track_m=excluded.distance_track_m, max_alt_m=excluded.max_alt_m,
            min_alt_m=excluded.min_alt_m, alt_gain_m=excluded.alt_gain_m,
            max_climb_ms=excluded.max_climb_ms, max_sink_ms=excluded.max_sink_ms,
            track_simplified=excluded.track_simplified, track_full=excluded.track_full",
    )
    .bind(&r.id)
    .bind(&r.source)
    .bind(&r.filename)
    .bind(&r.date_utc)
    .bind(&r.takeoff_time_utc)
    .bind(&r.landing_time_utc)
    .bind(r.duration_s)
    .bind(r.takeoff_lat)
    .bind(r.takeoff_lon)
    .bind(r.landing_lat)
    .bind(r.landing_lon)
    .bind(r.takeoff_alt_m)
    .bind(&r.site)
    .bind(&r.site_source)
    .bind(&r.pilot)
    .bind(r.distance_track_m)
    .bind(r.max_alt_m)
    .bind(r.min_alt_m)
    .bind(r.alt_gain_m)
    .bind(r.max_climb_ms)
    .bind(r.max_sink_ms)
    .bind(&insert.track_simplified)
    .bind(&insert.track_full)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM climbs WHERE flight_id = ?")
        .bind(&r.id)
        .execute(&mut *tx)
        .await?;

    for c in &insert.climbs {
        sqlx::query(
            "INSERT INTO climbs (flight_id, start_time_utc, end_time_utc, avg_climb_ms, peak_climb_ms, gain_m, centroid_lat, centroid_lon)
             VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(&c.flight_id)
        .bind(&c.start_time_utc)
        .bind(&c.end_time_utc)
        .bind(c.avg_climb_ms)
        .bind(c.peak_climb_ms)
        .bind(c.gain_m)
        .bind(c.centroid_lat)
        .bind(c.centroid_lon)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn flight_exists(pool: &SqlitePool, id: &str) -> Result<bool> {
    let row = sqlx::query("SELECT 1 AS one FROM flights WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// msgpack-encode a track.
pub fn encode_track(track: &[TrackRow]) -> Vec<u8> {
    rmp_serde::to_vec(track).expect("msgpack encode of track rows cannot fail")
}

/// msgpack-decode a track.
pub fn decode_track(blob: &[u8]) -> Result<Vec<TrackRow>> {
    Ok(rmp_serde::from_slice(blob)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// File-backed temp db: a pooled `:memory:` db would give each pooled
    /// connection its own empty database.
    async fn test_db() -> SqlitePool {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("parastats-dbtest-{}-{n}.db", std::process::id()));
        // Left in /tmp after the test — fine, the OS cleans up and a unique
        // name per test avoids cross-test interference.
        open_db(path.to_str().unwrap()).await.unwrap()
    }

    fn sample_track() -> Vec<TrackRow> {
        vec![
            (1_753_000_000, 46.0, 8.0, 1200),
            (1_753_000_001, 46.0001, 8.0001, 1205),
            (1_753_000_002, 46.0002, 8.0002, 1210),
        ]
    }

    #[test]
    fn track_msgpack_round_trip() {
        let blob = encode_track(&sample_track());
        assert_eq!(decode_track(&blob).unwrap(), sample_track());
    }

    #[tokio::test]
    async fn upsert_is_idempotent_and_replaces_climbs() {
        let pool = test_db().await;
        let row = FlightRow {
            id: "abc".into(),
            source: "local".into(),
            filename: "f.igc".into(),
            date_utc: "2025-07-20".into(),
            takeoff_time_utc: "2025-07-20T12:00:00".into(),
            landing_time_utc: "2025-07-20T13:00:00".into(),
            duration_s: 3600,
            takeoff_lat: Some(46.0),
            takeoff_lon: Some(8.0),
            landing_lat: Some(46.1),
            landing_lon: Some(8.1),
            takeoff_alt_m: Some(1200),
            site: Some("Monte Lema".into()),
            site_source: Some("igc".into()),
            pilot: Some("Ruben".into()),
            distance_track_m: 12_345.6,
            max_alt_m: 2000,
            min_alt_m: 1190,
            alt_gain_m: 800,
            max_climb_ms: 3.5,
            max_sink_ms: -4.2,
        };
        let climbs = vec![ClimbRow {
            flight_id: "abc".into(),
            start_time_utc: "2025-07-20T12:10:00".into(),
            end_time_utc: "2025-07-20T12:15:00".into(),
            avg_climb_ms: 2.0,
            peak_climb_ms: 3.5,
            gain_m: 300,
            centroid_lat: 46.05,
            centroid_lon: 8.05,
        }];
        let insert = FlightInsert {
            row: row.clone(),
            track_simplified: encode_track(&sample_track()),
            track_full: encode_track(&sample_track()),
            climbs,
        };
        upsert_flight(&pool, &insert).await.unwrap();

        // Re-ingest with a different climb set — old climbs must be replaced.
        let mut insert2 = FlightInsert {
            row: row.clone(),
            ..insert
        };
        insert2.climbs = vec![ClimbRow {
            flight_id: "abc".into(),
            start_time_utc: "2025-07-20T12:20:00".into(),
            end_time_utc: "2025-07-20T12:25:00".into(),
            avg_climb_ms: 1.5,
            peak_climb_ms: 2.5,
            gain_m: 200,
            centroid_lat: 46.06,
            centroid_lon: 8.06,
        }];
        upsert_flight(&pool, &insert2).await.unwrap();

        assert_eq!(flights(&pool, None).await.unwrap().len(), 1);
        assert_eq!(climbs_for(&pool, "abc").await.unwrap().len(), 1);
        assert_eq!(climbs_for(&pool, "abc").await.unwrap()[0].gain_m, 200);
        assert_eq!(
            decode_track(
                &track_blob(&pool, "abc", "track_full")
                    .await
                    .unwrap()
                    .unwrap()
            )
            .unwrap()
            .len(),
            3
        );
    }

    #[tokio::test]
    async fn year_filter_uses_local_date() {
        let pool = test_db().await;
        for (id, date) in [("a", "2024-06-01"), ("b", "2025-06-01")] {
            let row = FlightRow {
                id: id.into(),
                source: "local".into(),
                filename: format!("{id}.igc"),
                date_utc: date.into(),
                takeoff_time_utc: format!("{date}T12:00:00"),
                landing_time_utc: format!("{date}T13:00:00"),
                duration_s: 3600,
                takeoff_lat: None,
                takeoff_lon: None,
                landing_lat: None,
                landing_lon: None,
                takeoff_alt_m: None,
                site: None,
                site_source: None,
                pilot: None,
                distance_track_m: 0.0,
                max_alt_m: 0,
                min_alt_m: 0,
                alt_gain_m: 0,
                max_climb_ms: 0.0,
                max_sink_ms: 0.0,
            };
            upsert_flight(
                &pool,
                &FlightInsert {
                    row,
                    track_simplified: encode_track(&sample_track()),
                    track_full: encode_track(&sample_track()),
                    climbs: vec![],
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(flights(&pool, None).await.unwrap().len(), 2);
        assert_eq!(flights(&pool, Some(2025)).await.unwrap().len(), 1);
    }
}

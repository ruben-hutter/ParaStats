//! Ingest pipeline: scan a folder of `.igc` files (recursively, in parallel
//! via rayon), parse each, compute stats + climbs + simplified tracks, and
//! upsert into SQLite keyed by the blake3 hash of the file content.
//!
//! Idempotency: identical file content → identical id → re-ingest overwrites
//! (one flight, one row). `--rebuild` forces reprocessing even when the hash
//! already exists; without it, known hashes are skipped entirely.
//!
//! Ported from flightmap's `ingest` module (same folder-walk + parallel
//! parse + skip-bad-files strategy) and extended with the parastats
//! stats/climb/simplify persistence stages.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::NaiveDateTime;
use chrono_tz::Europe::Zurich;
use rayon::prelude::*;
use sqlx::SqlitePool;
use walkdir::WalkDir;

use crate::climb::{detect_climbs, ClimbConfig};
use crate::db::{self, ClimbRow, FlightInsert, FlightRow};
use crate::igc::parse_igc;
use crate::model::SourceKind;
use crate::simplify::simplify_indices;
use crate::stats;

/// Douglas–Peucker tolerance for the stored simplified track (flightmap
/// default; good balance of fidelity vs payload for the map layer).
pub const SIMPLIFY_TOLERANCE_M: f64 = 5.0;

/// One IGC file's outcome.
#[derive(Debug)]
pub struct IngestOutcome {
    pub path: PathBuf,
    pub result: std::result::Result<Ingested, String>,
}

#[derive(Debug)]
pub enum Ingested {
    Stored,
    Replaced,
    Skipped,
}

/// Summary printed after an ingest run.
pub struct IngestSummary {
    pub scanned: usize,
    pub stored: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub failed: usize,
    pub failures: Vec<(PathBuf, String)>,
}

/// Ingest every `.igc` under `folder` into `pool`.
pub async fn ingest_folder(
    pool: &SqlitePool,
    folder: &Path,
    rebuild: bool,
) -> Result<IngestSummary> {
    let files = collect_igc_files(folder)?;
    if files.is_empty() {
        bail!("no .igc files found under {}", folder.display());
    }

    // Phase 1 (parallel, CPU-bound): read + hash + parse + derive everything.
    let prepared: Vec<Prepared> = files
        .par_iter()
        .filter_map(|path| match prepare_file(path) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("warn: skip {}: {}", path.display(), e);
                None
            }
        })
        .collect();

    // Phase 2 (serial, DB-bound): idempotent upserts in a stable order.
    let mut summary = IngestSummary {
        scanned: files.len(),
        stored: 0,
        replaced: 0,
        skipped: 0,
        failed: files.len() - prepared.len(),
        failures: Vec::new(),
    };

    for p in prepared {
        let exists = db::flight_exists(pool, &p.insert.row.id).await?;
        if exists && !rebuild {
            summary.skipped += 1;
            continue;
        }
        match db::upsert_flight(pool, &p.insert).await {
            Ok(()) => {
                if exists {
                    summary.replaced += 1;
                } else {
                    summary.stored += 1;
                }
            }
            Err(e) => {
                let msg = format!("{e:#}");
                eprintln!("warn: db error {}: {msg}", p.path.display());
                summary.failed += 1;
                summary.failures.push((p.path.clone(), msg));
            }
        }
    }

    Ok(summary)
}

/// Everything derived from one file, ready to persist.
struct Prepared {
    path: PathBuf,
    insert: FlightInsert,
}

/// Timestamp format used everywhere in the DB/API: naive ISO, implicitly UTC
/// ("2025-07-20T12:04:01"). The frontend appends `Z` when parsing.
pub fn iso(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Local (Europe/Zurich) calendar date of a UTC takeoff timestamp — the
/// `flights.date_utc` column, used for year filters.
pub fn local_date(t: NaiveDateTime) -> String {
    // Naive → UTC instant → Zurich wall clock. Starting from an instant is
    // always unambiguous (DST fold/gap only matter in the other direction).
    t.and_utc()
        .with_timezone(&Zurich)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// Read, hash, parse and derive the DB payload for one file.
fn prepare_file(path: &Path) -> Result<Prepared> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let id = blake3::hash(&bytes).to_hex().to_string();
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let text = String::from_utf8_lossy(&bytes);
    let flight_id = format!("{}:{}", SourceKind::Local.prefix(), filename);
    let flight = parse_igc(&text, &flight_id).context("parsing IGC")?;

    let s = stats::summarize(&flight);
    let climbs = detect_climbs(&flight, &ClimbConfig::default());

    let full_track: Vec<(i64, f64, f64, i32)> = flight
        .points
        .iter()
        .map(|p| {
            (
                p.time.and_utc().timestamp(),
                p.lat,
                p.lon,
                p.alt().unwrap_or(0),
            )
        })
        .collect();
    let simple_idxs = simplify_indices(&flight, SIMPLIFY_TOLERANCE_M);
    let simple_track: Vec<(i64, f64, f64, i32)> =
        simple_idxs.iter().map(|&i| full_track[i]).collect();

    let row = FlightRow {
        id,
        source: SourceKind::Local.prefix().to_string(),
        filename,
        date_utc: local_date(s.takeoff_time),
        takeoff_time_utc: iso(s.takeoff_time),
        landing_time_utc: iso(s.landing_time),
        duration_s: s.duration_s,
        takeoff_lat: Some(s.takeoff_lat),
        takeoff_lon: Some(s.takeoff_lon),
        landing_lat: Some(s.landing_lat),
        landing_lon: Some(s.landing_lon),
        takeoff_alt_m: s.takeoff_alt_m.map(i64::from),
        site: flight.site.clone(),
        pilot: flight.pilot.clone(),
        distance_track_m: s.distance_track_m,
        max_alt_m: s.max_alt_m.map(i64::from).unwrap_or(0),
        min_alt_m: s.min_alt_m.map(i64::from).unwrap_or(0),
        alt_gain_m: s.alt_gain_m,
        max_climb_ms: f64::from(s.max_climb_ms.unwrap_or(0.0)),
        max_sink_ms: f64::from(s.max_sink_ms.unwrap_or(0.0)),
    };

    let climb_rows: Vec<ClimbRow> = climbs
        .iter()
        .map(|c| ClimbRow {
            flight_id: row.id.clone(),
            start_time_utc: iso(c.start_time),
            end_time_utc: iso(c.end_time),
            avg_climb_ms: f64::from(c.avg_climb_ms),
            peak_climb_ms: f64::from(c.peak_climb_ms),
            gain_m: i64::from(c.gain_m),
            centroid_lat: c.centroid.0,
            centroid_lon: c.centroid.1,
        })
        .collect();

    Ok(Prepared {
        path: path.to_path_buf(),
        insert: FlightInsert {
            row,
            track_simplified: db::encode_track(&simple_track),
            track_full: db::encode_track(&full_track),
            climbs: climb_rows,
        },
    })
}

fn collect_igc_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let is_igc = entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("igc"));
        if is_igc {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_date_handles_dst_boundaries() {
        // 2025-07-20 12:00 UTC = 14:00 CEST (UTC+2, summer time) → same day.
        let summer =
            NaiveDateTime::parse_from_str("2025-07-20 23:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(local_date(summer), "2025-07-21"); // 01:30 next day local

        // 2025-01-20 23:30 UTC = 00:30 next day CET (UTC+1, winter time).
        let winter =
            NaiveDateTime::parse_from_str("2025-01-20 23:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(local_date(winter), "2025-01-21");
    }

    #[tokio::test]
    async fn ingest_fixtures_end_to_end() {
        let dir = std::env::temp_dir().join(format!("parastats-ingest-{}", std::process::id()));
        let db_path = dir.join("test.db");
        std::fs::create_dir_all(&dir).unwrap();
        let pool = db::open_db(db_path.to_str().unwrap()).await.unwrap();

        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let summary = ingest_folder(&pool, &fixtures, false).await.unwrap();
        assert_eq!(summary.stored, 2, "both fixtures must ingest");
        assert_eq!(summary.failed, 0);

        // Idempotent second run.
        let summary = ingest_folder(&pool, &fixtures, false).await.unwrap();
        assert_eq!(summary.skipped, 2);
        assert_eq!(summary.stored, 0);

        let flights = db::flights(&pool, None).await.unwrap();
        assert_eq!(flights.len(), 2);
        let skytraxx = flights
            .iter()
            .find(|f| f.filename.contains("skytraxx"))
            .expect("skytraxx fixture row");
        assert_eq!(skytraxx.site.as_deref(), Some("Monte Lema"));
        assert!(skytraxx.duration_s > 0);
        assert!(skytraxx.distance_track_m > 0.0);

        let full = db::track_blob(&pool, &skytraxx.id, "track_full")
            .await
            .unwrap()
            .expect("track_full blob");
        let decoded = db::decode_track(&full).unwrap();
        assert_eq!(decoded.len(), 430); // known fixture point count
        assert!(decoded[0].3 > 0); // altitude present

        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! Snapshot tests for the IGC parser against `tests/fixtures/`.
//!
//! Two real flights (anonymised) are committed: one from a Skytraxx vario
//! (bare `HFDTE` date format) and one from XCTrack (`HFDTEDATE:DDMMYY,NN`).
//! Together they pin both formats the parser has to handle in production.
//!
//! Run `cargo insta review` after a deliberate parser change to accept new
//! snapshots, or `cargo insta accept` to accept all new snapshots blindly.

use std::fs;

use parastats::parse_igc;

fn load_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    fs::read_to_string(path).unwrap_or_else(|e| panic!("reading fixture {name}: {e}"))
}

/// Compact summary we snapshot — full point dumps would be noisy. Per-field
/// correctness is covered by the unit tests in `src/igc.rs`.
fn summary(flight: &parastats::Flight) -> String {
    let pts = &flight.points;
    let (lat_min, lat_max) = pts
        .iter()
        .map(|p| p.lat)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
    let (lon_min, lon_max) = pts
        .iter()
        .map(|p| p.lon)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
    let baro_min = pts.iter().filter_map(|p| p.alt_baro).min();
    let baro_max = pts.iter().filter_map(|p| p.alt_baro).max();
    format!(
        "id={} points={} start={} end={} lat=[{:.5},{:.5}] lon=[{:.5},{:.5}] baro=[{:?},{:?}]",
        flight.id,
        pts.len(),
        pts.first().unwrap().time,
        pts.last().unwrap().time,
        lat_min,
        lat_max,
        lon_min,
        lon_max,
        baro_min,
        baro_max,
    )
}

#[test]
fn snapshot_skytraxx_fixture() {
    // Skytraxx vario. Uses bare `HFDTEdDMMYY` date format.
    let text = load_fixture("skytraxx_monte_lema.igc");
    let flight = parse_igc(&text, "local:skytraxx_monte_lema.igc").expect("parse must succeed");
    insta::assert_snapshot!("skytraxx_monte_lema", summary(&flight));
}

#[test]
fn snapshot_xctrack_fixture() {
    // XCTrack (phone). Uses `HFDTEDATE:DDMMYY,NN` — the format that broke the
    // original parser. This test exists as a regression guard.
    let text = load_fixture("xctrack_monte_lema.igc");
    let flight = parse_igc(&text, "local:xctrack_monte_lema.igc").expect("parse must succeed");
    insta::assert_snapshot!("xctrack_monte_lema", summary(&flight));
}

#[test]
fn both_date_formats_yield_same_calendar_day() {
    // Skytraxx (HFDTE200725) and XCTrack (HFDTEDATE:210725,01) are one day
    // apart in calendar terms; this just confirms each format decodes to its
    // own correct date rather than both falling back to a default.
    let sky = parse_igc(&load_fixture("skytraxx_monte_lema.igc"), "local:sky.igc").unwrap();
    let xc = parse_igc(&load_fixture("xctrack_monte_lema.igc"), "local:xc.igc").unwrap();
    assert_eq!(
        sky.points[0].time.date(),
        chrono::NaiveDate::from_ymd_opt(2025, 7, 20).unwrap()
    );
    assert_eq!(
        xc.points[0].time.date(),
        chrono::NaiveDate::from_ymd_opt(2025, 7, 21).unwrap()
    );
}

#[test]
fn site_and_pilot_headers_are_captured() {
    // Skytraxx writes HFSITSITE + HFPLTPILOT; XCTrack writes
    // HFPLTPILOTINCHARGE and its site as the O-record HOSITSite. Both site
    // forms must surface in metadata.
    let sky = parse_igc(&load_fixture("skytraxx_monte_lema.igc"), "local:sky.igc").unwrap();
    assert_eq!(sky.site.as_deref(), Some("Monte Lema"));
    assert_eq!(sky.pilot.as_deref(), Some("Anonymised"));

    let xc = parse_igc(&load_fixture("xctrack_monte_lema.igc"), "local:xc.igc").unwrap();
    assert_eq!(xc.site.as_deref(), Some("Monte Lema"));
    assert_eq!(xc.pilot.as_deref(), Some("Anonymised"));
}

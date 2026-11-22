//! Per-flight summary statistics, computed once at ingest and stored in the
//! `flights` row. Everything the list/detail/stats views need except climbs.
//!
//! Semantics (supersedes the old Python scripts — their stats are a strict
//! subset of these):
//!
//!  * takeoff/landing  = first/last valid fix.
//!  * duration         = span between them (UTC; IGC is UTC-native).
//!  * distance_track_m = haversine sum over consecutive fixes; fixes with a
//!    non-positive dt (clock skew, duplicates) are skipped so they don't
//!    double-count.
//!  * max/min alt      = over the baro-preferred series (GPS fallback), same
//!    series climb detection uses.
//!  * alt_gain_m       = sum of *positive* deltas of the smoothed altitude
//!    series — total metres climbed, thermalling included.
//!  * max_climb_ms / max_sink_ms = max/min of the smoothed vario series
//!    (flightmap semantics; the old Python's 15 s integrated variant is
//!    superseded by this).

use crate::climb;
use crate::geo::haversine_approx_m;
use crate::model::Flight;

/// Rolling-mean window for the gain/vario series. Matches
/// [`climb::ClimbConfig::default`] so the numbers a pilot sees in the detail
/// view agree with what climb detection "felt".
pub const SMOOTHING_WINDOW_S: f32 = 5.0;

/// Summary stats for one flight. Coordinates are (lat, lon).
#[derive(Debug, Clone, PartialEq)]
pub struct FlightSummary {
    pub takeoff_time: chrono::NaiveDateTime,
    pub landing_time: chrono::NaiveDateTime,
    pub duration_s: i64,
    pub takeoff_lat: f64,
    pub takeoff_lon: f64,
    pub landing_lat: f64,
    pub landing_lon: f64,
    pub takeoff_alt_m: Option<i32>,
    pub distance_track_m: f64,
    pub max_alt_m: Option<i32>,
    pub min_alt_m: Option<i32>,
    pub alt_gain_m: i64,
    /// Peak smoothed climb rate, m/s. `None` when no altitude data exists.
    pub max_climb_ms: Option<f32>,
    /// Peak smoothed sink rate, m/s (negative). `None` when no altitude data.
    pub max_sink_ms: Option<f32>,
}

/// Compute the summary for a parsed [`Flight`].
pub fn summarize(flight: &Flight) -> FlightSummary {
    let first = &flight.points[0];
    let last = &flight.points[flight.points.len() - 1];
    let duration_s = (last.time - first.time).num_seconds().max(0);

    let mut distance = 0.0_f64;
    let mut prev: Option<&crate::model::TrackPoint> = None;
    for p in &flight.points {
        if let Some(q) = prev {
            // Skip non-monotonic timestamps: duplicates and clock skew would
            // otherwise contribute bogus segments to the distance sum.
            if p.time > q.time {
                distance += haversine_approx_m(q.lat, q.lon, p.lat, p.lon);
            }
        }
        prev = Some(p);
    }

    // Altitude stats over the baro-preferred series, aligned with climb
    // detection (same skip pattern → same series).
    let (alts, dts) = climb::altitude_series(flight);
    let (max_alt, min_alt, gain, max_climb, max_sink) = if alts.is_empty() {
        (None, None, 0, None, None)
    } else {
        let smoothed = climb::rolling_mean(&alts, &dts, SMOOTHING_WINDOW_S);
        let vario = climb::vertical_speed(&smoothed, &dts);

        let mut gain: f32 = 0.0;
        for i in 1..smoothed.len() {
            let d = smoothed[i] - smoothed[i - 1];
            if d > 0.0 {
                gain += d;
            }
        }

        let max_climb = vario.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let max_sink = vario.iter().copied().fold(f32::INFINITY, f32::min);

        (
            Some(*alts.iter().max_by(|a, b| a.total_cmp(b)).unwrap() as i32),
            Some(*alts.iter().min_by(|a, b| a.total_cmp(b)).unwrap() as i32),
            gain.round() as i64,
            Some(max_climb),
            Some(max_sink),
        )
    };

    FlightSummary {
        takeoff_time: first.time,
        landing_time: last.time,
        duration_s,
        takeoff_lat: first.lat,
        takeoff_lon: first.lon,
        landing_lat: last.lat,
        landing_lon: last.lon,
        takeoff_alt_m: first.alt(),
        distance_track_m: distance,
        max_alt_m: max_alt,
        min_alt_m: min_alt,
        alt_gain_m: gain,
        max_climb_ms: max_climb,
        max_sink_ms: max_sink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceKind, TrackPoint};
    use chrono::{Duration, NaiveDate};

    fn base_time() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2025, 7, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    /// n points, 1 Hz. Position steps `step_deg` north per second (0.001° ≈
    /// 111 m). Baro altitude from `alt_fn(i)`, GPS altitude None.
    fn synthetic_flight(n: usize, step_deg: f64, alt_fn: impl Fn(usize) -> i32) -> Flight {
        let points: Vec<TrackPoint> = (0..n)
            .map(|i| TrackPoint {
                time: base_time() + Duration::seconds(i as i64),
                lat: 46.0 + i as f64 * step_deg,
                lon: 8.0,
                alt_gps: None,
                alt_baro: Some(alt_fn(i)),
            })
            .collect();
        Flight {
            id: "local:sync.igc".into(),
            pilot: None,
            site: None,
            points,
            source: SourceKind::Local,
        }
    }

    #[test]
    fn flat_flight_has_zero_gain_and_distance_right() {
        // 100 points, 111.32 m apart each → ~11.0 km total.
        let f = synthetic_flight(100, 0.001, |_| 1000);
        let s = summarize(&f);
        assert_eq!(s.duration_s, 99);
        assert!(
            (s.distance_track_m - 99.0 * 111.32).abs() < 200.0,
            "got {}",
            s.distance_track_m
        );
        assert_eq!(s.alt_gain_m, 0);
        assert_eq!(s.max_alt_m, Some(1000));
        assert_eq!(s.min_alt_m, Some(1000));
        assert_eq!(s.takeoff_alt_m, Some(1000));
        // Flat series → vario exactly 0 everywhere.
        assert_eq!(s.max_climb_ms, Some(0.0));
        assert_eq!(s.max_sink_ms, Some(0.0));
    }

    #[test]
    fn steady_climb_is_reported() {
        // 2 m/s for 60 s → gain ~120 m, max climb ~2 m/s.
        let f = synthetic_flight(61, 0.0, |i| 1000 + (i as i32) * 2);
        let s = summarize(&f);
        assert_eq!(s.max_alt_m, Some(1120));
        assert_eq!(s.min_alt_m, Some(1000));
        assert!(
            (s.alt_gain_m - 115).abs() <= 4,
            "gain {} (120 m climbed minus ~5 m smoothing edge lag)",
            s.alt_gain_m
        );
        let climb = s.max_climb_ms.unwrap();
        assert!(climb > 1.5 && climb < 2.5, "max climb {climb}");
        assert!(
            s.max_sink_ms.unwrap() >= -0.1,
            "no sink expected: {}",
            s.max_sink_ms.unwrap()
        );
    }

    #[test]
    fn steady_sink_is_reported_negative() {
        // -1 m/s for 60 s.
        let f = synthetic_flight(61, 0.0, |i| 2000 - (i as i32));
        let s = summarize(&f);
        assert_eq!(s.alt_gain_m, 0);
        let sink = s.max_sink_ms.unwrap();
        assert!(sink < -0.8 && sink > -1.2, "sink {sink}");
        assert!((s.max_climb_ms.unwrap() - 0.0).abs() < 0.1);
    }

    #[test]
    fn duplicate_timestamps_do_not_inflate_distance() {
        // Two fixes at the same instant (clock hiccup): distance only counts
        // the monotonic leg.
        let mut f = synthetic_flight(3, 0.001, |_| 1000);
        f.points[2].time = f.points[1].time; // duplicate ts
        let s = summarize(&f);
        assert!(s.distance_track_m < 250.0, "got {}", s.distance_track_m);
    }

    #[test]
    fn no_altitude_data_yields_none() {
        let mut f = synthetic_flight(10, 0.001, |_| 0);
        for p in &mut f.points {
            p.alt_baro = None;
            p.alt_gps = None;
        }
        let s = summarize(&f);
        assert_eq!(s.max_alt_m, None);
        assert_eq!(s.takeoff_alt_m, None);
        assert_eq!(s.alt_gain_m, 0);
        assert_eq!(s.max_climb_ms, None);
        assert_eq!(s.max_sink_ms, None);
        // Position/duration stats still work.
        assert_eq!(s.duration_s, 9);
        assert!(s.distance_track_m > 900.0);
    }

    #[test]
    fn gps_altitude_is_fallback() {
        let mut f = synthetic_flight(10, 0.0, |_| 0);
        for (i, p) in f.points.iter_mut().enumerate() {
            p.alt_baro = None;
            p.alt_gps = Some(500 + i as i32);
        }
        let s = summarize(&f);
        assert_eq!(s.max_alt_m, Some(509));
    }

    #[test]
    fn takeoff_and_landing_are_first_and_last_fix() {
        let f = synthetic_flight(5, 0.001, |_| 1000);
        let s = summarize(&f);
        assert_eq!(s.takeoff_time, base_time());
        assert_eq!(s.landing_time, base_time() + Duration::seconds(4));
        assert_eq!(s.takeoff_lat, 46.0);
        assert_eq!(s.landing_lat, 46.004);
    }
}

//! Climb-segment detection.
//!
//! Ported verbatim from flightmap `src/climb.rs` (verified against 245 real
//! flights — the smoothing/threshold semantics are load-bearing for the
//! thermal layer, so resist the urge to "clean them up").
//!
//! Algorithm:
//!   1. Pick an altitude source: prefer `alt_baro`, fall back to `alt_gps`.
//!   2. Smooth the altitude series with a rolling mean (~5 s window). Baro
//!      noise dominates if you difference raw fixes.
//!   3. Compute vertical speed between adjacent smoothed fixes (m/s).
//!   4. Find maximal runs where smoothed vario ≥ `min_climb_ms` sustained for
//!      at least `min_duration_s` seconds.
//!   5. Emit one [`ClimbSegment`] per run with avg/peak rate, total gain,
//!      and a centroid (mean of fixes for now — circle-fit is a possible
//!      refinement).

use chrono::NaiveDateTime;

use crate::model::{ClimbSegment, Flight};

/// Tunable parameters for [`detect_climbs`]. Defaults are sensible starting
/// points for paragliding.
#[derive(Debug, Clone)]
pub struct ClimbConfig {
    /// Minimum smoothed climb rate, m/s. Below this the pilot isn't
    /// considered to be in a thermal. 0.5 m/s ≈ 100 fpm — the paraglider
    /// "are we going up?" floor.
    pub min_climb_ms: f32,
    /// Minimum sustained duration, seconds. Filters out momentary bumps.
    pub min_duration_s: f32,
    /// Smoothing window in seconds. 5 s ≈ 1 full thermal circle for an
    /// average paraglider; long enough to dampen baro noise, short enough
    /// not to smear the segment boundaries.
    pub smoothing_window_s: f32,
}

impl Default for ClimbConfig {
    fn default() -> Self {
        Self {
            min_climb_ms: 0.5,
            min_duration_s: 10.0,
            smoothing_window_s: 5.0,
        }
    }
}

/// Detect climb segments in a flight. Returns segments ordered by start
/// time. Flights with fewer than two points or no usable altitude return an
/// empty vec.
pub fn detect_climbs(flight: &Flight, config: &ClimbConfig) -> Vec<ClimbSegment> {
    let n = flight.points.len();
    if n < 2 {
        return Vec::new();
    }

    // Pick an altitude series: baro preferred, gps fallback.
    let (altitudes, dt_s): (Vec<f32>, Vec<f32>) = altitude_series(flight);
    if altitudes.is_empty() {
        return Vec::new();
    }

    // Smooth, then compute vertical speed.
    let smoothed = rolling_mean(&altitudes, &dt_s, config.smoothing_window_s);
    let vario = vertical_speed(&smoothed, &dt_s);

    // Find runs where vario ≥ threshold, sustained ≥ min_duration_s.
    let runs = find_climb_runs(&vario, &dt_s, config.min_climb_ms, config.min_duration_s);

    let mut segments = Vec::with_capacity(runs.len());
    for (start_idx, end_idx) in runs {
        segments.push(build_segment(flight, &vario, &dt_s, start_idx, end_idx));
    }
    segments
}

/// Build the (altitude, dt) series for a flight. Returns parallel vecs;
/// `dt_s[i]` is the seconds between point `i-1` and `i` (with `dt_s[0] = 0`).
/// Points with no altitude source are skipped; if that breaks the time
/// continuity we still proceed by treating dts as the gaps between surviving
/// points (the smoothing/vario math handles non-uniform spacing).
///
/// Public so [`crate::stats`] can reuse the same smoothed-vario series for
/// max climb/sink (one altitude pipeline, one set of semantics).
pub fn altitude_series(flight: &Flight) -> (Vec<f32>, Vec<f32>) {
    let mut alts = Vec::with_capacity(flight.points.len());
    let mut dts = Vec::with_capacity(flight.points.len());
    let mut prev_t: Option<NaiveDateTime> = None;
    for p in &flight.points {
        let Some(alt) = p.alt_baro.or(p.alt_gps) else {
            continue;
        };
        let dt = match prev_t {
            Some(t) => (p.time - t).num_milliseconds() as f32 / 1000.0,
            None => 0.0,
        };
        // Clamp negative dts (clock skew, duplicate timestamps) to a small
        // positive value so smoothing math doesn't blow up.
        let dt = if dt > 0.0 { dt } else { 0.001 };
        alts.push(alt as f32);
        dts.push(dt);
        prev_t = Some(p.time);
    }
    (alts, dts)
}

/// Rolling mean with a *time-based* window in seconds. The window for index
/// `i` extends backward in time until the cumulative dt exceeds `window_s`.
/// This handles non-uniform fix rates gracefully (a real issue with IGC from
/// phones that throttle logging).
pub fn rolling_mean(values: &[f32], dt_s: &[f32], window_s: f32) -> Vec<f32> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0_f32; n];
    for (i, out_i) in out.iter_mut().enumerate() {
        let mut acc = 0.0_f32;
        let mut weight_sum = 0.0_f32;
        let mut elapsed = 0.0_f32;
        // Walk backward from i (inclusive) until we've covered window_s.
        let mut j = i;
        loop {
            let dt = if j == 0 { 0.0 } else { dt_s[j] };
            // Weight each sample by its forward dt so longer gaps don't
            // dominate the mean. (Crude trapezoidal-ish handling.)
            let w = if j + 1 < n { dt_s[j + 1] } else { dt }.max(0.001);
            acc += values[j] * w;
            weight_sum += w;
            if j == 0 {
                break;
            }
            elapsed += dt;
            if elapsed >= window_s {
                break;
            }
            j -= 1;
        }
        *out_i = acc / weight_sum;
    }
    out
}

/// Per-sample vertical speed (m/s). `out[i]` is the rate of altitude change
/// between sample `i-1` and `i`. `out[0]` is 0.
pub fn vertical_speed(smoothed: &[f32], dt_s: &[f32]) -> Vec<f32> {
    let n = smoothed.len();
    let mut out = vec![0.0_f32; n];
    for i in 1..n {
        let dt = dt_s[i].max(0.001);
        out[i] = (smoothed[i] - smoothed[i - 1]) / dt;
    }
    out
}

/// Find maximal runs of indices where `vario[i] ≥ min_climb_ms` and the
/// cumulative dt of the run is ≥ `min_duration_s`. Returns `(start, end)`
/// inclusive index pairs into the `vario` array.
fn find_climb_runs(
    vario: &[f32],
    dt_s: &[f32],
    min_climb_ms: f32,
    min_duration_s: f32,
) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    let n = vario.len();
    for i in 0..n {
        let climbing = vario[i] >= min_climb_ms;
        match (start, climbing) {
            (None, true) => start = Some(i),
            (Some(_), false) => {
                if let Some(s) = start.take() {
                    let elapsed: f32 = dt_s[s + 1..=i].iter().sum();
                    if elapsed >= min_duration_s {
                        runs.push((s, i - 1));
                    }
                }
            }
            _ => {}
        }
    }
    // Tail: file ends while still climbing.
    if let Some(s) = start {
        let elapsed: f32 = dt_s[s + 1..].iter().sum();
        if elapsed >= min_duration_s {
            runs.push((s, n - 1));
        }
    }
    runs
}

/// Build a [`ClimbSegment`] from a run of indices. Centroid is the unweighted
/// mean of (lat, lon) over the run for now — circle-fit is a planned
/// refinement (PLAN.md §4).
fn build_segment(
    flight: &Flight,
    vario: &[f32],
    dt_s: &[f32],
    start_idx: usize,
    end_idx: usize,
) -> ClimbSegment {
    // Indices here index into the altitude/vario arrays, which may be a
    // subset of flight.points (when some points had no altitude). We need
    // to map back to flight.points. We rebuild the same skip pattern to
    // avoid carrying an extra index vec through the call chain.
    let flight_idxs: Vec<usize> = flight
        .points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.alt_baro.or(p.alt_gps).map(|_| i))
        .collect();

    let f_start = flight_idxs.get(start_idx).copied().unwrap_or(0);
    let f_end = flight_idxs
        .get(end_idx)
        .copied()
        .unwrap_or(flight.points.len() - 1);

    let pts = &flight.points[f_start..=f_end];
    let vario_slice = &vario[start_idx..=end_idx];

    let avg_climb_ms = vario_slice.iter().copied().sum::<f32>() / vario_slice.len().max(1) as f32;
    let peak_climb_ms = vario_slice
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);

    let alt_first = pts
        .first()
        .and_then(|p| p.alt_baro.or(p.alt_gps))
        .unwrap_or(0);
    let alt_last = pts
        .last()
        .and_then(|p| p.alt_baro.or(p.alt_gps))
        .unwrap_or(0);
    let gain_m = (alt_last - alt_first).max(0);

    // Unweighted centroid. Circle-fit would be more accurate for thermal
    // cores (PLAN.md §4) but mean is a reasonable Phase 1 starting point.
    let (lat_sum, lon_sum) = pts
        .iter()
        .fold((0.0_f64, 0.0_f64), |(la, lo), p| (la + p.lat, lo + p.lon));
    let centroid = (lat_sum / pts.len() as f64, lon_sum / pts.len() as f64);

    let start_time = pts.first().map(|p| p.time).unwrap_or_default();
    let end_time = pts.last().map(|p| p.time).unwrap_or_default();

    let _ = dt_s; // currently unused here, kept for future weighting
    ClimbSegment {
        flight_id: flight.id.clone(),
        start_time,
        end_time,
        avg_climb_ms,
        peak_climb_ms,
        gain_m,
        centroid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceKind, TrackPoint};
    use chrono::{Duration, NaiveDate};

    fn flight_with_altitudes(alts_baro: Vec<i32>, climb_at: (usize, usize)) -> Flight {
        // 1Hz, lat/lon constant (climb-in-place). alt goes up between
        // climb_at indices at ~2 m/s, otherwise flat.
        let base = NaiveDate::from_ymd_opt(2025, 7, 20)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let n = alts_baro.len();
        let points: Vec<TrackPoint> = (0..n)
            .map(|i| {
                let alt = if i >= climb_at.0 && i <= climb_at.1 {
                    // 2 m/s climb over the climb window
                    alts_baro[climb_at.0] + ((i - climb_at.0) as i32) * 2
                } else {
                    alts_baro[i]
                };
                TrackPoint {
                    time: base + Duration::seconds(i as i64),
                    lat: 46.0,
                    lon: 8.0,
                    alt_baro: Some(alt),
                    alt_gps: None,
                }
            })
            .collect();
        Flight {
            id: "local:climb.igc".into(),
            pilot: None,
            site: None,
            points,
            source: SourceKind::Local,
        }
    }

    #[test]
    fn detects_single_thermal() {
        // 60s flat → 30s of 2 m/s climb → 30s flat.
        let mut alts = vec![1000_i32; 60];
        alts.extend(vec![1000; 30]);
        alts.extend(vec![1600; 30]);
        let flight = flight_with_altitudes(alts, (60, 90));
        let config = ClimbConfig::default();
        let climbs = detect_climbs(&flight, &config);
        assert_eq!(climbs.len(), 1, "expected one climb segment");
        let c = &climbs[0];
        assert!(
            c.avg_climb_ms > 1.0,
            "avg climb should be ~2 m/s, got {}",
            c.avg_climb_ms
        );
        assert!(c.peak_climb_ms > 1.5);
        assert!(c.gain_m >= 50, "gain should be ~60m, got {}", c.gain_m);
    }

    #[test]
    fn ignores_short_bumps() {
        // 1s of 5 m/s climb is below min_duration_s.
        let mut alts = vec![1000_i32; 30];
        alts.push(1005); // 1s spike
        alts.extend(vec![1005; 30]);
        let flight = flight_with_altitudes(alts, (30, 30));
        let config = ClimbConfig::default();
        let climbs = detect_climbs(&flight, &config);
        assert!(climbs.is_empty(), "should have ignored the 1s bump");
    }

    #[test]
    fn empty_for_flight_without_altitudes() {
        let flight = Flight {
            id: "local:noalt.igc".into(),
            pilot: None,
            site: None,
            points: vec![
                TrackPoint {
                    time: NaiveDate::from_ymd_opt(2025, 1, 1)
                        .unwrap()
                        .and_hms_opt(0, 0, 0)
                        .unwrap(),
                    lat: 46.0,
                    lon: 8.0,
                    alt_baro: None,
                    alt_gps: None,
                },
                TrackPoint {
                    time: NaiveDate::from_ymd_opt(2025, 1, 1)
                        .unwrap()
                        .and_hms_opt(0, 0, 1)
                        .unwrap(),
                    lat: 46.0,
                    lon: 8.0,
                    alt_baro: None,
                    alt_gps: None,
                },
            ],
            source: SourceKind::Local,
        };
        assert!(detect_climbs(&flight, &ClimbConfig::default()).is_empty());
    }
}

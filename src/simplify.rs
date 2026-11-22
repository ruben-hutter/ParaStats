//! Track simplification via Douglas–Peucker, applied in Web-Mercator metres
//! so the tolerance is true ground distance regardless of latitude. Done
//! *before* emitting GeoJSON so deck.gl's PathLayer doesn't choke on
//! hundreds of flights × thousands of fixes.
//!
//! Ported verbatim from flightmap `src/simplify.rs`.

use geo::{Coord, LineString, SimplifyIdx};

use crate::geo::latlon_to_webmercator;
use crate::model::Flight;

/// Simplify a flight's track to within `tolerance_m` metres on the ground.
/// Returns the indices of points that survive the simplification; useful if
/// you want to slice other per-point data (altitudes, climb segments) to
/// match.
pub fn simplify_indices(flight: &Flight, tolerance_m: f64) -> Vec<usize> {
    if flight.points.len() < 3 {
        return (0..flight.points.len()).collect();
    }

    // Project to Web-Mercator metres so the DP tolerance is a real distance.
    let coords: Vec<Coord> = flight
        .points
        .iter()
        .map(|p| {
            let (x, y) = latlon_to_webmercator(p.lat, p.lon);
            Coord { x, y }
        })
        .collect();
    let line = LineString::new(coords);

    line.simplify_idx(&tolerance_m)
}

/// Simplify a flight's track to within `tolerance_m` metres, returning a new
/// [`Flight`] with the surviving points (all per-point fields preserved).
pub fn simplify_flight(flight: &Flight, tolerance_m: f64) -> Flight {
    let idxs = simplify_indices(flight, tolerance_m);
    let points = idxs.into_iter().map(|i| flight.points[i].clone()).collect();
    Flight {
        id: flight.id.clone(),
        pilot: flight.pilot.clone(),
        site: flight.site.clone(),
        points,
        source: flight.source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SourceKind, TrackPoint};
    use chrono::{Duration, NaiveDate};

    fn synthetic_flight(n: usize) -> Flight {
        let base = NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let points: Vec<TrackPoint> = (0..n)
            .map(|i| TrackPoint {
                time: base + Duration::seconds(i as i64),
                lat: 46.0 + (i as f64) * 0.0001, // ~11m steps north
                lon: 8.0,
                alt_baro: Some(1000 + i as i32),
                alt_gps: Some(1000 + i as i32),
            })
            .collect();
        Flight {
            id: "local:test.igc".into(),
            pilot: None,
            site: None,
            points,
            source: SourceKind::Local,
        }
    }

    #[test]
    fn simplify_keeps_endpoints() {
        let f = synthetic_flight(100);
        let idxs = simplify_indices(&f, 5.0);
        assert_eq!(*idxs.first().unwrap(), 0);
        assert_eq!(*idxs.last().unwrap(), 99);
    }

    #[test]
    fn simplify_collapses_straight_line_to_endpoints() {
        // All points along a perfect meridian at the same lon — DP should
        // collapse everything except the endpoints when tolerance exceeds
        // the spacing.
        let f = synthetic_flight(50);
        let idxs = simplify_indices(&f, 50.0); // 50 m tolerance, 11 m spacing
        assert!(idxs.len() < 5, "got {} indices", idxs.len());
    }

    #[test]
    fn simplify_preserves_short_flights_verbatim() {
        let f = synthetic_flight(2);
        let simplified = simplify_flight(&f, 5.0);
        assert_eq!(simplified.points.len(), 2);
    }
}

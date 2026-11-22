//! Mercator-grid binning + GeoJSON layer products.
//!
//! Ported from flightmap `src/bin.rs` + `src/aggregate.rs`. Two changes for
//! parastats:
//!
//!  * the skyway/thermal GeoJSON products are built from *database rows*
//!    (simplified tracks and climbs stored at ingest), not from freshly
//!    parsed `Flight`s — the API serves them directly;
//!  * the thermal layer endpoint returns **binned cells** (count, avg/peak
//!    climb, gain per cell). The frontend density canvas places one Gaussian
//!    kernel per cell weighted by `peak_climb_ms` — at 150 m cells the
//!    difference from per-climb kernels is invisible under the blur, and the
//!    payload is an order of magnitude smaller for a busy season.
//!
//! Why pre-aggregated cells instead of deck.gl's `HeatmapLayer`: a
//! per-frame density recompute lags on large climb sets, and a
//! `ScatterPlotLayer` on raw cells loses the gradient look. Binning here +
//! density-canvas client-side gives smooth rendering with zero per-frame
//! work (flightmap decision, kept).

use std::collections::HashMap;

use geojson::{Feature, FeatureCollection, Geometry, Value};

use crate::geo::{latlon_to_webmercator, webmercator_to_latlon};
use crate::model::ClimbSegment;

/// A single aggregated grid cell.
#[derive(Debug, Clone, PartialEq)]
pub struct BinnedCell {
    pub centroid_lat: f64,
    pub centroid_lon: f64,
    pub count: u32,
    /// Mean climb rate over all climbs in this cell, m/s.
    pub avg_climb_ms: f32,
    /// Peak climb rate observed in this cell, m/s.
    pub peak_climb_ms: f32,
    /// Sum of altitude gained across all climbs in this cell, metres.
    pub total_gain_m: i64,
}

#[derive(Debug)]
struct Accumulator {
    cell_key: (i64, i64),
    cell_size_m: f64,
    count: u32,
    weight_sum: f32,
    peak_climb_ms: f32,
    total_gain_m: i64,
}

impl Accumulator {
    fn new(cell_key: (i64, i64), cell_size_m: f64) -> Self {
        Self {
            cell_key,
            cell_size_m,
            count: 0,
            weight_sum: 0.0,
            peak_climb_ms: f32::NEG_INFINITY,
            total_gain_m: 0,
        }
    }

    fn add(&mut self, c: &ClimbSegment) {
        self.count += 1;
        self.weight_sum += c.avg_climb_ms;
        if c.peak_climb_ms > self.peak_climb_ms {
            self.peak_climb_ms = c.peak_climb_ms;
        }
        self.total_gain_m += c.gain_m as i64;
    }

    fn finalize(self) -> BinnedCell {
        // Cell centre = floor(key * size) + size/2.
        let center_x = (self.cell_key.0 as f64 * self.cell_size_m) + self.cell_size_m / 2.0;
        let center_y = (self.cell_key.1 as f64 * self.cell_size_m) + self.cell_size_m / 2.0;
        let (lat, lon) = webmercator_to_latlon(center_x, center_y);
        BinnedCell {
            centroid_lat: lat,
            centroid_lon: lon,
            count: self.count,
            avg_climb_ms: if self.count > 0 {
                self.weight_sum / self.count as f32
            } else {
                0.0
            },
            peak_climb_ms: self.peak_climb_ms,
            total_gain_m: self.total_gain_m,
        }
    }
}

/// Bin a slice of [`ClimbSegment`]s into a Mercator grid of `cell_size_m`
/// metres. Cells with one or more climbs are returned; empty cells are
/// omitted (sparse encoding — fine for typical flight densities).
pub fn bin_climbs(climbs: &[ClimbSegment], cell_size_m: f64) -> Vec<BinnedCell> {
    let mut bins: HashMap<(i64, i64), Accumulator> = HashMap::new();
    for c in climbs {
        let (x, y) = latlon_to_webmercator(c.centroid.0, c.centroid.1);
        let key = (
            (x / cell_size_m).floor() as i64,
            (y / cell_size_m).floor() as i64,
        );
        bins.entry(key)
            .or_insert_with(|| Accumulator::new(key, cell_size_m))
            .add(c);
    }
    bins.into_values().map(|a| a.finalize()).collect()
}

/// Default cell size: 150 m. Empirically a good balance — fine enough to
/// separate neighbouring thermals, coarse enough to aggregate a season's
/// flights into a usable density signal.
pub const DEFAULT_CELL_SIZE_M: f64 = 150.0;

/// One decoded track row: `(unix_ts, lat, lon, alt_m)`. This is the msgpack
/// shape stored in `flights.track_simplified` / `track_full`.
pub type TrackRow = (i64, f64, f64, i32);

/// Skyway GeoJSON: one LineString feature per flight, vertices `[lon, lat, alt]`.
/// The third coordinate lets the frontend colour per-vertex by altitude
/// without a separate property array (flightmap pattern, kept).
pub fn skyway_feature(id: &str, date_local: &str, start_utc: &str, track: &[TrackRow]) -> Feature {
    let coords: Vec<Vec<f64>> = track
        .iter()
        .map(|&(_, lat, lon, alt)| vec![lon, lat, alt as f64])
        .collect();
    Feature {
        bbox: None,
        geometry: Some(Geometry::new(Value::LineString(coords))),
        id: None,
        properties: Some(
            serde_json::json!({
                "id": id,
                "date": date_local,
                "start": start_utc,
                "points": track.len(),
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        foreign_members: None,
    }
}

/// Thermal density GeoJSON: one Point feature per non-empty Mercator grid
/// cell, with the aggregated climb properties.
pub fn thermal_density_collection(cells: &[BinnedCell]) -> FeatureCollection {
    let features: Vec<Feature> = cells.iter().map(cell_to_density_feature).collect();
    FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }
}

fn cell_to_density_feature(c: &BinnedCell) -> Feature {
    Feature {
        bbox: None,
        geometry: Some(Geometry::new(Value::Point(vec![
            c.centroid_lon,
            c.centroid_lat,
        ]))),
        id: None,
        properties: Some(
            serde_json::json!({
                "count": c.count,
                "avg_climb_ms": c.avg_climb_ms,
                "peak_climb_ms": c.peak_climb_ms,
                "total_gain_m": c.total_gain_m,
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
        foreign_members: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn climb_at(lat: f64, lon: f64, rate: f32, gain: i32) -> ClimbSegment {
        ClimbSegment {
            flight_id: "local:t.igc".into(),
            start_time: NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            end_time: NaiveDate::from_ymd_opt(2025, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            avg_climb_ms: rate,
            peak_climb_ms: rate * 1.5,
            gain_m: gain,
            centroid: (lat, lon),
        }
    }

    #[test]
    fn nearby_climbs_collapse_into_one_cell() {
        // Two climbs within a few metres of each other must bin together.
        let climbs = vec![
            climb_at(46.0, 8.0, 1.5, 100),
            climb_at(46.0001, 8.0001, 2.5, 200), // ~13 m away
        ];
        let cells = bin_climbs(&climbs, 150.0);
        assert_eq!(cells.len(), 1);
        let c = &cells[0];
        assert_eq!(c.count, 2);
        assert!((c.avg_climb_ms - 2.0).abs() < 1e-3);
        assert!((c.peak_climb_ms - 3.75).abs() < 1e-3); // max peak (1.5*1.5=2.25, 2.5*1.5=3.75)
        assert_eq!(c.total_gain_m, 300);
    }

    #[test]
    fn distant_climbs_stay_separate() {
        let climbs = vec![
            climb_at(46.0, 8.0, 1.5, 100),
            climb_at(46.5, 8.5, 2.5, 200), // ~60 km away
        ];
        let cells = bin_climbs(&climbs, 150.0);
        assert_eq!(cells.len(), 2);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let cells = bin_climbs(&[], 150.0);
        assert!(cells.is_empty());
    }

    #[test]
    fn density_collection_round_trips_through_json() {
        let climbs = [climb_at(46.0, 8.0, 2.0, 300)];
        let cells = bin_climbs(&climbs, DEFAULT_CELL_SIZE_M);
        let collection = thermal_density_collection(&cells);
        let json = serde_json::to_string(&collection).unwrap();
        assert!(json.contains("\"FeatureCollection\""));
        assert!(json.contains("\"peak_climb_ms\""));
    }

    #[test]
    fn skyway_feature_emits_altitude_vertices() {
        let track: Vec<TrackRow> = vec![(0, 46.0, 8.0, 1200), (1, 46.001, 8.001, 1300)];
        let f = skyway_feature("abc", "2025-07-20", "2025-07-20T12:00:00", &track);
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["geometry"]["type"], "LineString");
        assert_eq!(json["geometry"]["coordinates"][0][2].as_f64(), Some(1200.0));
        assert_eq!(json["properties"]["id"], "abc");
    }
}

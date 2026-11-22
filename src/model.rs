//! Core data model for parastats.
//!
//! Ported from flightmap `src/model.rs` (the id/prefix convention and the
//! TrackPoint/Flight/ClimbSegment shape are unchanged — battle-tested
//! against 245 real flights). Extended with `site` (from the `HFSITSITE`
//! IGC header) and a `FlightMeta` for the ingest pipeline.

use chrono::NaiveDateTime;

/// Where a [`Flight`] came from. Drives the `Flight::id` prefix convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Local,
    XContest,
}

impl SourceKind {
    pub fn prefix(self) -> &'static str {
        match self {
            SourceKind::Local => "local",
            SourceKind::XContest => "xcontest",
        }
    }
}

/// One GPS/baro fix in a [`Flight`]. Times are always UTC (IGC is UTC-native).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    pub time: NaiveDateTime,
    pub lat: f64,
    pub lon: f64,
    /// GPS altitude, metres MSL. Optional — many older varios only log baro.
    pub alt_gps: Option<i32>,
    /// Pressure/barometric altitude, metres. Preferred for climb-rate math.
    pub alt_baro: Option<i32>,
}

impl TrackPoint {
    /// Baro-preferred altitude, GPS fallback. This is the series used for
    /// stats/climb math when a single value is needed.
    pub fn alt(&self) -> Option<i32> {
        self.alt_baro.or(self.alt_gps)
    }
}

/// A single flight, parsed from one IGC file or one XContest entry.
#[derive(Debug, Clone)]
pub struct Flight {
    /// Prefixed id: `local:foo.igc` or `xcontest:1234` (see [`SourceKind::prefix`]).
    pub id: String,
    pub pilot: Option<String>,
    /// Site name from the `HFSITSITE` IGC header, when present.
    pub site: Option<String>,
    pub points: Vec<TrackPoint>,
    pub source: SourceKind,
}

/// A continuous segment of circling climb in a [`Flight`].
///
/// Note: no `Vec<usize>` indices into the parent flight's points — the time
/// range + `flight_id` is enough for every downstream layer and keeps the
/// struct message-packable for the parse cache (flightmap decision, kept).
#[derive(Debug, Clone)]
pub struct ClimbSegment {
    pub flight_id: String,
    pub start_time: NaiveDateTime,
    pub end_time: NaiveDateTime,
    /// Average climb rate over the segment, m/s. Weight for the thermal layer.
    pub avg_climb_ms: f32,
    /// Peak instantaneous climb rate within the segment, m/s.
    pub peak_climb_ms: f32,
    /// Total altitude gained, metres.
    pub gain_m: i32,
    /// Segment centroid (lat, lon). Mean-of-fixes; circle-fit is a possible
    /// refinement.
    pub centroid: (f64, f64),
}

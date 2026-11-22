//! parastats — self-hosted paragliding flight stats (map + tracks + stats)
//! from IGC logs.
//!
//! Absorbs flightmap's proven core (IGC parsing, climb detection, track
//! simplification, Mercator binning — see the module doc comments for the
//! per-module reasoning) and adds the parastats pipeline: stats, SQLite
//! persistence, ingest and an axum API serving a Svelte SPA.

pub mod aggregate;
pub mod api;
pub mod climb;
pub mod db;
pub mod geo;
pub mod igc;
pub mod ingest;
pub mod model;
pub mod simplify;
pub mod sites;
pub mod stats;

pub use aggregate::{bin_climbs, BinnedCell, DEFAULT_CELL_SIZE_M};
pub use climb::{detect_climbs, ClimbConfig};
pub use igc::{parse_igc, IgcError};
pub use model::{ClimbSegment, Flight, SourceKind, TrackPoint};
pub use simplify::simplify_flight;
pub use sites::{
    backfill_missing, nearest_site, Overpass, SiteSource, TakeoffSite, RESOLVE_RADIUS_M,
};
pub use stats::summarize;

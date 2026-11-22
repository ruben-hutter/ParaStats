//! Takeoff site resolution from an external site database (issue #5).
//!
//! Flights recorded without a site header (mostly XCTrack on a phone) get
//! their site from the **nearest cached external takeoff site** within
//! [`RESOLVE_RADIUS_M`] of the flight's first fix. The external source is
//! pluggable ([`SiteSource`]); the first implementation is OpenStreetMap via
//! Overpass, where Swiss launches are tagged `sport=free_flying`
//! (± `free_flying:site=takeoff`).
//!
//! The site list is fetched on demand and **cached wholesale in the
//! `takeoff_sites` SQLite table** (TTL [`SITE_CACHE_TTL`]) — network lookups
//! never happen per-flight on the ingest hot path.
//!
//! Resolution order per flight (see `ingest` and [`backfill_missing`]):
//! `HFSITSITE`/`HOSITSite` header → external lookup → stay NULL (honest)
//! → UI filename fallback.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{NaiveDateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::geo::haversine_approx_m;

/// Max distance between a flight's takeoff fix and an external site for the
/// site to be adopted. 300 m, validated against the 117 header-labeled
/// flights: nearest-site name agrees everywhere, zero wrong-site matches;
/// beyond ~150 m the match count barely moves (88 → 89 of 159).
pub const RESOLVE_RADIUS_M: f64 = 300.0;

/// How long the cached site list is trusted before the next resolution run
/// refetches it (one Overpass call per week, not per flight).
pub const SITE_CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 3600);

/// Search area: Switzerland plus a border strip (bbox south,west,north,east).
/// A bbox instead of `ISO3166-1=CH` also covers border launches and is
/// cheaper on every Overpass instance. Validated 2026-11: 509 named takeoff
/// sites vs 430 for the CH area alone.
pub const OVERPASS_BBOX: &str = "45.8,5.8,47.9,10.6";

/// Overpass QL for the takeoff sites. Covers the modern `sport=free_flying`
/// scheme plus the legacy `paragliding=takeoff` / `free_flying=takeoff`
/// forms (near-zero coverage in CH, included for free). Ways and relations
/// come back as centroids (`out center`).
pub const OVERPASS_QUERY: &str = "[out:json][timeout:180];\n(\n  nwr[\"sport\"=\"free_flying\"](BBOX);\n  nwr[\"free_flying\"=\"takeoff\"](BBOX);\n  nwr[\"paragliding\"=\"takeoff\"](BBOX);\n);\nout center tags;\n";

/// `meta` key holding the ISO instant of the last successful site fetch.
pub const META_SITES_FETCHED_AT: &str = "sites_fetched_at";

/// `flights.site_source` value for sites resolved from the external source
/// (the OSM cache). Centralises the vocabulary shared by ingest, backfill
/// and [`SiteSource::source_name`].
pub const SITE_SOURCE_OSM: &str = "osm";

/// Public Overpass instances tried in order; instances are routinely
/// rate-limited (observed: 406/502/timeouts during research). Overridden
/// entirely by `PARASTATS_OVERPASS_URL`.
pub const DEFAULT_ENDPOINTS: &[&str] = &[
    "https://overpass-api.de/api/interpreter",
    "https://overpass.osm.ch/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
];

/// One cached external takeoff site.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeoffSite {
    /// OSM element type: `node` | `way` | `relation`.
    pub osm_type: String,
    pub osm_id: i64,
    /// Display name (OSM `name`, falling back to `name:de`/`:fr`/`:it`).
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// Pluggable external source of takeoff sites. Implementations fetch the
/// full list; [`refresh_cache`] persists it wholesale. Fetching does
/// blocking network I/O — call sites move the source into
/// `tokio::task::spawn_blocking`.
pub trait SiteSource: Send + Sync {
    /// Stable identifier stored as `flights.site_source` (e.g. `"osm"`).
    fn source_name(&self) -> &'static str;
    /// Fetch all takeoff sites the source knows for the search area.
    fn fetch(&self) -> Result<Vec<TakeoffSite>>;
}

/// External takeoff sites from OpenStreetMap via the Overpass API.
pub struct Overpass {
    /// Single interpreter URL (usually from `PARASTATS_OVERPASS_URL`);
    /// `None` → try [`DEFAULT_ENDPOINTS`] in order.
    endpoint: Option<String>,
}

impl Overpass {
    pub fn new(endpoint: Option<String>) -> Self {
        Self { endpoint }
    }

    /// From env `PARASTATS_OVERPASS_URL` if set, else the mirror list.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("PARASTATS_OVERPASS_URL")
                .ok()
                .filter(|s| !s.is_empty()),
        )
    }
}

impl SiteSource for Overpass {
    fn source_name(&self) -> &'static str {
        SITE_SOURCE_OSM
    }

    fn fetch(&self) -> Result<Vec<TakeoffSite>> {
        let query = OVERPASS_QUERY.replace("BBOX", OVERPASS_BBOX);
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("parastats/", env!("CARGO_PKG_VERSION")))
            .build();
        let endpoints: Vec<String> = match &self.endpoint {
            Some(url) => vec![url.clone()],
            None => DEFAULT_ENDPOINTS.iter().map(|s| s.to_string()).collect(),
        };
        let mut last_err: Option<String> = None;
        for url in &endpoints {
            let attempt: std::result::Result<String, String> = (|| {
                let resp = agent
                    .post(url)
                    .send_form(&[("data", query.as_str())])
                    .map_err(|e| e.to_string())?;
                resp.into_string().map_err(|e| e.to_string())
            })();
            match attempt {
                Ok(body) => {
                    let sites = parse_overpass(&body)
                        .with_context(|| format!("parsing Overpass response from {url}"))?;
                    if sites.is_empty() {
                        bail!("Overpass response from {url} contained no takeoff sites");
                    }
                    return Ok(sites);
                }
                Err(msg) => {
                    eprintln!("warn: overpass {url}: {msg}; trying next mirror");
                    last_err = Some(msg);
                }
            }
        }
        bail!(
            "all Overpass endpoints failed (last: {})",
            last_err.unwrap_or_else(|| "none".into())
        )
    }
}

/// Parse an Overpass JSON response (`out center tags`) into takeoff sites.
///
/// Skips unnamed elements, elements without coordinates, elements explicitly
/// tagged `free_flying:site=landing` (this resolver matches takeoffs), and
/// duplicates across the tag schemes.
pub fn parse_overpass(body: &str) -> Result<Vec<TakeoffSite>> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let elements = v
        .get("elements")
        .and_then(|e| e.as_array())
        .context("Overpass response has no elements array")?;

    let mut sites = Vec::new();
    let mut seen: HashSet<(String, i64)> = HashSet::new();
    for e in elements {
        let tags = e.get("tags");
        let tag =
            |key: &str| -> Option<&str> { tags.and_then(|t| t.get(key)).and_then(|v| v.as_str()) };

        // A landing site must not win a takeoff match — in any of the tag
        // schemes the query collects.
        if matches!(
            tag("free_flying:site").or_else(|| tag("free_flying")),
            Some("landing")
        ) || tag("paragliding") == Some("landing")
        {
            continue;
        }
        let Some(name) = ["name", "name:de", "name:fr", "name:it", "alt_name"]
            .iter()
            .find_map(|k| tag(k))
            .map(str::to_string)
        else {
            continue;
        };

        // Nodes carry lat/lon; ways and relations carry a centroid.
        let coords = e
            .get("center")
            .map(|c| {
                (
                    c.get("lat").and_then(|v| v.as_f64()),
                    c.get("lon").and_then(|v| v.as_f64()),
                )
            })
            .unwrap_or_else(|| {
                (
                    e.get("lat").and_then(|v| v.as_f64()),
                    e.get("lon").and_then(|v| v.as_f64()),
                )
            });
        let (Some(lat), Some(lon)) = coords else {
            continue;
        };

        let Some(osm_type) = e.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(osm_id) = e.get("id").and_then(|v| v.as_i64()) else {
            continue;
        };
        if !seen.insert((osm_type.to_string(), osm_id)) {
            continue; // element matched by more than one tag scheme
        }
        sites.push(TakeoffSite {
            osm_type: osm_type.to_string(),
            osm_id,
            name,
            lat,
            lon,
        });
    }
    Ok(sites)
}

/// Nearest site within `radius_m` of `(lat, lon)`, or `None`. Ties break to
/// the first site in slice order (the cache is loaded in a stable order).
pub fn nearest_site(
    lat: f64,
    lon: f64,
    radius_m: f64,
    sites: &[TakeoffSite],
) -> Option<&TakeoffSite> {
    sites
        .iter()
        .map(|s| (haversine_approx_m(lat, lon, s.lat, s.lon), s))
        .filter(|(d, _)| *d <= radius_m)
        .min_by(|a, b| a.0.partial_cmp(&b.0).expect("distances are finite"))
        .map(|(_, s)| s)
}

/// The takeoff-site adoption rule in one place: the nearest cached site
/// within [`RESOLVE_RADIUS_M`], used by both ingest and [`backfill_missing`].
/// Callers store the result with `site_source = SITE_SOURCE_OSM`.
pub fn resolve_takeoff(lat: f64, lon: f64, sites: &[TakeoffSite]) -> Option<&TakeoffSite> {
    nearest_site(lat, lon, RESOLVE_RADIUS_M, sites)
}

// ---------------------------------------------------------------------------
// SQLite cache
// ---------------------------------------------------------------------------

/// Cached sites, in stable `(osm_type, osm_id)` order.
pub async fn cached_sites(pool: &SqlitePool) -> Result<Vec<TakeoffSite>> {
    let rows = sqlx::query(
        "SELECT osm_type, osm_id, name, lat, lon FROM takeoff_sites ORDER BY osm_type, osm_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TakeoffSite {
            osm_type: r.get::<String, _>("osm_type"),
            osm_id: r.get::<i64, _>("osm_id"),
            name: r.get::<String, _>("name"),
            lat: r.get::<f64, _>("lat"),
            lon: r.get::<f64, _>("lon"),
        })
        .collect())
}

/// ISO instant of the last successful refresh, if any.
async fn cached_fetched_at(pool: &SqlitePool) -> Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM meta WHERE key = ?")
        .bind(META_SITES_FETCHED_AT)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<String, _>("value")))
}

async fn cache_is_fresh(pool: &SqlitePool) -> Result<bool> {
    match cached_fetched_at(pool).await? {
        Some(iso) => match NaiveDateTime::parse_from_str(&iso, "%Y-%m-%dT%H:%M:%S") {
            Ok(t) => Ok((Utc::now() - t.and_utc()).num_seconds() < SITE_CACHE_TTL.as_secs() as i64),
            Err(_) => Ok(false), // unreadable stamp → refetch
        },
        None => Ok(false),
    }
}

/// Fetch `source` (off the async runtime — blocking I/O) and replace the
/// cache wholesale in one transaction.
pub async fn refresh_cache<S: SiteSource + 'static>(
    pool: &SqlitePool,
    source: S,
) -> Result<Vec<TakeoffSite>> {
    let sites = tokio::task::spawn_blocking(move || source.fetch())
        .await
        .context("site source fetch panicked")??;

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM takeoff_sites")
        .execute(&mut *tx)
        .await?;
    for s in &sites {
        sqlx::query(
            "INSERT INTO takeoff_sites (osm_type, osm_id, name, lat, lon) VALUES (?,?,?,?,?)",
        )
        .bind(&s.osm_type)
        .bind(s.osm_id)
        .bind(&s.name)
        .bind(s.lat)
        .bind(s.lon)
        .execute(&mut *tx)
        .await?;
    }
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(META_SITES_FETCHED_AT)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(sites)
}

/// Cached sites, refreshed when stale (empty cache, unreadable/old stamp, or
/// `force_refresh`). On a failed refresh with a non-empty cache, keeps the
/// stale cache with a warning; with an empty cache, errors.
pub async fn ensure_cache(pool: &SqlitePool, force_refresh: bool) -> Result<Vec<TakeoffSite>> {
    let cached = cached_sites(pool).await?;
    if !force_refresh && !cached.is_empty() && cache_is_fresh(pool).await? {
        return Ok(cached);
    }
    match refresh_cache(pool, Overpass::from_env()).await {
        Ok(sites) => Ok(sites),
        Err(e) if !cached.is_empty() => {
            eprintln!("warn: takeoff site refresh failed ({e:#}); using stale cache");
            Ok(cached)
        }
        Err(e) => bail!("no cached takeoff sites and refresh failed: {e:#}"),
    }
}

/// Sites for the ingest path: like [`ensure_cache`], but a total failure
/// just warns and yields an empty list — ingest proceeds, sites stay NULL.
pub async fn sites_for_resolution(pool: &SqlitePool) -> Vec<TakeoffSite> {
    match ensure_cache(pool, false).await {
        Ok(sites) => sites,
        Err(e) => {
            eprintln!("warn: no takeoff sites available; site stays NULL: {e:#}");
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Backfill
// ---------------------------------------------------------------------------

/// Outcome of a [`backfill_missing`] run.
#[derive(Debug, Default, PartialEq)]
pub struct BackfillSummary {
    /// Sites in the (possibly refreshed) cache.
    pub sites: usize,
    /// Flights examined (`site IS NULL`).
    pub considered: usize,
    /// Flights that got a site from the external source.
    pub resolved: usize,
    /// Flights whose takeoff matched no site within [`RESOLVE_RADIUS_M`].
    pub unmatched: usize,
    /// Flights without takeoff coordinates.
    pub no_coords: usize,
}

/// Resolve `site IS NULL` flights against the external source (cache
/// refreshed when stale; `force_refresh` to refetch now). Never touches
/// flights that already have a site — an existing device-provided name
/// always wins. Existing rows only: no IGC files are re-read.
pub async fn backfill_missing(pool: &SqlitePool, force_refresh: bool) -> Result<BackfillSummary> {
    let sites = ensure_cache(pool, force_refresh).await?;
    let rows = sqlx::query(
        "SELECT id, takeoff_lat, takeoff_lon FROM flights WHERE site IS NULL ORDER BY takeoff_time_utc",
    )
    .fetch_all(pool)
    .await?;

    let mut summary = BackfillSummary {
        sites: sites.len(),
        considered: rows.len(),
        ..BackfillSummary::default()
    };
    for r in rows {
        let id: String = r.get("id");
        let lat: Option<f64> = r.try_get("takeoff_lat").unwrap_or(None);
        let lon: Option<f64> = r.try_get("takeoff_lon").unwrap_or(None);
        match (lat, lon) {
            (Some(lat), Some(lon)) => match resolve_takeoff(lat, lon, &sites) {
                Some(s) => {
                    sqlx::query("UPDATE flights SET site = ?, site_source = ? WHERE id = ?")
                        .bind(&s.name)
                        .bind(SITE_SOURCE_OSM)
                        .bind(&id)
                        .execute(pool)
                        .await?;
                    summary.resolved += 1;
                }
                None => summary.unmatched += 1,
            },
            _ => summary.no_coords += 1,
        }
    }
    Ok(summary)
}

/// Test helper: load `tests/fixtures/overpass_takeoffs.json` into the cache
/// with a fresh timestamp, so [`ensure_cache`] skips the network entirely.
#[cfg(test)]
pub(crate) async fn seed_cache_from_fixture(pool: &SqlitePool) -> Vec<TakeoffSite> {
    let sites = fixture_sites_from_disk();
    let mut tx = pool.begin().await.unwrap();
    for s in &sites {
        sqlx::query("INSERT OR REPLACE INTO takeoff_sites (osm_type, osm_id, name, lat, lon) VALUES (?,?,?,?,?)")
            .bind(&s.osm_type).bind(s.osm_id).bind(&s.name).bind(s.lat).bind(s.lon)
            .execute(&mut *tx).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(META_SITES_FETCHED_AT)
    .bind(Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    sites
}

#[cfg(test)]
fn fixture_sites_from_disk() -> Vec<TakeoffSite> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/overpass_takeoffs.json");
    let body = std::fs::read_to_string(path).unwrap();
    parse_overpass(&body).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_sites() -> Vec<TakeoffSite> {
        fixture_sites_from_disk()
    }

    #[test]
    fn parses_fixture_overpass_response() {
        let sites = fixture_sites();
        let names: Vec<&str> = sites.iter().map(|s| s.name.as_str()).collect();
        // Order follows the fixture's element order; the landing site and
        // the unnamed node are skipped (asserted in the next test).
        assert_eq!(
            names,
            vec![
                "Metschstand",            // node via lat/lon
                "Monte Lema",             // node via lat/lon
                "Alp Scheidegg",          // way via center, no takeoff subtag
                "First",                  // way via center
                "Metschstand alternativ", // name:de fallback
                "Lema S\u{fc}dstart",     // legacy paragliding=takeoff, node
            ]
        );
        assert!(sites.iter().all(|s| s.osm_id > 0));
    }

    #[test]
    fn landing_sites_and_unnamed_elements_are_skipped() {
        let sites = fixture_sites();
        assert!(!sites.iter().any(|s| s.name.contains("Landeplatz")));
        // The unnamed node (id 9900000003) is dropped.
        assert_eq!(sites.len(), 6);
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse_overpass("not json").is_err());
        assert!(parse_overpass(r#"{"nope":1}"#).is_err());
        // Valid but empty is fine (fetch() guards against empty results).
        assert_eq!(parse_overpass(r#"{"elements":[]}"#).unwrap(), vec![]);
    }

    #[test]
    fn nearest_site_matches_within_radius() {
        let sites = fixture_sites();
        // Monte Lema takeoff node: 46.0400447, 8.8317534 — a takeoff fix
        // ~45 m away.
        let near = nearest_site(46.0404, 8.8315, RESOLVE_RADIUS_M, &sites);
        assert_eq!(near.unwrap().name, "Monte Lema");

        // The buba.2026-10-11 flight (issue #5 done-when): takeoff
        // 46.4435, 7.49673 → Metschstand node ~144 m away.
        let m = nearest_site(46.4435, 7.49673, RESOLVE_RADIUS_M, &sites).unwrap();
        assert_eq!(m.name, "Metschstand");

        // Way centroids work: First (Grindelwald) center 46.6574328, 8.0551795.
        let f = nearest_site(46.6576, 8.0554, RESOLVE_RADIUS_M, &sites).unwrap();
        assert_eq!(f.name, "First");
    }

    #[test]
    fn nearest_site_returns_none_outside_radius() {
        let sites = fixture_sites();
        // Kandersteg village, ~2 km from Metschstand node.
        assert!(nearest_site(46.4950, 7.4900, RESOLVE_RADIUS_M, &sites).is_none());
        // Tight radius finds nothing at the buba flight either.
        assert!(nearest_site(46.4435, 7.49673, 100.0, &sites).is_none());
        assert!(nearest_site(46.4435, 7.49673, 150.0, &sites).is_some());
    }

    #[tokio::test]
    async fn cache_round_trip_and_backfill() {
        // File-backed temp db (pooled :memory: would give each connection
        // its own database).
        let dir = std::env::temp_dir().join(format!("parastats-sites-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = crate::db::open_db(dir.join("t.db").to_str().unwrap())
            .await
            .unwrap();

        // Seed the cache from the fixture (refresh_cache would hit the network).
        let sites = seed_cache_from_fixture(&pool).await;

        // Fresh cache → ensure_cache must not attempt a refresh (no network).
        let cached = ensure_cache(&pool, false).await.unwrap();
        assert_eq!(cached.len(), sites.len());

        // Three flights: one resolvable, one too far, one without coords.
        for (id, lat, lon) in [
            ("xctrack-metschstand", Some(46.4435), Some(7.49673)),
            ("far-away", Some(46.4950), Some(7.4900)),
            ("no-coords", None, None),
        ] {
            let insert = crate::db::FlightInsert {
                row: crate::db::FlightRow {
                    id: id.into(),
                    site: None,
                    site_source: None,
                    takeoff_lat: lat,
                    takeoff_lon: lon,
                    ..test_row()
                },
                track_simplified: vec![],
                track_full: vec![],
                climbs: vec![],
            };
            crate::db::upsert_flight(&pool, &insert).await.unwrap();
        }

        let summary = backfill_missing(&pool, false).await.unwrap();
        assert_eq!(
            summary,
            BackfillSummary {
                sites: sites.len(),
                considered: 3,
                resolved: 1,
                unmatched: 1,
                no_coords: 1
            }
        );

        let flights = crate::db::flights(&pool, None).await.unwrap();
        let m = flights
            .iter()
            .find(|f| f.id == "xctrack-metschstand")
            .unwrap();
        assert_eq!(m.site.as_deref(), Some("Metschstand"));
        assert_eq!(m.site_source.as_deref(), Some("osm"));
        let far = flights.iter().find(|f| f.id == "far-away").unwrap();
        assert_eq!(far.site, None);
        assert_eq!(far.site_source, None);

        // Backfill is idempotent: already-resolved flights are not re-examined.
        let again = backfill_missing(&pool, false).await.unwrap();
        assert_eq!(again.considered, 2); // far-away + no-coords
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Minimal valid `FlightRow` for tests (the flights table has many NOT
    /// NULL columns).
    fn test_row() -> crate::db::FlightRow {
        crate::db::FlightRow {
            id: "test".into(),
            source: "local".into(),
            filename: "t.igc".into(),
            date_utc: "2026-10-11".into(),
            takeoff_time_utc: "2026-10-11T11:40:33".into(),
            landing_time_utc: "2026-10-11T13:00:00".into(),
            duration_s: 4767,
            takeoff_lat: None,
            takeoff_lon: None,
            landing_lat: None,
            landing_lon: None,
            takeoff_alt_m: None,
            site: None,
            site_source: None,
            pilot: None,
            distance_track_m: 12_345.0,
            max_alt_m: 2000,
            min_alt_m: 1200,
            alt_gain_m: 800,
            max_climb_ms: 3.0,
            max_sink_ms: -4.0,
        }
    }
}

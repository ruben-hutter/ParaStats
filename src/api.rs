//! axum HTTP API + static SPA serving.
//!
//! All JSON endpoints live under `/api`. In production the SPA built into
//! `web/dist` is embedded into the binary via rust-embed (single deployable
//! artifact, no runtime filesystem dependency). In debug builds rust-embed
//! reads `web/dist` from disk so a `vite build` refreshes without a cargo
//! rebuild — the tradeoff (debug needs the files present, release doesn't)
//! is the right one for a homelab tool. Non-`/api` paths fall back to
//! `index.html` so client-side hash routing works on deep links.

use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::aggregate::{
    bin_climbs, skyway_feature, thermal_density_collection, BinnedCell, DEFAULT_CELL_SIZE_M,
};
use crate::db;
use crate::model::ClimbSegment;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

pub type SharedState = Arc<AppState>;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/api/flights", get(list_flights))
        .route("/api/flights/{id}", get(flight_detail))
        .route("/api/flights/{id}/track", get(flight_track))
        .route("/api/stats", get(stats))
        .route("/api/layers/skyway", get(layer_skyway))
        .route("/api/layers/thermals", get(layer_thermals))
        .route("/api/ingest", post(ingest))
        .fallback(static_handler)
        .with_state(state)
}

// ---- JSON API ----

#[derive(Deserialize)]
struct FlightsQuery {
    year: Option<i32>,
}

async fn list_flights(
    State(state): State<SharedState>,
    Query(q): Query<FlightsQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flights = db::flights(&state.pool, q.year).await?;
    Ok(Json(json!({ "flights": flights })))
}

async fn flight_detail(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let flight = db::flight(&state.pool, &id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no flight {id}")))?;
    let climbs = db::climbs_for(&state.pool, &id).await?;
    Ok(Json(json!({ "flight": flight, "climbs": climbs })))
}

async fn flight_track(
    State(state): State<SharedState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let blob = db::track_blob(&state.pool, &id, "track_full")
        .await?
        .ok_or_else(|| ApiError::not_found(format!("no flight {id}")))?;
    let track = db::decode_track(&blob)?;
    Ok(Json(json!({ "id": id, "track": track })))
}

async fn stats(State(state): State<SharedState>) -> Result<Json<serde_json::Value>, ApiError> {
    let flights = db::flights(&state.pool, None).await?;

    let total_flights = flights.len();
    let total_duration_s: i64 = flights.iter().map(|f| f.duration_s).sum();
    // NB: Rust's empty f64 sum is -0.0; normalise so an empty DB doesn't
    // serialize as "-0.0".
    let total_distance_m: f64 = flights.iter().map(|f| f.distance_track_m).sum::<f64>() + 0.0;
    let max_alt_m: i64 = flights.iter().map(|f| f.max_alt_m).max().unwrap_or(0);
    let total_alt_gain_m: i64 = flights.iter().map(|f| f.alt_gain_m).sum();
    let total_climb_ms: f64 = flights
        .iter()
        .map(|f| f.max_climb_ms)
        .fold(f64::NEG_INFINITY, f64::max);

    // Per-year breakdown keyed on the local (Europe/Zurich) takeoff date.
    let mut by_year: std::collections::BTreeMap<i32, serde_json::Map<String, serde_json::Value>> =
        std::collections::BTreeMap::new();
    for f in &flights {
        let year: i32 = f.date_utc[0..4].parse().unwrap_or(0);
        let entry = by_year.entry(year).or_insert_with(|| {
            json!({
                "year": year,
                "flights": 0,
                "duration_s": 0,
                "distance_m": 0.0,
                "max_alt_m": 0,
                "alt_gain_m": 0,
            })
            .as_object()
            .unwrap()
            .clone()
        });
        entry["flights"] = json!(entry["flights"].as_i64().unwrap_or(0) + 1);
        entry["duration_s"] = json!(entry["duration_s"].as_i64().unwrap_or(0) + f.duration_s);
        entry["distance_m"] =
            json!(entry["distance_m"].as_f64().unwrap_or(0.0) + f.distance_track_m);
        entry["max_alt_m"] = json!(entry["max_alt_m"].as_i64().unwrap_or(0).max(f.max_alt_m));
        entry["alt_gain_m"] = json!(entry["alt_gain_m"].as_i64().unwrap_or(0) + f.alt_gain_m);
    }

    Ok(Json(json!({
        "totals": {
            "flights": total_flights,
            "duration_s": total_duration_s,
            "distance_m": total_distance_m,
            "max_alt_m": max_alt_m,
            "alt_gain_m": total_alt_gain_m,
            "max_climb_ms": if total_flights == 0 { 0.0 } else { total_climb_ms },
        },
        "by_year": by_year.values().cloned().collect::<Vec<_>>(),
    })))
}

async fn layer_skyway(
    State(state): State<SharedState>,
) -> Result<Json<geojson::FeatureCollection>, ApiError> {
    let rows = db::simplified_tracks(&state.pool).await?;
    let features = rows
        .iter()
        .filter_map(|(id, date, start, blob)| {
            db::decode_track(blob)
                .ok()
                .map(|track| skyway_feature(id, date, start, &track))
        })
        .collect::<Vec<_>>();
    Ok(Json(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }))
}

async fn layer_thermals(
    State(state): State<SharedState>,
) -> Result<Json<geojson::FeatureCollection>, ApiError> {
    // Binned cells carry no timestamps; season/time-of-day filtering happens
    // on the skyway layer client-side (per-flight `start` property). The
    // thermal layer is a "where are my thermals" overview, not a filterable
    // series — binning erases per-climb times by design.
    let climbs = db::all_climbs(&state.pool).await?;
    let segments: Vec<ClimbSegment> = climbs
        .iter()
        .map(|c| ClimbSegment {
            flight_id: c.flight_id.clone(),
            start_time: chrono::NaiveDateTime::parse_from_str(
                &c.start_time_utc,
                "%Y-%m-%dT%H:%M:%S",
            )
            .unwrap_or_default(),
            end_time: chrono::NaiveDateTime::parse_from_str(&c.end_time_utc, "%Y-%m-%dT%H:%M:%S")
                .unwrap_or_default(),
            avg_climb_ms: c.avg_climb_ms as f32,
            peak_climb_ms: c.peak_climb_ms as f32,
            gain_m: c.gain_m as i32,
            centroid: (c.centroid_lat, c.centroid_lon),
        })
        .collect();
    let cells: Vec<BinnedCell> = bin_climbs(&segments, DEFAULT_CELL_SIZE_M);
    Ok(Json(thermal_density_collection(&cells)))
}

#[derive(Deserialize)]
struct IngestBody {
    folder: String,
    #[serde(default)]
    rebuild: bool,
}

async fn ingest(
    State(state): State<SharedState>,
    Json(body): Json<IngestBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let summary = crate::ingest::ingest_folder(
        &state.pool,
        std::path::Path::new(&body.folder),
        body.rebuild,
    )
    .await?;
    Ok(Json(json!({
        "scanned": summary.scanned,
        "stored": summary.stored,
        "replaced": summary.replaced,
        "skipped": summary.skipped,
        "failed": summary.failed,
        "failures": summary.failures.iter().map(|(p, e)| json!({"path": p.display().to_string(), "error": e})).collect::<Vec<_>>(),
    })))
}

// ---- Static assets ----

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "no such API endpoint").into_response();
    }

    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_for(path);
            ([(header::CONTENT_TYPE, mime)], file.data).into_response()
        }
        // SPA fallback: unknown non-API paths get index.html so hash routing
        // and refreshes work.
        None => match WebAssets::get("index.html") {
            Some(index) => Html(String::from_utf8_lossy(&index.data).into_owned()).into_response(),
            None => (
                StatusCode::SERVICE_UNAVAILABLE,
                "web/dist not built — run `just build-web`",
            )
                .into_response(),
        },
    }
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        "map" => "application/json",
        _ => "application/octet-stream",
    }
}

// ---- Error handling ----

struct ApiError {
    status: StatusCode,
    source: anyhow::Error,
}

impl ApiError {
    fn not_found(msg: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            source: anyhow::anyhow!(msg),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(source: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            source,
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

impl From<rmp_serde::decode::Error> for ApiError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": format!("{:#}", self.source) })),
        )
            .into_response()
    }
}

// ---- API smoke tests (router over a real temp db) ----

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt; // for .oneshot

    async fn test_state() -> SharedState {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("parastats-apitest-{}-{n}.db", std::process::id()));
        let pool = db::open_db(path.to_str().unwrap()).await.unwrap();
        // Seed the external takeoff-site cache so ingest doesn't hit the
        // network from tests.
        crate::sites::seed_cache_from_fixture(&pool).await;
        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        crate::ingest::ingest_folder(&pool, &fixtures, false)
            .await
            .unwrap();
        Arc::new(AppState { pool })
    }

    async fn get_json(state: &SharedState, uri: &str) -> serde_json::Value {
        let response = router(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "GET {uri}");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn flights_list_and_detail() {
        let state = test_state().await;
        let list = get_json(&state, "/api/flights").await;
        let flights = list["flights"].as_array().unwrap();
        assert_eq!(flights.len(), 3);

        // Detail/track assertions need a full-size flight: the skytraxx one
        // (the synthetic no-site fixture has only 5 fix records).
        let sky = flights
            .iter()
            .find(|f| f["filename"].as_str().unwrap().contains("skytraxx"))
            .expect("skytraxx fixture in list");
        let id = sky["id"].as_str().unwrap();
        let detail = get_json(&state, &format!("/api/flights/{id}")).await;
        assert_eq!(detail["flight"]["id"].as_str().unwrap(), id);
        assert!(detail["climbs"].is_array());

        let track = get_json(&state, &format!("/api/flights/{id}/track")).await;
        let points = track["track"].as_array().unwrap();
        assert!(
            points.len() > 100,
            "full-res track expected, got {}",
            points.len()
        );
        assert_eq!(points[0].as_array().unwrap().len(), 4);

        let missing = router(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/flights/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stats_and_layers() {
        let state = test_state().await;
        let stats = get_json(&state, "/api/stats").await;
        assert_eq!(stats["totals"]["flights"].as_i64().unwrap(), 3);
        assert!(stats["totals"]["duration_s"].as_i64().unwrap() > 0);
        assert!(!stats["by_year"].as_array().unwrap().is_empty());

        let skyway = get_json(&state, "/api/layers/skyway").await;
        assert_eq!(skyway["type"], "FeatureCollection");
        assert_eq!(skyway["features"].as_array().unwrap().len(), 3);

        let thermals = get_json(&state, "/api/layers/thermals").await;
        assert_eq!(thermals["type"], "FeatureCollection");
        // Monte Lema fixtures contain climbs; at least one cell must exist.
        assert!(!thermals["features"].as_array().unwrap().is_empty());
    }
}

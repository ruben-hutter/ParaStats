//! Cheap geo helpers. No PROJ, no UTM — Web-Mercator + equirectangular
//! approximation is good to < 0.5 % at paragliding latitudes. (Deliberate
//! flightmap decision, kept: avoiding the PROJ C dependency makes builds
//! trivial everywhere; paragliding latitudes don't need better.)
//!
//! Ported verbatim from flightmap `src/geo.rs`.
//! All functions take/return degrees for lat/lon and metres for distances.

/// Web-Mercator forward: (lat, lon) in degrees → (x, y) in metres on a
/// sphere of radius `R = 6 378 137 m` (the EPSg:3857 sphere).
pub fn latlon_to_webmercator(lat_deg: f64, lon_deg: f64) -> (f64, f64) {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    let r = 6_378_137.0_f64;
    let x = r * lon;
    // Clamp latitude to avoid infinity at the poles — paraglider flights
    // never get there but defensive coding costs nothing.
    let clamped = lat.clamp(-1.483_529_864_195_180_7, 1.483_529_864_195_180_7);
    let y = r * clamped.tan().asinh();
    (x, y)
}

/// Web-Mercator inverse: (x, y) in metres → (lat, lon) in degrees.
pub fn webmercator_to_latlon(x: f64, y: f64) -> (f64, f64) {
    let r = 6_378_137.0_f64;
    let lon = (x / r).to_degrees();
    let lat = (y / r).sinh().atan().to_degrees();
    (lat, lon)
}

/// Equirectangular distance between two (lat, lon) points in metres. Good
/// enough for climb-rate and short-distance work; the error at paraglider
/// latitudes is sub-percent.
pub fn haversine_approx_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let mean_lat = ((lat1 + lat2) / 2.0).to_radians();
    let dlat = (lat2 - lat1) * 111_320.0;
    let dlon = (lon2 - lon1) * 111_320.0 * mean_lat.cos();
    (dlat * dlat + dlon * dlon).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webmercator_round_trip_is_stable() {
        let (lat, lon) = (46.175, 8.985); // Locarno-ish
        let (x, y) = latlon_to_webmercator(lat, lon);
        let (lat2, lon2) = webmercator_to_latlon(x, y);
        assert!((lat - lat2).abs() < 1e-9, "lat drift {lat} vs {lat2}");
        assert!((lon - lon2).abs() < 1e-9, "lon drift {lon} vs {lon2}");
    }

    #[test]
    fn haversine_locarno_to_lugano_is_plausible() {
        // Locarno (46.17, 8.80) → Lugano (46.00, 8.95) is ~22 km as the
        // crow flies. We don't need exactness, just order-of-magnitude right.
        let d = haversine_approx_m(46.17, 8.80, 46.00, 8.95);
        assert!(d > 18_000.0 && d < 28_000.0, "got {d} m");
    }
}

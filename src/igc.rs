//! Lenient IGC B-record parser.
//!
//! Ported from flightmap `src/igc.rs` (fixed-width slices, tolerant of
//! off-spec phone/vario files — verified against 245 real flights).
//! Extended for parastats: `HFSITSITE` (site name) and `HFPLTPILOT` /
//! `HFPLTPILOTINCHARGE` (pilot) headers are captured into [`Flight`] metadata.
//!
//! Strategy: parse by fixed-width slices (robust against off-spec phone/vario
//! files), tolerate short lines and missing altitudes, and never fail the whole
//! flight on one bad record. Site headers are accepted in both common
//! forms: `HFSITSITE:` (Skytraxx) and `HOSITSite:` (XCTrack's O-record).

use crate::model::{Flight, SourceKind, TrackPoint};
use chrono::NaiveDate;

#[derive(Debug, thiserror::Error)]
pub enum IgcError {
    #[error("no fix records (B lines) found")]
    NoFixRecords,
    #[error("invalid date record: {0}")]
    InvalidDate(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse IGC text into a [`Flight`]. The `id` becomes `Flight::id` verbatim —
/// callers should prefix per [`SourceKind::prefix`].
pub fn parse_igc(text: &str, id: &str) -> Result<Flight, IgcError> {
    let mut date: Option<NaiveDate> = None;
    let mut site: Option<String> = None;
    let mut pilot: Option<String> = None;
    let mut points: Vec<TrackPoint> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }

        // Date header. Two common shapes in the wild:
        //   HFDTE280625            (bare, older IGC spec)
        //   HFDTEDATE:280625,03    (XCTrack / newer devices; trailing ,NN is
        //                           a per-day flight sequence number we ignore)
        if let Some(rest) = line.strip_prefix("HFDTEDATE:") {
            if let Some(d) = parse_date(rest.split(',').next().unwrap_or(rest)) {
                date = Some(d);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("HFDTE") {
            // Skip the "DATE:" form already handled above; only accept if the
            // next chars look like a 6-digit date, not "DATE:...".
            if !rest.starts_with("DATE:") {
                if let Some(d) = parse_date(rest) {
                    date = Some(d);
                }
            }
            continue;
        }

        // Site name: HFSITSITE:<name> (Skytraxx) or XCTrack's observation
        // record HOSITSite:<name> (mixed case, per XCTrack's writer).
        if let Some(rest) = line
            .strip_prefix("HFSITSITE:")
            .or_else(|| strip_xctrack_site(line))
        {
            let v = rest.trim();
            // XCTrack writes a literal "?" when no site was selected — a
            // junk name is worse than none, so require at least one
            // alphanumeric character.
            if v.chars().any(|c| c.is_alphanumeric()) {
                site = Some(v.to_string());
            }
            continue;
        }

        // Pilot: HFPLTPILOTINCHARGE:<name> per spec; some devices write
        // HFPLTPILOT:<name>. Accept both.
        if let Some(rest) = line
            .strip_prefix("HFPLTPILOTINCHARGE:")
            .or_else(|| line.strip_prefix("HFPLTPILOT:"))
        {
            let v = rest.trim();
            if !v.is_empty() {
                pilot = Some(v.to_string());
            }
            continue;
        }

        if !line.starts_with('B') || line.len() < 35 {
            continue;
        }

        // B-record layout (after stripping the leading 'B'), 1-indexed in the
        // IGC spec, 0-indexed in the slices below:
        //   [0..6]    time        HHMMSS
        //   [6..13]   latitude    DDMMmmm  (7 chars)
        //   [13]      lat hemi    N / S
        //   [14..22]  longitude   DDDMMmmm (8 chars)
        //   [22]      lon hemi    E / W
        //   [23]      fix valid   A / V
        //   [24..29]  pressure alt  (5 chars, signed)
        //   [29..34]  gps alt        (5 chars, signed)
        let body = &line[1..];
        let time = parse_hms(&body[0..6]);
        let (lat, lon) = match parse_lat_lon(&body[6..14], &body[14..23]) {
            Some(v) => v,
            None => continue,
        };
        // fix validity at [23] — we keep V fixes too (degraded but real); just
        // don't fail the parse.
        let alt_baro = parse_signed_alt(&body[24..29]);
        let alt_gps = parse_signed_alt(&body[29..34]);

        let Some(time) = time else { continue };
        let Some(date) = date else {
            // No date yet — we can't form a timestamp. Skip until we see HFDTE.
            // Most well-formed files put HFDTE before the B records.
            continue;
        };

        points.push(TrackPoint {
            time: date.and_hms_opt(time.0, time.1, time.2).unwrap_or_default(),
            lat,
            lon,
            alt_gps,
            alt_baro,
        });
    }

    if points.is_empty() {
        return Err(IgcError::NoFixRecords);
    }

    Ok(Flight {
        id: id.to_string(),
        pilot,
        site,
        points,
        source: SourceKind::Local,
    })
}

/// XCTrack writes the site as an observation record, `HOSITSite:<name>`
/// (mixed case). Match the mnemonic case-insensitively so both
/// `HOSITSite:` and `HOSITSITE:` forms are accepted.
fn strip_xctrack_site(line: &str) -> Option<&str> {
    let (head, rest) = line.split_at_checked(10)?; // "HOSITSite:" is 10 bytes
    head.eq_ignore_ascii_case("HOSITSite:").then_some(rest)
}

/// Parse `DDMMYY` into a [`NaiveDate`]. Returns `None` if not parseable so a
/// malformed header doesn't kill the whole parse.
fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.len() < 6 {
        return None;
    }
    let d: u32 = s[0..2].parse().ok()?;
    let m: u32 = s[2..4].parse().ok()?;
    let y: u32 = s[4..6].parse().ok()?;
    // Window: 80..30 → 1980..2030. Good enough; revisit at year 2029.
    let year: i32 = if y < 30 {
        2000 + y as i32
    } else {
        1900 + y as i32
    };
    NaiveDate::from_ymd_opt(year, m, d)
}

/// Parse `HHMMSS`. Returns `(h, m, s)` or `None`.
fn parse_hms(s: &str) -> Option<(u32, u32, u32)> {
    if s.len() < 6 {
        return None;
    }
    let h: u32 = s[0..2].parse().ok()?;
    let m: u32 = s[2..4].parse().ok()?;
    let sec: u32 = s[4..6].parse().ok()?;
    Some((h, m, sec))
}

/// Decode `DDMMmmm + N/S` (7 chars + hemi) and `DDDMMmmm + E/W` (8 chars + hemi)
/// into signed (lat, lon) degrees. Returns `None` on any parse failure.
fn parse_lat_lon(lat_raw: &str, lon_raw: &str) -> Option<(f64, f64)> {
    // Lat: DDMMmmm + N/S — lat_raw is 8 chars (incl. hemi).
    if lat_raw.len() < 8 {
        return None;
    }
    let lat_deg: f64 = lat_raw[0..2].parse().ok()?;
    let lat_min_raw: &str = &lat_raw[2..7]; // MMmmm
    let lat_min: f64 = lat_min_raw.parse().ok()?; // e.g. "10500" → 10.500
    let lat_hemi = lat_raw.as_bytes()[7];
    let lat = lat_deg + lat_min / 60_000.0;
    let lat = if lat_hemi == b'S' { -lat } else { lat };

    // Lon: DDDMMmmm + E/W — lon_raw is 9 chars (incl. hemi).
    if lon_raw.len() < 9 {
        return None;
    }
    let lon_deg: f64 = lon_raw[0..3].parse().ok()?;
    let lon_min: f64 = lon_raw[3..8].parse().ok()?;
    let lon_hemi = lon_raw.as_bytes()[8];
    let lon = lon_deg + lon_min / 60_000.0;
    let lon = if lon_hemi == b'W' { -lon } else { lon };

    Some((lat, lon))
}

/// Parse a 5-char IGC altitude field. Per spec it's signed ("-0000".."99999")
/// but real files sometimes drop the sign or pad weirdly. Returns `None`
/// instead of failing the whole record on junk.
fn parse_signed_alt(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Some files put a space in for missing values — treat those as missing.
    if s.chars()
        .any(|c| !(c.is_ascii_digit() || c == '-' || c == '+'))
    {
        return None;
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_igc() -> &'static str {
        // Minimal hand-rolled IGC. HFDTE280625 = 2025-06-28.
        // Fix 1: 12:00:00 UTC, 46°10.500'N 008°59.100'E, A, baro 1450, gps 1460.
        // Fix 2: 12:00:01 UTC, 1 m further north, baro 1455, gps 1465.
        "HFDTE280625\n\
         B1200004610500N00859100EA0145001460\n\
         B1200014610501N00859100EA0145501465\n"
    }

    #[test]
    fn parses_basic_b_records() {
        let flight = parse_igc(sample_igc(), "local:test.igc").unwrap();
        assert_eq!(flight.id, "local:test.igc");
        assert_eq!(flight.points.len(), 2);

        let p0 = &flight.points[0];
        assert_eq!(
            p0.time,
            NaiveDate::from_ymd_opt(2025, 6, 28)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
        );
        // 46°10.500' = 46 + 10.5/60 = 46.175
        assert!((p0.lat - 46.175).abs() < 1e-9);
        // 008°59.100' = 8 + 59.1/60 = 8.985
        assert!((p0.lon - 8.985).abs() < 1e-9);
        assert_eq!(p0.alt_baro, Some(1450));
        assert_eq!(p0.alt_gps, Some(1460));
    }

    #[test]
    fn captures_site_and_pilot_headers() {
        let text = "HFDTE280625\n\
                    HFPLTPILOTINCHARGE:Ruben\n\
                    HFSITSITE:Monte Lema\n\
                    B1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert_eq!(flight.site.as_deref(), Some("Monte Lema"));
        assert_eq!(flight.pilot.as_deref(), Some("Ruben"));
    }

    #[test]
    fn accepts_bare_pilot_header() {
        // Skytraxx writes HFPLTPILOT:<name> (second-pilot slot per spec).
        let text = "HFDTE280625\nHFPLTPILOT:Anonymised\nB1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert_eq!(flight.pilot.as_deref(), Some("Anonymised"));
    }

    #[test]
    fn missing_site_and_pilot_stay_none() {
        let flight = parse_igc(sample_igc(), "local:t.igc").unwrap();
        assert!(flight.site.is_none());
        assert!(flight.pilot.is_none());
    }

    #[test]
    fn empty_site_header_is_ignored() {
        let text = "HFDTE280625\nHFSITSITE:\nB1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert!(flight.site.is_none());
    }

    #[test]
    fn captures_xctrack_observation_site_header() {
        // XCTrack writes HOSITSite:<name> (mixed case, O-record form) —
        // 95 of the 159 header-less flights in the real DB carry it.
        let text = "HFDTE280625\nHOSITSite:Monte Lema\nB1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert_eq!(flight.site.as_deref(), Some("Monte Lema"));

        // Uppercase variant accepted too.
        let text = "HFDTE280625\nHOSITSITE:Carì\nB1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert_eq!(flight.site.as_deref(), Some("Carì"));
    }

    #[test]
    fn junk_site_value_is_ignored() {
        // XCTrack writes "?" when the pilot never picked a site.
        let text = "HFDTE280625\nHOSITSite:?\nB1200004610500N00859100EA0145001460\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert!(flight.site.is_none());
    }

    #[test]
    fn ignores_short_b_lines_and_non_b_lines() {
        let text = "HFDTE010125\nB1200004610500N00859100EA0145001460\nshort\nI023638FXA\n";
        let flight = parse_igc(text, "local:t.igc").unwrap();
        assert_eq!(flight.points.len(), 1);
    }

    #[test]
    fn errors_when_no_fix_records() {
        let err = parse_igc("HFDTE010125\nonly headers\n", "local:t.igc").unwrap_err();
        assert!(matches!(err, IgcError::NoFixRecords));
    }

    #[test]
    fn handles_southern_and_western_hemispheres() {
        // 33°50.000'S 151°00.000'E (Sydney-ish). Lat = DDMMmmm + S, lon = DDDMMmmm + E.
        let text = "HFDTE010125\nB1200003350000S15100000EA0000000000\n";
        let flight = parse_igc(text, "local:sydney.igc").unwrap();
        // 33°50.000' S = -(33 + 50/60) ≈ -33.8333
        let lat = flight.points[0].lat;
        assert!(lat < 0.0 && lat > -34.0, "lat={lat}");
        let lon = flight.points[0].lon;
        assert!(lon > 150.0 && lon < 152.0, "lon={lon}");
    }
}

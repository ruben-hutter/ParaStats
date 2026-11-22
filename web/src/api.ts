// Typed fetch helpers for the parastats JSON API.
//
// Timestamps arrive as naive ISO strings ("2025-07-20T12:04:01") meaning UTC;
// `utcToLocalTime`/`utcToLocalDate` in fmt.ts convert them for display.

export interface Flight {
  id: string;
  source: string;
  filename: string;
  /** Local (Europe/Zurich) calendar date of takeoff, "YYYY-MM-DD". */
  date_utc: string;
  takeoff_time_utc: string;
  landing_time_utc: string;
  duration_s: number;
  takeoff_lat: number | null;
  takeoff_lon: number | null;
  landing_lat: number | null;
  landing_lon: number | null;
  takeoff_alt_m: number | null;
  site: string | null;
  pilot: string | null;
  distance_track_m: number;
  max_alt_m: number;
  min_alt_m: number;
  alt_gain_m: number;
  max_climb_ms: number;
  max_sink_ms: number;
}

export interface Climb {
  flight_id: string;
  start_time_utc: string;
  end_time_utc: string;
  avg_climb_ms: number;
  peak_climb_ms: number;
  gain_m: number;
  centroid_lat: number;
  centroid_lon: number;
}

export interface StatsTotals {
  flights: number;
  duration_s: number;
  distance_m: number;
  max_alt_m: number;
  alt_gain_m: number;
  max_climb_ms: number;
}

export interface YearStat {
  year: number;
  flights: number;
  duration_s: number;
  distance_m: number;
  max_alt_m: number;
  alt_gain_m: number;
}

export interface StatsResponse {
  totals: StatsTotals;
  by_year: YearStat[];
}

/** One full/simplified track point: [unix_ts, lat, lon, alt_m]. */
export type TrackRow = [number, number, number, number];

export interface SkywayProperties {
  id: string;
  date: string;
  start: string;
  points: number;
}

export interface ThermalCellProperties {
  count: number;
  avg_climb_ms: number;
  peak_climb_ms: number;
  total_gain_m: number;
}

async function getJson<T>(url: string): Promise<T> {
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${url}: HTTP ${r.status}`);
  return r.json() as Promise<T>;
}

export const api = {
  flights: (year?: number) =>
    getJson<{ flights: Flight[] }>(`/api/flights${year ? `?year=${year}` : ""}`).then(
      (r) => r.flights,
    ),

  flight: (id: string) => getJson<{ flight: Flight; climbs: Climb[] }>(`/api/flights/${id}`),

  track: (id: string) =>
    getJson<{ id: string; track: TrackRow[] }>(`/api/flights/${id}/track`).then(
      (r) => r.track,
    ),

  stats: () => getJson<StatsResponse>("/api/stats"),

  /** GeoJSON FeatureCollection of simplified tracks, vertices [lon, lat, alt]. */
  skyway: () =>
    getJson<GeoJSON.FeatureCollection<GeoJSON.LineString, SkywayProperties>>(
      "/api/layers/skyway",
    ),

  /** GeoJSON FeatureCollection of binned thermal cells. */
  thermals: () =>
    getJson<GeoJSON.FeatureCollection<GeoJSON.Point, ThermalCellProperties>>(
      "/api/layers/thermals",
    ),
};

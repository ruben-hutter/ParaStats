// Sunrise times (NOAA solar calculation, the classic ~30-line worksheet —
// same algorithm family SunCalc uses) for the kk7-style sunrise-relative
// time-of-day filter: morning = 0–7 h after sunrise, midday = 5–11 h,
// evening = 7 h+ (ranges deliberately overlap, as on thermal.kk7.ch).

/** Sunrise for a date/place as UTC hours [0,24), or null (polar day/night). */
export function sunriseUtcHours(dateIso: string, lat: number, lon: number): number | null {
  const date = new Date(`${dateIso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return null;
  const n =
    Math.floor(
      (date.getTime() - Date.UTC(date.getUTCFullYear(), 0, 0)) / 86_400_000,
    ) || 1;

  const lngHour = lon / 15;
  const t = n + (6 - lngHour) / 24; // rising

  const m = 0.9856 * t - 3.289;
  const mr = (m * Math.PI) / 180;

  let l = m + 1.916 * Math.sin(mr) + 0.02 * Math.sin(2 * mr) + 282.634;
  l = ((l % 360) + 360) % 360;

  let ra = (Math.atan(0.91764 * Math.tan((l * Math.PI) / 180)) * 180) / Math.PI;
  ra = ((ra % 360) + 360) % 360;
  // Put RA in the same quadrant as L.
  const lQuadrant = Math.floor(l / 90) * 90;
  const raQuadrant = Math.floor(ra / 90) * 90;
  ra = (ra + (lQuadrant - raQuadrant)) / 15;

  const sinDec = 0.39782 * Math.sin((l * Math.PI) / 180);
  const cosDec = Math.cos(Math.asin(sinDec));

  const zenith = 90.833; // official sunrise
  const latR = (lat * Math.PI) / 180;
  const cosH =
    (Math.cos((zenith * Math.PI) / 180) - sinDec * Math.sin(latR)) /
    (cosDec * Math.cos(latR));
  if (cosH > 1 || cosH < -1) return null; // sun never rises/sets that day

  const h = (360 - (Math.acos(cosH) * 180) / Math.PI) / 15; // rising

  const tLocal = h + ra - 0.06571 * t - 6.622;
  let ut = tLocal - lngHour;
  ut = ((ut % 24) + 24) % 24;
  return ut;
}

/** Hours between sunrise and a naive-ISO UTC timestamp; null if unknown. */
export function hoursAfterSunrise(
  takeoffIsoUtc: string,
  dateIso: string,
  lat: number | null,
  lon: number | null,
): number | null {
  if (lat == null || lon == null) return null;
  const sunrise = sunriseUtcHours(dateIso, lat, lon);
  if (sunrise == null) return null;
  const t = Date.parse(`${takeoffIsoUtc.includes("T") ? takeoffIsoUtc : takeoffIsoUtc.replace(" ", "T")}Z`);
  if (Number.isNaN(t)) return null;
  const hours = (t / 3_600_000) % 24 - sunrise;
  // Takeoff slightly before sunrise (dawn sledder) still counts as morning.
  return hours < -1 ? hours + 24 : Math.max(0, hours);
}

export type DayBucket = "morning" | "midday" | "evening";

/** kk7's overlapping sunrise-relative buckets. */
export function dayBuckets(hAfterSunrise: number): DayBucket[] {
  const out: DayBucket[] = [];
  if (hAfterSunrise >= 0 && hAfterSunrise < 7) out.push("morning");
  if (hAfterSunrise >= 5 && hAfterSunrise < 11) out.push("midday");
  if (hAfterSunrise >= 7) out.push("evening");
  return out;
}

/** Circular month distance |a - b| on a 12-month ring. */
export function monthDistance(a: number, b: number): number {
  const d = Math.abs(a - b);
  return Math.min(d, 12 - d);
}

/** kk7 time-of-year window: mid-month ±2 months (seasons overlap). */
export function inSeasonWindow(month: number, anchor: number): boolean {
  return monthDistance(month, anchor) <= 2;
}

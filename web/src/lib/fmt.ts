// Formatting + pilot-timezone helpers.
//
// All backend timestamps are naive-ISO UTC ("2025-07-20T12:04:01"). The
// pilot flies in Europe/Zurich; display conversion goes through `Intl` so
// DST is handled correctly (flightmap pattern, kept).

export const PILOT_TZ = "Europe/Zurich";

export function utcDate(isoUtc: string): Date {
  // Backend emits naive ISO meaning UTC — append Z to pin it.
  return new Date(isoUtc.includes("T") ? `${isoUtc}Z` : `${isoUtc.replace(" ", "T")}Z`);
}

const timeFmt = new Intl.DateTimeFormat("de-CH", {
  timeZone: PILOT_TZ,
  hour: "2-digit",
  minute: "2-digit",
});

const dateFmt = new Intl.DateTimeFormat("de-CH", {
  timeZone: PILOT_TZ,
  day: "2-digit",
  month: "short",
  year: "numeric",
});

const dateLongFmt = new Intl.DateTimeFormat("de-CH", {
  timeZone: PILOT_TZ,
  weekday: "short",
  day: "2-digit",
  month: "short",
  year: "numeric",
});

const monthHourFmt = new Intl.DateTimeFormat("en-US", {
  timeZone: PILOT_TZ,
  month: "numeric",
  hour: "numeric",
  hour12: false,
});

export function localTime(isoUtc: string): string {
  return timeFmt.format(utcDate(isoUtc));
}

export function localDate(isoUtc: string): string {
  return dateFmt.format(utcDate(isoUtc));
}

export function localDateLong(isoUtc: string): string {
  return dateLongFmt.format(utcDate(isoUtc));
}

export function formatDuration(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.round((s % 3600) / 60);
  return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
}

export function formatDurationShort(s: number): string {
  const h = s / 3600;
  if (h >= 100) return `${Math.round(h)}h`;
  if (h >= 10) return `${h.toFixed(0)}h`;
  return formatDuration(s);
}

export function formatDistance(m: number): string {
  return m >= 1000 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`;
}

/** Whole kilometres with de-CH grouping — for prose/headlines: 17'318 km. */
export function formatDistanceGrouped(m: number): string {
  return `${Math.round(m / 1000).toLocaleString("de-CH")} km`;
}

export function formatAltitude(m: number): string {
  return `${Math.round(m).toLocaleString("de-CH")} m`;
}

export function formatRate(ms: number): string {
  return `${ms > 0 ? "+" : ""}${ms.toFixed(1)} m/s`;
}

/** Local (pilot-tz) month 1-12 and hour 0-23 for a UTC timestamp. */
export function localMonthHour(isoUtc: string): { month: number; hour: number } | null {
  const d = utcDate(isoUtc);
  if (Number.isNaN(d.getTime())) return null;
  const parts = monthHourFmt.formatToParts(d);
  const month = Number(parts.find((p) => p.type === "month")?.value ?? 0);
  const hour = Number(parts.find((p) => p.type === "hour")?.value ?? 0);
  return { month, hour: hour === 24 ? 0 : hour };
}

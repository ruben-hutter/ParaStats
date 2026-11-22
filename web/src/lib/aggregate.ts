// Pure client-side aggregations for the Overview page — everything the
// paraglidingstats-style landing page shows (career summary, headline
// facts, per-year/month charts, site leaderboard) is derived from the
// plain `/api/flights` list; no extra endpoints needed.
//
// `date_utc` is the pilot-tz calendar date of takeoff ("YYYY-MM-DD"), so
// year/month grouping is a simple string slice — no Intl round-trips.

import type { Flight } from "../api";
import {
  formatDuration,
  formatDistance,
  formatDistanceGrouped,
  formatAltitude,
} from "./fmt";

export interface SiteStat {
  site: string;
  flights: number;
  duration_s: number;
  distance_m: number;
  max_alt_m: number;
  best_climb_ms: number;
  last_date: string;
}

export interface Pause {
  days: number;
  from: string; // ISO date of last flight before the pause
  to: string; // ISO date of first flight after it
}

export interface YearBar {
  year: string;
  flights: number;
  duration_s: number;
  distance_m: number;
}

export interface Aggregates {
  count: number;
  pilotName: string | null;
  first: Flight | null;
  last: Flight | null;
  activeYears: string[];
  duration_s: number;
  distance_m: number;
  alt_gain_m: number;
  longest: Flight | null;
  farthest: Flight | null;
  highest: Flight | null;
  bestClimb: Flight | null;
  sites: SiteStat[];
  busiest: SiteStat | null;
  perYear: YearBar[];
  perMonth: number[];
  longestPause: Pause | null;
}

/** Distinct pilot-tz years present in the list, newest first. */
export function yearsOf(list: Array<{ date_utc: string }>): string[] {
  return [...new Set(list.map((f) => f.date_utc.slice(0, 4)))].sort().reverse();
}

const MONTHS_SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function dateKey(f: Flight): string {
  return f.date_utc; // pilot-tz calendar date "YYYY-MM-DD"
}

function siteName(f: Flight): string {
  // Site strings look like "Monte Tamaro_Alpe Fop" (takeoff_landing) —
  // keep as-is; that's the vocabulary the log uses.
  return (f.site ?? "").trim();
}

export function aggregate(flights: Flight[]): Aggregates {
  const sorted = [...flights].sort((a, b) => dateKey(a).localeCompare(dateKey(b)));
  const empty: Aggregates = {
    count: 0,
    pilotName: null,
    first: null,
    last: null,
    activeYears: [],
    duration_s: 0,
    distance_m: 0,
    alt_gain_m: 0,
    longest: null,
    farthest: null,
    highest: null,
    bestClimb: null,
    sites: [],
    busiest: null,
    perYear: [],
    perMonth: Array(12).fill(0),
    longestPause: null,
  };
  if (sorted.length === 0) return empty;

  const byPilot = new Map<string, number>();
  for (const f of sorted) {
    if (f.pilot) byPilot.set(f.pilot, (byPilot.get(f.pilot) ?? 0) + 1);
  }
  let pilotName: string | null = null;
  let best = 0;
  for (const [name, n] of byPilot) {
    if (n > best) {
      best = n;
      pilotName = name;
    }
  }
  const agg: Aggregates = {
    ...empty,
    count: sorted.length,
    pilotName,
    first: sorted[0],
    last: sorted[sorted.length - 1],
  };

  // Per-year accumulation (ordered by year) + totals + extremes.
  const perYear = new Map<string, YearBar>();
  const perSite = new Map<string, SiteStat>();
  const years = new Set<string>();

  for (const f of sorted) {
    years.add(dateKey(f).slice(0, 4));
    agg.duration_s += f.duration_s;
    agg.distance_m += f.distance_track_m;
    agg.alt_gain_m += f.alt_gain_m;

    const year = dateKey(f).slice(0, 4);
    const yb = perYear.get(year) ?? { year, flights: 0, duration_s: 0, distance_m: 0 };
    yb.flights += 1;
    yb.duration_s += f.duration_s;
    yb.distance_m += f.distance_track_m;
    perYear.set(year, yb);

    const month = Number(dateKey(f).slice(5, 7)) - 1;
    if (month >= 0 && month < 12) agg.perMonth[month] += 1;

    const site = siteName(f);
    if (site) {
      const ss =
        perSite.get(site) ??
        {
          site,
          flights: 0,
          duration_s: 0,
          distance_m: 0,
          max_alt_m: 0,
          best_climb_ms: 0,
          last_date: dateKey(f),
        };
      ss.flights += 1;
      ss.duration_s += f.duration_s;
      ss.distance_m += f.distance_track_m;
      ss.max_alt_m = Math.max(ss.max_alt_m, f.max_alt_m);
      ss.best_climb_ms = Math.max(ss.best_climb_ms, f.max_climb_ms);
      if (dateKey(f) > ss.last_date) ss.last_date = dateKey(f);
      perSite.set(site, ss);
    }

    if (!agg.longest || f.duration_s > agg.longest.duration_s) agg.longest = f;
    if (!agg.farthest || f.distance_track_m > agg.farthest.distance_track_m) agg.farthest = f;
    if (!agg.highest || f.max_alt_m > agg.highest.max_alt_m) agg.highest = f;
    if (!agg.bestClimb || f.max_climb_ms > agg.bestClimb.max_climb_ms) agg.bestClimb = f;
  }

  agg.activeYears = [...years].sort();
  agg.perYear = [...perYear.values()].sort((a, b) => a.year.localeCompare(b.year));
  agg.sites = [...perSite.values()].sort(
    (a, b) => b.flights - a.flights || b.duration_s - a.duration_s,
  );
  agg.busiest = agg.sites[0] ?? null;

  // Longest pause between consecutive flights (calendar days).
  for (let i = 1; i < sorted.length; i++) {
    const prev = sorted[i - 1];
    const cur = sorted[i];
    const days = daysBetween(dateKey(prev), dateKey(cur));
    if (!agg.longestPause || days > agg.longestPause.days) {
      agg.longestPause = { days, from: dateKey(prev), to: dateKey(cur) };
    }
  }

  return agg;
}

function daysBetween(aIso: string, bIso: string): number {
  const a = Date.parse(`${aIso}T00:00:00Z`);
  const b = Date.parse(`${bIso}T00:00:00Z`);
  if (Number.isNaN(a) || Number.isNaN(b)) return 0;
  return Math.round((b - a) / 86_400_000);
}

/** "8 months" / "1 year 3 months" / "6 weeks" — pgstats-style pause text. */
export function humanPause(days: number): string {
  if (days < 60) return `${Math.max(1, Math.round(days / 7))} weeks`;
  const months = Math.round(days / 30.44);
  if (months < 12) return `${months} month${months === 1 ? "" : "s"}`;
  const years = Math.floor(months / 12);
  const rest = months % 12;
  const ys = `${years} year${years === 1 ? "" : "s"}`;
  return rest > 0 ? `${ys} ${rest} month${rest === 1 ? "" : "s"}` : ys;
}

/** "Mar 2022" from a "YYYY-MM-DD" string. */
export function monthYear(isoDate: string): string {
  const m = Number(isoDate.slice(5, 7));
  return `${MONTHS_SHORT[m - 1] ?? "?"} ${isoDate.slice(0, 4)}`;
}

/** Narrative career-summary sentence, numbers woven in prose. */
export function careerSummary(a: Aggregates): string {
  const first = a.first;
  const last = a.last;
  if (!first || !last) return "";
  const parts: string[] = [];

  const who = a.pilotName ? `${a.pilotName} has` : "This log has";
  const yearsText =
    a.activeYears.length > 1
      ? `between ${monthYear(first.date_utc)} and ${monthYear(last.date_utc)}`
      : `in ${first.date_utc.slice(0, 4)}`;

  parts.push(
    `${who} ${a.count} logged flights, ${yearsText}, totalling ${formatDuration(a.duration_s)} of airtime and ${formatDistanceGrouped(a.distance_m)} of track distance.`,
  );

  if (a.longest) {
    const site = siteName(a.longest);
    parts.push(
      `The longest flight lasted ${formatDuration(a.longest.duration_s)}${site ? ` from ${site}` : ""}${a.farthest && a.farthest.id !== a.longest.id ? `, the farthest covered ${formatDistance(a.farthest.distance_track_m)}` : ""}.`,
    );
  }
  if (a.busiest) {
    parts.push(
      `The favourite site is ${a.busiest.site} with ${a.busiest.flights} flights, and ${(a.alt_gain_m / 1000).toLocaleString("de-CH", { maximumFractionDigits: 0 })} km of total climbing along the way.`,
    );
  }
  if (a.highest && a.bestClimb) {
    parts.push(
      `Highest altitude: ${formatAltitude(a.highest.max_alt_m)}; strongest climb: ${a.bestClimb.max_climb_ms.toFixed(1)} m/s.`,
    );
  }
  return parts.join(" ");
}

export { siteName };

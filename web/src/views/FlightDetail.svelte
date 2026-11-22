<script lang="ts">
  // Flight detail (#/flight/:id) — data-dense page: stat strip, scrubable
  // altitude profile, embedded single-track map (map as a component, not a
  // full-screen shell), dense climbs table. The progress slider drives a
  // marker + flown segment on the map, the profile marker, and clicking a
  // climb scrubs to its start.

  import { PathLayer, ScatterplotLayer } from "@deck.gl/layers";
  import type { Layer } from "@deck.gl/core";
  import { api, type Climb, type Flight, type TrackRow } from "../api";
  import MapView from "../lib/MapView.svelte";
  import AltitudeProfile from "../lib/AltitudeProfile.svelte";
  import { altitudeColor } from "../lib/colors";
  import {
    localDateLong,
    localTime,
    formatDuration,
    formatDistance,
    formatRate,
  } from "../lib/fmt";
  import { formatAltitude } from "../lib/fmt";
  import { navigate } from "../router";

  let { id }: { id: string } = $props();

  let flight = $state<Flight | null>(null);
  let climbs = $state<Climb[]>([]);
  let track = $state<TrackRow[]>([]);
  let error = $state("");
  let loading = $state(true);
  let progress = $state(0); // 0..1 along the track (by fix index); starts
  // at launch so the full altitude-coloured track is visible before replay.

  async function load() {
    loading = true;
    error = "";
    try {
      const detail = await api.flight(id);
      flight = detail.flight;
      climbs = detail.climbs;
      track = await api.track(id);
      progress = 0;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void id;
    void load();
  });

  // Interpolated position at the current progress.
  let pos = $derived.by(() => {
    if (track.length === 0) return null;
    const f = progress * (track.length - 1);
    const i0 = Math.floor(f);
    const i1 = Math.min(i0 + 1, track.length - 1);
    const t = f - i0;
    const a = track[i0];
    const b = track[i1];
    return {
      lon: a[2] + (b[2] - a[2]) * t,
      lat: a[1] + (b[1] - a[1]) * t,
      alt: a[3] + (b[3] - a[3]) * t,
      t: a[0] + (b[0] - a[0]) * t,
      idx: i0,
    };
  });

  let fitBounds = $derived.by(() => {
    if (track.length === 0) return null;
    let minLon = Infinity;
    let minLat = Infinity;
    let maxLon = -Infinity;
    let maxLat = -Infinity;
    for (const [, lat, lon] of track) {
      if (lon < minLon) minLon = lon;
      if (lat < minLat) minLat = lat;
      if (lon > maxLon) maxLon = lon;
      if (lat > maxLat) maxLat = lat;
    }
    // A little breathing room so the embedded card doesn't crop the track.
    const padLon = (maxLon - minLon) * 0.12 + 0.01;
    const padLat = (maxLat - minLat) * 0.12 + 0.01;
    return [
      [minLon - padLon, minLat - padLat],
      [maxLon + padLon, maxLat + padLat],
    ] as [[number, number], [number, number]];
  });

  let layers = $derived.by(() => {
    if (!flight || track.length < 2 || !pos) return [];
    const flownEnd = pos.idx + 2; // flown path includes the interpolated fix
    // Track legibility (skytrackpro recipe): ONE continuous polyline per
    // layer (per-vertex altitude colours), white casing underneath, thick
    // round line — plus launch/landing dots with white outlines.
    const flown = track.slice(0, flownEnd);
    const endpoints: Array<{ pos: [number, number]; kind: "launch" | "land" }> = [];
    if (flight.takeoff_lat != null && flight.takeoff_lon != null) {
      endpoints.push({ pos: [flight.takeoff_lon, flight.takeoff_lat], kind: "launch" });
    }
    if (flight.landing_lat != null && flight.landing_lon != null) {
      endpoints.push({ pos: [flight.landing_lon, flight.landing_lat], kind: "land" });
    }
    return [
      // White halo so the track pops off the basemap.
      new PathLayer<TrackRow[]>({
        id: "track-casing",
        data: [track],
        getPath: (t: TrackRow[]) => t.map((p) => [p[2], p[1]] as [number, number]),
        getColor: [255, 255, 255, 230],
        getWidth: 8,
        widthUnits: "pixels",
      }),
      // Full track, coloured by altitude (per-vertex).
      new PathLayer<TrackRow[]>({
        id: "track",
        data: [track],
        getPath: (t: TrackRow[]) => t.map((p) => [p[2], p[1]] as [number, number]),
        getColor: (t: TrackRow[]) =>
          t.map((p) => {
            const [r, g, b] = altitudeColor(p[3]);
            return [r, g, b, 255] as [number, number, number, number];
          }),
        getWidth: 4.5,
        widthUnits: "pixels",
        jointRounded: true,
        capRounded: true,
      }),
      // Trailing "flown" segment in white.
      new PathLayer<TrackRow[]>({
        id: "flown",
        data: [flown],
        getPath: (t: TrackRow[]) => t.map((p) => [p[2], p[1]] as [number, number]),
        getColor: [255, 255, 255, 240],
        getWidth: 5.5,
        widthUnits: "pixels",
        jointRounded: true,
      }),
      // Launch / landing dots (cool cyan = launch, hot orange = landing).
      ...(endpoints.length > 0
        ? [
            new ScatterplotLayer<(typeof endpoints)[number]>({
              id: "endpoints",
              data: endpoints,
              getPosition: (d) => d.pos,
              getFillColor: (d) => (d.kind === "launch" ? [0, 178, 255, 255] : [255, 104, 0, 255]),
              getLineColor: [255, 255, 255, 255],
              radiusUnits: "pixels",
              getRadius: 7,
              stroked: true,
              lineWidthUnits: "pixels",
              getLineWidth: 2.5,
            }),
          ]
        : []),
      // Current position marker.
      new ScatterplotLayer({
        id: "marker",
        data: [{ position: [pos.lon, pos.lat] as [number, number] }],
        getPosition: (d: { position: [number, number] }) => d.position,
        getFillColor: [255, 255, 255, 255],
        getLineColor: [7, 11, 36, 255],
        radiusUnits: "pixels",
        getRadius: 7,
        stroked: true,
        lineWidthUnits: "pixels",
        getLineWidth: 2,
      }),
    ] as Layer[];
  });

  let times = $derived(track.map((p) => p[0]));
  let alts = $derived(track.map((p) => p[3]));

  function scrubToClimb(c: Climb) {
    const t = Date.parse(`${c.start_time_utc}Z`) / 1000;
    const span = times[times.length - 1] - times[0];
    progress = span > 0 ? Math.min(1, Math.max(0, (t - times[0]) / span)) : 0;
  }

  function onClimbKeydown(e: KeyboardEvent, c: Climb) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      scrubToClimb(c);
    }
  }
</script>

<div class="detail">
  {#if loading}
    <p class="status">loading…</p>
  {:else if error}
    <p class="status error">
      {error}
      <button class="linkish" onclick={() => navigate("#/flights")}>← back to flights</button>
    </p>
  {:else if flight}
    <button class="linkish back" onclick={() => navigate("#/flights")}>← all flights</button>
    <header class="head">
      <h1>{flight.site || flight.filename}</h1>
      <p class="sub">
        {localDateLong(flight.takeoff_time_utc)}{#if flight.pilot} · {flight.pilot}{/if}
      </p>
    </header>

    <dl class="stat-strip">
      <div class="stat">
        <dt>Takeoff</dt>
        <dd>{localTime(flight.takeoff_time_utc)}</dd>
        <span class="stat-sub">
          {#if flight.takeoff_lat != null}{flight.takeoff_lat.toFixed(4)}, {flight.takeoff_lon?.toFixed(4)}{/if}
          {#if flight.takeoff_alt_m != null} · {formatAltitude(flight.takeoff_alt_m)}{/if}
        </span>
      </div>
      <div class="stat">
        <dt>Landing</dt>
        <dd>{localTime(flight.landing_time_utc)}</dd>
        <span class="stat-sub">
          {#if flight.landing_lat != null}{flight.landing_lat.toFixed(4)}, {flight.landing_lon?.toFixed(4)}{/if}
        </span>
      </div>
      <div class="stat">
        <dt>Duration</dt>
        <dd>{formatDuration(flight.duration_s)}</dd>
      </div>
      <div class="stat">
        <dt>Track distance</dt>
        <dd>{formatDistance(flight.distance_track_m)}</dd>
      </div>
      <div class="stat">
        <dt>Max altitude</dt>
        <dd>{formatAltitude(flight.max_alt_m)}</dd>
        <span class="stat-sub">min {formatAltitude(flight.min_alt_m)}</span>
      </div>
      <div class="stat">
        <dt>Alt gain</dt>
        <dd>{formatAltitude(flight.alt_gain_m)}</dd>
      </div>
      <div class="stat">
        <dt>Max climb</dt>
        <dd class="up">{formatRate(flight.max_climb_ms)}</dd>
      </div>
      <div class="stat">
        <dt>Max sink</dt>
        <dd class="down">{formatRate(flight.max_sink_ms)}</dd>
      </div>
    </dl>

    <h2 class="h-section">Altitude profile</h2>
    <section class="card profile-card">
      <div class="scrub">
        <span class="hint">{pos ? localTime(new Date(pos.t * 1000).toISOString().slice(0, 19)) : ""}</span>
        <input
          type="range"
          min="0"
          max="1000"
          aria-label="Scrub flight progress"
          value={Math.round(progress * 1000)}
          oninput={(e) => (progress = Number((e.target as HTMLInputElement).value) / 1000)}
        />
        <span class="hint">{pos ? formatAltitude(pos.alt) : ""}</span>
      </div>
      <AltitudeProfile {times} {alts} bind:progress />
    </section>

    <h2 class="h-section">Track</h2>
    <section class="card map-card">
      <MapView layers={layers} {fitBounds} />
    </section>

    <h2 class="h-section">Climbs <span class="dim">· {climbs.length}</span></h2>
    <section class="card table-card">
      <table class="dense">
        <thead>
          <tr>
            <th>Window</th>
            <th class="num">Avg climb</th>
            <th class="num">Peak climb</th>
            <th class="num">Gain</th>
          </tr>
        </thead>
        <tbody>
          {#each climbs as c (c.start_time_utc + c.end_time_utc)}
            <tr
              class="climb-row"
              role="button"
              tabindex="0"
              title="Scrub to this climb"
              onclick={() => scrubToClimb(c)}
              onkeydown={(e) => onClimbKeydown(e, c)}
            >
              <td class="dim">{localTime(c.start_time_utc)} – {localTime(c.end_time_utc)}</td>
              <td class="num up">{formatRate(c.avg_climb_ms)}</td>
              <td class="num up">{formatRate(c.peak_climb_ms)}</td>
              <td class="num">+{formatAltitude(c.gain_m)}</td>
            </tr>
          {:else}
            <tr><td colspan="4" class="dim no-climbs">no climbs detected</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .detail {
    max-width: 1040px;
    margin: 0 auto;
    padding: clamp(16px, 3vw, 32px) clamp(14px, 3vw, 28px) 64px;
  }

  .back {
    margin-bottom: 6px;
  }

  .head h1 {
    margin: 4px 0 2px;
    font-size: clamp(1.3rem, 4vmin, 1.7rem);
    font-weight: 700;
    line-height: 1.3;
  }

  .head .sub {
    margin: 0 0 16px;
    color: var(--text-dim);
    font-size: 0.85rem;
  }

  .profile-card {
    padding: 14px 16px 10px;
  }

  .scrub {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
  }

  .scrub input[type="range"] {
    flex: 1;
  }

  .hint {
    color: var(--text-dim);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    min-width: 52px;
  }

  .hint:last-child {
    text-align: right;
  }

  .map-card {
    position: relative;
    height: 440px;
    overflow: hidden;
  }

  .table-card {
    padding: 8px 12px;
  }

  .climb-row {
    cursor: pointer;
  }

  .no-climbs {
    text-align: center;
    padding: 18px 0 !important;
  }

  .status {
    color: var(--text-dim);
    text-align: center;
    padding: 48px 12px;
  }
  .status.error {
    color: var(--red);
  }
</style>

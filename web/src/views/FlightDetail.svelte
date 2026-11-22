<script lang="ts">
  // Flight detail: stat cards, full-resolution track coloured by altitude,
  // progress slider (marker + flown segment), synced altitude profile,
  // climbs list. Clicking a climb scrubs to its start.

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
  import { navigate } from "../router";

  let { id }: { id: string } = $props();

  let flight = $state<Flight | null>(null);
  let climbs = $state<Climb[]>([]);
  let track = $state<TrackRow[]>([]);
  let error = $state("");
  let loading = $state(true);
  let progress = $state(1); // 0..1 along the track (by fix index)

  async function load() {
    loading = true;
    error = "";
    try {
      const detail = await api.flight(id);
      flight = detail.flight;
      climbs = detail.climbs;
      track = await api.track(id);
      progress = 1;
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
    return [
      [minLon, minLat],
      [maxLon, maxLat],
    ] as [[number, number], [number, number]];
  });

  let layers = $derived.by(() => {
    if (!flight || track.length < 2 || !pos) return [];
    const flownEnd = pos.idx + 2; // flown path includes the interpolated fix
    return [
      // Full track, coloured by altitude.
      new PathLayer<TrackRow>({
        id: "track",
        data: track,
        getPath: (p: TrackRow) => [p[2], p[1]],
        getColor: (p: TrackRow) => {
          const [r, g, b] = altitudeColor(p[3]);
          return [r, g, b, 230];
        },
        getWidth: 2,
        widthUnits: "pixels",
      }),
      // Trailing "flown" segment in white.
      new PathLayer<TrackRow>({
        id: "flown",
        data: track.slice(0, flownEnd),
        getPath: (p: TrackRow) => [p[2], p[1]],
        getColor: [255, 255, 255, 140],
        getWidth: 2.5,
        widthUnits: "pixels",
      }),
      // Current position marker.
      new ScatterplotLayer({
        id: "marker",
        data: [{ position: [pos.lon, pos.lat] as [number, number] }],
        getPosition: (d: { position: [number, number] }) => d.position,
        getFillColor: [255, 255, 255, 255],
        getLineColor: [20, 20, 30],
        radiusUnits: "pixels",
        getRadius: 6,
        stroked: true,
        lineWidthUnits: "pixels",
        getLineWidth: 2,
      }),
    ] as Layer[];
  });

  let times = $derived(track.map((p) => p[0]));
  let alts = $derived(track.map((p) => p[3]));
</script>

<div class="view">
  {#if loading}
    <div class="panel status">loading…</div>
  {:else if error}
    <div class="panel status error">
      {error}
      <button class="linkish" onclick={() => navigate("#/")}>← back to map</button>
    </div>
  {:else if flight}
    <MapView {layers} {fitBounds} />

    <section class="panel detail-panel scrolly">
      <button class="linkish" onclick={() => navigate("#/")}>← all flights</button>
      <h1>{flight.site || flight.filename}</h1>
      <p class="sub">{localDateLong(flight.takeoff_time_utc)}{#if flight.pilot} · {flight.pilot}{/if}</p>

      <div class="card-grid">
        <div class="card">
          <div class="stat-label">Takeoff</div>
          <div class="stat-value">{localTime(flight.takeoff_time_utc)}</div>
          <div class="stat-sub">
            {flight.takeoff_lat?.toFixed(4)}, {flight.takeoff_lon?.toFixed(4)}
            {#if flight.takeoff_alt_m != null} · {flight.takeoff_alt_m} m{/if}
          </div>
        </div>
        <div class="card">
          <div class="stat-label">Landing</div>
          <div class="stat-value">{localTime(flight.landing_time_utc)}</div>
          <div class="stat-sub">
            {flight.landing_lat?.toFixed(4)}, {flight.landing_lon?.toFixed(4)}
          </div>
        </div>
        <div class="card">
          <div class="stat-label">Duration</div>
          <div class="stat-value">{formatDuration(flight.duration_s)}</div>
        </div>
        <div class="card">
          <div class="stat-label">Track distance</div>
          <div class="stat-value">{formatDistance(flight.distance_track_m)}</div>
        </div>
        <div class="card">
          <div class="stat-label">Max altitude</div>
          <div class="stat-value">{flight.max_alt_m} m</div>
          <div class="stat-sub">min {flight.min_alt_m} m</div>
        </div>
        <div class="card">
          <div class="stat-label">Alt gain</div>
          <div class="stat-value">{flight.alt_gain_m} m</div>
        </div>
        <div class="card">
          <div class="stat-label">Max climb</div>
          <div class="stat-value up">{formatRate(flight.max_climb_ms)}</div>
        </div>
        <div class="card">
          <div class="stat-label">Max sink</div>
          <div class="stat-value down">{formatRate(flight.max_sink_ms)}</div>
        </div>
      </div>

      <h2>Flight progress</h2>
      <div class="scrub">
        <span class="hint">{localTime(new Date(pos?.t ?? 0).toISOString().slice(0, 19))}</span>
        <input
          type="range"
          min="0"
          max="1000"
          value={Math.round(progress * 1000)}
          oninput={(e) => (progress = Number((e.target as HTMLInputElement).value) / 1000)}
        />
        <span class="hint">{pos ? `${Math.round(pos.alt)} m` : ""}</span>
      </div>
      <AltitudeProfile {times} {alts} bind:progress />

      <h2>Climbs ({climbs.length})</h2>
      {#each climbs as c, i (i)}
        <button
          class="climb-row"
          onclick={() => {
            const t = Date.parse(`${c.start_time_utc}Z`) / 1000;
            const span = times[times.length - 1] - times[0];
            progress = span > 0 ? Math.min(1, Math.max(0, (t - times[0]) / span)) : 0;
          }}
        >
          <span class="cl-time">{localTime(c.start_time_utc)}–{localTime(c.end_time_utc)}</span>
          <span class="up">{formatRate(c.avg_climb_ms)} avg</span>
          <span class="up">{formatRate(c.peak_climb_ms)} peak</span>
          <span>+{c.gain_m} m</span>
        </button>
      {:else}
        <p class="hint">no climbs detected</p>
      {/each}
    </section>
  {/if}
</div>

<style>
  .view {
    position: absolute;
    inset: 0;
  }

  .status {
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    padding: 6px 14px;
    color: var(--text-dim);
  }

  .status.error {
    color: #ff8f8f;
  }

  .detail-panel {
    top: 12px;
    right: 12px;
    bottom: 12px;
    width: 430px;
    padding: 14px 16px;
    overflow-y: auto;
  }

  h1 {
    font-size: 18px;
    margin: 8px 0 2px;
  }

  .sub {
    color: var(--text-dim);
    margin: 0 0 12px;
  }

  h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin: 16px 0 8px;
  }

  .linkish {
    background: none;
    border: none;
    color: var(--accent);
    font: inherit;
    font-size: 12px;
    padding: 2px 0;
    cursor: pointer;
  }

  .scrub {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }

  .scrub input[type="range"] {
    flex: 1;
  }

  .hint {
    color: var(--text-dim);
    font-size: 12px;
  }

  .up {
    color: #7dd97d;
  }

  .down {
    color: #ff9f7d;
  }

  .climb-row {
    display: flex;
    width: 100%;
    gap: 12px;
    align-items: baseline;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    padding: 7px 4px;
    cursor: pointer;
    text-align: left;
  }

  .climb-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .cl-time {
    color: var(--text-dim);
    min-width: 110px;
  }
</style>

<script lang="ts">
  // Map view (default): all flights via the skyway PathLayer, thermal density
  // BitmapLayer, kk7 comparison overlay, season/time-of-day/year filters,
  // searchable flight list, hover popup + click → detail.
  //
  // Layer patterns ported from flightmap web/src/main.ts (altitude-band
  // colouring, density canvas, kk7 tms overlay, Intl-based pilot-tz filters).

  import { PathLayer, BitmapLayer } from "@deck.gl/layers";
  import type { PickingInfo } from "@deck.gl/core";
  import type { Map as MlMap } from "maplibre-gl";
  import maplibregl from "maplibre-gl";
  import type { Layer } from "@deck.gl/core";
  import { api, type Flight, type SkywayProperties, type ThermalCellProperties } from "../api";
  import MapView from "../lib/MapView.svelte";
  import { DEFAULT_KNOBS, buildThermalTexture, type ThermalKnobs } from "../lib/thermal";
  import { altitudeColor, rgbCss } from "../lib/colors";
  import { localMonthHour, localTime, localDate, formatDuration, formatDistance } from "../lib/fmt";
  import { navigate } from "../router";

  type SkywayFeature = GeoJSON.Feature<GeoJSON.LineString, SkywayProperties>;
  type CellFeature = GeoJSON.Feature<GeoJSON.Point, ThermalCellProperties>;

  let map: MlMap | null = null;
  let popup: maplibregl.Popup | null = null;

  let flights = $state<Flight[]>([]);
  let skyway = $state<SkywayFeature[]>([]);
  let thermals = $state<CellFeature[]>([]);
  let error = $state("");
  let loading = $state(true);

  // Layer + filter state.
  let showSkyway = $state(true);
  let showThermal = $state(true);
  let kk7 = $state(false);
  let colorMode = $state<"uniform" | "altitude">("altitude");
  let season = $state("all");
  let tod = $state("all");
  let year = $state("all");
  let knobs = $state<ThermalKnobs>({ ...DEFAULT_KNOBS });
  let showKnobs = $state(false);

  // Flight list state.
  let search = $state("");
  let sortKey = $state<"date" | "duration" | "distance" | "alt">("date");

  type Season = "all" | "spring" | "summer" | "autumn" | "winter";
  type TimeOfDay = "all" | "morning" | "midday" | "afternoon";

  function classifySeason(month: number): Season {
    if (month === 12 || month <= 2) return "winter";
    if (month <= 5) return "spring";
    if (month <= 8) return "summer";
    return "autumn";
  }

  function classifyTod(hour: number): TimeOfDay {
    if (hour < 12) return "morning";
    if (hour < 15) return "midday";
    return "afternoon";
  }

  function passesTimeFilters(startIso: string): boolean {
    if (season === "all" && tod === "all") return true;
    const parts = localMonthHour(startIso);
    if (!parts) return true;
    if (season !== "all" && classifySeason(parts.month) !== season) return false;
    if (tod !== "all" && classifyTod(parts.hour) !== tod) return false;
    return true;
  }

  // Note: the thermal layer is deliberately NOT season/time filtered — the
  // backend bins climbs into cells (count/avg/peak per cell), which erases
  // per-climb timestamps. It's an all-time "where are my thermals" overview.
  // Season/tod/year filtering applies to the skyway layer.
  let filteredSkyway = $derived(
    skyway.filter((f) => {
      if (year !== "all" && !f.properties.date.startsWith(year)) return false;
      return passesTimeFilters(f.properties.start);
    }),
  );

  let years = $derived(
    [...new Set(flights.map((f) => f.date_utc.slice(0, 4)))].sort().reverse(),
  );

  let thermalTexture = $derived(
    showThermal && thermals.length > 0 ? buildThermalTexture(thermals, knobs) : null,
  );

  let flightById = $derived(new Map(flights.map((f) => [f.id, f])));

  // Deck layers. Rebuilt (new array identity) only when the inputs above
  // change — MapView forwards the array to overlay.setProps.
  let layers = $derived.by(() => {
    const out: Layer[] = [];
    if (thermalTexture) {
      out.push(
        new BitmapLayer({
          id: "thermal",
          image: thermalTexture.canvas,
          bounds: thermalTexture.bounds,
          pickable: false,
        }),
      );
    }
    if (showSkyway && filteredSkyway.length > 0) {
      const useAltitude = colorMode === "altitude";
      out.push(
        new PathLayer<SkywayFeature>({
          id: "skyway",
          data: filteredSkyway,
          getPath: (f: SkywayFeature) =>
            f.geometry.coordinates.map(
              (c) => [c[0], c[1]] as [number, number],
            ),
          getColor: (f: SkywayFeature) => {
            if (!useAltitude) return [80, 120, 200, 180];
            return f.geometry.coordinates.map(([, , alt]) => {
              const [r, g, b] = altitudeColor(alt);
              return [r, g, b, 200];
            });
          },
          getWidth: 1.2,
          widthMinPixels: 1,
          widthUnits: "pixels",
          pickable: true,
          onHover: (info: PickingInfo) => onTrackHover(info),
          onClick: (info: PickingInfo) => {
            const id = (info.object as SkywayFeature | null)?.properties.id;
            if (id) navigate(`#/flight/${id}`);
            return true;
          },
          updateTriggers: { getColor: colorMode },
        }),
      );
    }
    return out;
  });

  // Fit camera to data once loaded.
  let fitBounds = $state<[[number, number], [number, number]] | null>(null);

  function onTrackHover(info: PickingInfo) {
    const f = info.object as SkywayFeature | null | undefined;
    if (!f || !info.coordinate || !map) {
      popup?.remove();
      popup = null;
      return;
    }
    const flight = flightById.get(f.properties.id);
    const title = flight?.site || f.properties.id;
    const html = `<strong>${escapeHtml(title)}</strong><br/>${localDate(f.properties.start)} · ${localTime(f.properties.start)}–${localTime(
      flights.find((x) => x.id === f.properties.id)?.landing_time_utc ?? "",
    )}`;
    const lngLat = [info.coordinate[0], info.coordinate[1]] as [number, number];
    popup ??= new maplibregl.Popup({
      closeButton: false,
      closeOnClick: false,
      offset: 8,
    })
      .setLngLat(lngLat)
      .setHTML(html)
      .addTo(map);
    popup.setLngLat(lngLat).setHTML(html);
  }

  function escapeHtml(s: string): string {
    return s.replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
    );
  }

  async function load() {
    try {
      const [fl, sky, therm] = await Promise.all([api.flights(), api.skyway(), api.thermals()]);
      flights = fl;
      skyway = sky.features;
      thermals = therm.features;

      if (skyway.length > 0) {
        let minLon = Infinity;
        let minLat = Infinity;
        let maxLon = -Infinity;
        let maxLat = -Infinity;
        for (const f of skyway) {
          for (const [lon, lat] of f.geometry.coordinates) {
            if (lon < minLon) minLon = lon;
            if (lat < minLat) minLat = lat;
            if (lon > maxLon) maxLon = lon;
            if (lat > maxLat) maxLat = lat;
          }
        }
        fitBounds = [
          [minLon, minLat],
          [maxLon, maxLat],
        ];
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  void load();

  // ---- flight list ----

  let listFlights = $derived(
    flights
      .filter((f) => {
        if (year !== "all" && !f.date_utc.startsWith(year)) return false;
        if (season !== "all" || tod !== "all") {
          if (!passesTimeFilters(f.takeoff_time_utc)) return false;
        }
        const q = search.trim().toLowerCase();
        if (!q) return true;
        return (
          (f.site ?? "").toLowerCase().includes(q) ||
          f.filename.toLowerCase().includes(q) ||
          f.date_utc.includes(q)
        );
      })
      .sort((a, b) => {
        switch (sortKey) {
          case "duration":
            return b.duration_s - a.duration_s;
          case "distance":
            return b.distance_track_m - a.distance_track_m;
          case "alt":
            return b.max_alt_m - a.max_alt_m;
          default:
            return b.takeoff_time_utc.localeCompare(a.takeoff_time_utc);
        }
      }),
  );
</script>

<div class="view">
  <MapView {layers} {fitBounds} bind:kk7 onReady={(m) => (map = m)} />

  {#if loading}
    <div class="panel status">loading…</div>
  {/if}
  {#if error}
    <div class="panel status error">{error}</div>
  {/if}

  <!-- layers + filters panel -->
  <section class="panel layers-panel">
    <h2>Layers</h2>
    <label class="check"
      ><input type="checkbox" bind:checked={showSkyway} />
      tracks
      {#if showSkyway}
        <small class="mode">
          <label class="check inline"
            ><input type="radio" name="colormode" value="uniform" bind:group={colorMode} />
            plain</label
          >
          <label class="check inline"
            ><input type="radio" name="colormode" value="altitude" bind:group={colorMode} />
            by altitude</label
          >
        </small>{/if}
    </label>
    <label class="check"><input type="checkbox" bind:checked={showThermal} /> thermals</label>
    {#if showThermal}
      <button class="linkish" onclick={() => (showKnobs = !showKnobs)}>
        {showKnobs ? "▾" : "▸"} thermal knobs
      </button>
      {#if showKnobs}
        <div class="knobs">
          <div class="field">
            blur σ: {knobs.sigma.toFixed(0)} px
            <input type="range" min="2" max="16" step="1" bind:value={knobs.sigma} />
          </div>
          <div class="field">
            red at: {knobs.maxRate.toFixed(1)} m/s
            <input type="range" min="3" max="10" step="0.5" bind:value={knobs.maxRate} />
          </div>
          <div class="field">
            opaque at: {knobs.fullOpacityRate.toFixed(1)} m/s
            <input
              type="range"
              min="1"
              max="5"
              step="0.5"
              bind:value={knobs.fullOpacityRate}
            />
          </div>
        </div>
      {/if}
    {/if}
    <label class="check"><input type="checkbox" bind:checked={kk7} /> kk7 comparison</label>

    <h2>Filters</h2>
    <div class="field">
      year
      <select bind:value={year}>
        <option value="all">all</option>
        {#each years as y (y)}
          <option value={y}>{y}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      season
      <select bind:value={season}>
        <option value="all">all</option>
        <option value="spring">spring</option>
        <option value="summer">summer</option>
        <option value="autumn">autumn</option>
        <option value="winter">winter</option>
      </select>
    </div>
    <div class="field">
      time of day
      <select bind:value={tod}>
        <option value="all">all</option>
        <option value="morning">morning (–12)</option>
        <option value="midday">midday (12–15)</option>
        <option value="afternoon">afternoon (15–)</option>
      </select>
    </div>
    <p class="hint">
      {filteredSkyway.length} flights shown · {thermals.length} thermal cells
    </p>
  </section>

  <!-- altitude legend -->
  {#if colorMode === "altitude" && showSkyway}
    <section class="panel legend">
      {#each [0, 800, 1600, 2400, 3200, 4000] as alt (alt)}
        <span><i style="background: {rgbCss(altitudeColor(alt))}"></i>{alt}m</span>
      {/each}
    </section>
  {/if}

  <!-- flight list -->
  <section class="panel list-panel scrolly">
    <div class="list-head">
      <input type="search" placeholder="search site, file, date…" bind:value={search} />
      <select bind:value={sortKey}>
        <option value="date">newest</option>
        <option value="duration">longest</option>
        <option value="distance">farthest</option>
        <option value="alt">highest</option>
      </select>
    </div>
    {#each listFlights as f (f.id)}
      <button class="flight-row" onclick={() => navigate(`#/flight/${f.id}`)}>
        <div class="fr-main">
          <span class="fr-site">{f.site || f.filename}</span>
          <span class="fr-date">{localDate(f.takeoff_time_utc)}</span>
        </div>
        <div class="fr-stats">
          <span>{formatDuration(f.duration_s)}</span>
          <span>{formatDistance(f.distance_track_m)}</span>
          <span>↑ {f.max_alt_m} m</span>
        </div>
      </button>
    {:else}
      <p class="hint">no flights match</p>
    {/each}
  </section>
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

  .layers-panel {
    top: 12px;
    left: 12px;
    width: 230px;
    max-height: calc(100% - 24px);
    overflow-y: auto;
    padding: 10px 14px;
  }

  .layers-panel h2 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin: 10px 0 4px;
  }
  .layers-panel h2:first-child {
    margin-top: 0;
  }

  .layers-panel select {
    width: 100%;
  }

  .mode {
    display: inline-flex;
    gap: 2px;
    margin-left: 4px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .check.inline {
    margin: 0;
    display: inline-flex;
    font-size: 12px;
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

  .knobs {
    padding: 4px 0 4px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .hint {
    color: var(--text-dim);
    font-size: 12px;
    margin: 8px 0 0;
  }

  .legend {
    bottom: 12px;
    left: 12px;
    padding: 8px 12px;
    display: flex;
    gap: 10px;
    font-size: 11px;
    color: var(--text-dim);
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .legend i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    display: inline-block;
  }

  .list-panel {
    top: 12px;
    right: 12px;
    bottom: 12px;
    width: 300px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .list-head {
    display: flex;
    gap: 6px;
  }

  .list-head input[type="search"] {
    flex: 1;
    min-width: 0;
  }

  .flight-row {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font: inherit;
    padding: 8px 4px;
    cursor: pointer;
  }

  .flight-row:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .fr-main {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .fr-site {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fr-date {
    color: var(--text-dim);
    font-size: 12px;
    white-space: nowrap;
  }

  .fr-stats {
    display: flex;
    gap: 12px;
    color: var(--text-dim);
    font-size: 12px;
    margin-top: 2px;
  }
</style>

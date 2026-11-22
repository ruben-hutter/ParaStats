<script lang="ts">
  // Thermals (#/thermals) — the map view, in kk7's visual language: glass
  // layer panel (right), Skyways / Thermals / Hotspots / kk7-raster layer
  // taxonomy, and kk7's time filter — a season ramp slider (mid-month ±2
  // months, overlapping) plus a sunrise-relative time-of-day ramp slider
  // (morning 0–7 h, midday 5–11 h, evening 7+ h after sunrise; NOAA sunrise).
  // Hover popup + click → flight detail kept from the old map view.
  //
  // Layer patterns ported from flightmap web/src/main.ts (altitude-band
  // colouring, density canvas, kk7 tms overlay).

  import { PathLayer, BitmapLayer, ScatterplotLayer } from "@deck.gl/layers";
  import type { PickingInfo } from "@deck.gl/core";
  import type { Map as MlMap } from "maplibre-gl";
  import maplibregl from "maplibre-gl";
  import type { Layer } from "@deck.gl/core";
  import { api, type Flight, type SkywayProperties, type ThermalCellProperties } from "../api";
  import MapView from "../lib/MapView.svelte";
  import { DEFAULT_KNOBS, buildThermalTexture, type ThermalKnobs } from "../lib/thermal";
  import { altitudeColor, rgbCss, densityColor, hotColor } from "../lib/colors";
  import { localTime, localDate, formatAltitude } from "../lib/fmt";
  import { hoursAfterSunrise, dayBuckets, inSeasonWindow, type DayBucket } from "../lib/sunrise";
  import { navigate } from "../router";

  type SkywayFeature = GeoJSON.Feature<GeoJSON.LineString, SkywayProperties>;
  type CellFeature = GeoJSON.Feature<GeoJSON.Point, ThermalCellProperties>;

  const HOTSPOT_COUNT = 12;

  let map: MlMap | null = null;
  let popup: maplibregl.Popup | null = null;

  let flights = $state<Flight[]>([]);
  let skyway = $state<SkywayFeature[]>([]);
  let thermals = $state<CellFeature[]>([]);
  let error = $state("");
  let loading = $state(true);

  // Layer state (kk7 taxonomy).
  let showSkyway = $state(true);
  let showThermal = $state(true);
  let showHotspots = $state(true);
  let kk7 = $state(false);
  let colorMode = $state<"uniform" | "altitude">("altitude");

  // Filter state.
  let year = $state("all");
  /** 0 = all year, 1–12 = anchor month (window = ±2 months, circular). */
  let seasonAnchor = $state(0);
  /** 0 = all day, 1 = morning, 2 = midday, 3 = evening (sunrise-relative). */
  let daySlot = $state(0);

  let knobs = $state<ThermalKnobs>({ ...DEFAULT_KNOBS });
  let showKnobs = $state(false);

  let flightById = $derived(new Map(flights.map((f) => [f.id, f])));

  let years = $derived(
    [...new Set(flights.map((f) => f.date_utc.slice(0, 4)))].sort().reverse(),
  );

  // kk7 ramp gradients: seasonal cycle for time-of-year, day cycle for
  // time-of-day. Inline on the slider (input.ramp --ramp).
  const MONTHS_SHORT = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const SEASON_RAMP =
    "linear-gradient(90deg, #4aa3ff 0%, #7ad0a8 17%, #ffc600 42%, #ff6800 58%, #f24a00 67%, #b06a1f 83%, #4aa3ff 100%)";
  const DAY_RAMP =
    "linear-gradient(90deg, #141c4a 0%, #31418f 20%, #ffc600 40%, #8ad4ff 60%, #ffc600 80%, #141c4a 100%)";

  let seasonLabel = $derived.by(() => {
    if (seasonAnchor === 0) return "all year";
    const m = seasonAnchor;
    const from = ((m - 3 + 12) % 12) + 1; // mid-(month-2)
    const to = ((m - 1 + 2) % 12) + 1; // mid-(month+2)
    return `${MONTHS_SHORT[m - 1]} · mid-${MONTHS_SHORT[from - 1]} – mid-${MONTHS_SHORT[to - 1]}`;
  });

  const DAY_SLOTS = ["all day", "morning", "midday", "evening"];
  let dayLabel = $derived(DAY_SLOTS[daySlot]);

  function passesTimeFilters(f: Flight | undefined, startIso: string, date: string): boolean {
    if (seasonAnchor === 0 && daySlot === 0 && year === "all") return true;
    if (year !== "all" && !date.startsWith(year)) return false;
    if (seasonAnchor !== 0 && !inSeasonWindow(Number(date.slice(5, 7)), seasonAnchor)) {
      return false;
    }
    if (daySlot !== 0) {
      const bucket = DAY_SLOTS[daySlot] as DayBucket;
      const flight = f ?? undefined;
      if (!flight || flight.takeoff_lat == null || flight.takeoff_lon == null) return true; // can't classify — keep
      const h = hoursAfterSunrise(startIso, date, flight.takeoff_lat, flight.takeoff_lon);
      if (h != null && !dayBuckets(h).includes(bucket)) return false;
    }
    return true;
  }

  // Note: the thermal density layer is deliberately NOT time filtered — the
  // backend bins climbs into cells (count/avg/peak per cell), which erases
  // per-climb timestamps. It's an all-time "where are my thermals" overview.
  // Time filtering applies to skyways (+ hotspots carry their own all-time
  // stats too).
  let filteredSkyway = $derived(
    skyway.filter((f) => {
      const flight = flightById.get(f.properties.id);
      return passesTimeFilters(flight, f.properties.start, f.properties.date);
    }),
  );

  let thermalTexture = $derived(
    showThermal && thermals.length > 0 ? buildThermalTexture(thermals, knobs) : null,
  );

  // Hotspots: strongest thermal cells — the "centers of thermal activity".
  // Top cells by climb count, greedily spaced ≥ ~2 km apart so clusters of
  // adjacent bins don't paint a bullseye on one mountain.
  let hotspots = $derived.by(() => {
    const km = (a: CellFeature, b: CellFeature) => {
      const dLat = (b.geometry.coordinates[1] - a.geometry.coordinates[1]) * 111;
      const dLon =
        (b.geometry.coordinates[0] - a.geometry.coordinates[0]) *
        111 * Math.cos(((a.geometry.coordinates[1] ?? 0) * Math.PI) / 180);
      return Math.hypot(dLat, dLon);
    };
    const picked: CellFeature[] = [];
    for (const f of [...thermals].sort((a, b) => b.properties.count - a.properties.count)) {
      if (picked.length >= HOTSPOT_COUNT) break;
      if (picked.every((p) => km(p, f) >= 2)) picked.push(f);
    }
    const maxCount = picked[0]?.properties.count ?? 1;
    return picked.map((f, rank) => ({
      f,
      rank,
      lon: f.geometry.coordinates[0] as number,
      lat: f.geometry.coordinates[1] as number,
      rel: f.properties.count / maxCount, // 1 = strongest
    }));
  });

  // Deck layers. Rebuilt (new array identity) only when the inputs above
  // change — MapView forwards the array to overlay.setProps.
  // Track legibility (skytrackpro recipe): thick rounded lines, strong
  // alpha, bright cool blues over the light topo basemap.
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
            if (!useAltitude) return [0, 90, 235, 255]; // cool blue #005aeb
            return f.geometry.coordinates.map(([, , alt]) => {
              const [r, g, b] = altitudeColor(alt);
              return [r, g, b, 245];
            });
          },
          getWidth: 2,
          widthMinPixels: 2,
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
    if (showHotspots && hotspots.length > 0) {
      out.push(
        new ScatterplotLayer<(typeof hotspots)[number]>({
          id: "hotspots",
          data: hotspots,
          getPosition: (d) => [d.lon, d.lat],
          getFillColor: (d) => {
            const [r, g, b] = hotColor(d.f.properties.avg_climb_ms / 5);
            return [r, g, b, 240];
          },
          getLineColor: [255, 255, 255, 255],
          getRadius: (d) => 6 + 12 * d.rel,
          radiusUnits: "pixels",
          stroked: true,
          lineWidthUnits: "pixels",
          getLineWidth: 2.5,
          pickable: true,
          onHover: (info: PickingInfo) => onHotspotHover(info),
        }),
      );
    }
    return out;
  });

  // Fit camera to data once loaded.
  let fitBounds = $state<[[number, number], [number, number]] | null>(null);

  function showPopup(lngLat: [number, number], html: string) {
    if (!map) return;
    popup ??= new maplibregl.Popup({
      closeButton: false,
      closeOnClick: false,
      offset: 10,
    })
      .setLngLat(lngLat)
      .setHTML(html)
      .addTo(map);
    popup.setLngLat(lngLat).setHTML(html);
  }

  function dismissPopup() {
    popup?.remove();
    popup = null;
  }

  function onTrackHover(info: PickingInfo) {
    const f = info.object as SkywayFeature | null | undefined;
    if (!f || !info.coordinate || !map) {
      dismissPopup();
      return;
    }
    const flight = flightById.get(f.properties.id);
    const title = flight?.site || f.properties.id;
    const html = `<strong>${escapeHtml(title)}</strong><br/>${localDate(f.properties.start)} · ${localTime(f.properties.start)}–${localTime(
      flight?.landing_time_utc ?? "",
    )}`;
    showPopup([info.coordinate[0], info.coordinate[1]], html);
  }

  function onHotspotHover(info: PickingInfo) {
    const d = info.object as (typeof hotspots)[number] | null | undefined;
    if (!d || !info.coordinate || !map) {
      dismissPopup();
      return;
    }
    const p = d.f.properties;
    const html = `<strong>Hotspot</strong><br/>${p.count} climbs · avg ${p.avg_climb_ms.toFixed(1)} m/s<br/>peak ${p.peak_climb_ms.toFixed(1)} m/s · ${formatAltitude(p.total_gain_m)} gained`;
    showPopup([info.coordinate[0], info.coordinate[1]], html);
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
</script>

<div class="view">
  <MapView layers={layers} {fitBounds} bind:kk7 onReady={(m) => (map = m)} />

  {#if loading}
    <div class="panel status">loading…</div>
  {/if}
  {#if error}
    <div class="panel status error">{error}</div>
  {/if}

  <!-- kk7-style glass layer panel, right side -->
  <section class="panel layers-panel scrolly">
    <h3 class="panel-title">Layers</h3>
    <label class="check"
      ><input type="checkbox" bind:checked={showSkyway} />
      skyways
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
    <label class="check"
      ><input type="checkbox" bind:checked={showHotspots} /> hotspots
      <small class="mode-inline">top {HOTSPOT_COUNT}</small></label
    >
    <label class="check"><input type="checkbox" bind:checked={kk7} /> kk7 comparison</label>

    <h3 class="panel-title">Time filter</h3>
    <div class="field">
      <select bind:value={year} aria-label="Filter by year">
        <option value="all">all years</option>
        {#each years as y (y)}
          <option value={y}>{y}</option>
        {/each}
      </select>
    </div>
    <label class="ramp-field">
      time of year <b>{seasonLabel}</b>
      <input
        class="ramp"
        type="range"
        min="0"
        max="12"
        step="1"
        bind:value={seasonAnchor}
        style="--ramp: {SEASON_RAMP}"
        aria-label="Time of year"
      />
    </label>
    <label class="ramp-field">
      time of day <b>{dayLabel}</b>
      <input
        class="ramp"
        type="range"
        min="0"
        max="3"
        step="1"
        bind:value={daySlot}
        style="--ramp: {DAY_RAMP}"
        aria-label="Time of day (sunrise-relative)"
      />
    </label>
    <p class="hint">
      {filteredSkyway.length} flights shown · {thermals.length} thermal cells
    </p>
  </section>

  <!-- legend: thermal climb ramp + altitude bands -->
  <section class="panel legend">
    <div class="legend-block">
      <span class="legend-title">climb</span>
      <span
        class="ramp-bar"
        style="background: linear-gradient(90deg, {[0, 0.25, 0.5, 0.75, 1]
          .map((t) => rgbCss(densityColor(t)))
          .join(', ')})"
      ></span>
      <span class="legend-range">0 – {knobs.maxRate.toFixed(1)} m/s</span>
    </div>
    {#if colorMode === "altitude" && showSkyway}
      <div class="legend-block">
        <span class="legend-title">alt</span>
        {#each [0, 1600, 3200, 4000] as alt (alt)}
          <span><i style="background: {rgbCss(altitudeColor(alt))}"></i>{alt}m</span>
        {/each}
      </div>
    {/if}
    {#if showHotspots}
      <div class="legend-block">
        <span class="legend-title">hot</span>
        <span
          class="ramp-bar hot"
          style="background: linear-gradient(90deg, {rgbCss(hotColor(0))}, {rgbCss(hotColor(0.5))}, {rgbCss(hotColor(1))})"
        ></span>
        <span class="legend-range">avg climb</span>
      </div>
    {/if}
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
    color: var(--red);
  }

  /* kk7 puts the layers panel on the right, over the map. */
  .layers-panel {
    top: 12px;
    right: 12px;
    width: 250px;
    max-height: calc(100% - 24px);
    overflow-y: auto;
    padding: 12px 16px;
    font-size: 14px;
    line-height: 1.4;
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

  .mode-inline {
    margin-left: auto;
    font-size: 11px;
    color: var(--text-faint);
  }

  .check.inline {
    margin: 0;
    display: inline-flex;
    font-size: 12px;
  }

  .knobs {
    padding: 4px 0 4px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--text-dim);
    margin: 6px 0;
  }

  .ramp-field {
    display: block;
    margin: 8px 0 10px;
    font-size: 12px;
    color: var(--text-dim);
    cursor: default;
  }
  .ramp-field b {
    float: right;
    margin-left: 8px;
    font-weight: 600;
    color: var(--text);
    font-size: 11.5px;
  }

  .hint {
    color: var(--text-dim);
    font-size: 12px;
    margin: 10px 0 0;
  }

  .legend {
    bottom: 12px;
    left: 12px;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 11px;
    color: var(--text-dim);
  }

  .legend-block {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .legend-block > span:not(.ramp-bar) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .legend-title {
    text-transform: uppercase;
    font-size: 9.5px;
    letter-spacing: 0.08em;
    color: var(--text-faint);
  }

  .legend .ramp-bar {
    width: 72px;
    height: 8px;
    border-radius: 4px;
    border: 1px solid var(--border);
    display: inline-block;
  }

  .legend-range {
    font-variant-numeric: tabular-nums;
  }

  .legend i {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    display: inline-block;
  }

  .scrolly {
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--border-strong) transparent;
  }
</style>

<script lang="ts">
  // Stats overview: totals cards + per-year bars, hand-rolled SVG.

  import { api, type StatsResponse } from "../api";
  import { formatDurationShort, formatDistance, formatAltitude } from "../lib/fmt";

  let data = $state<StatsResponse | null>(null);
  let error = $state("");
  let barMetric = $state<"flights" | "duration_s" | "distance_m">("flights");

  async function load() {
    try {
      data = await api.stats();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  void load();

  const METRICS = {
    flights: { label: "flights", value: (y: { flights: number }) => y.flights, fmt: (v: number) => String(v) },
    duration_s: {
      label: "hours",
      value: (y: { duration_s: number }) => y.duration_s,
      fmt: (v: number) => formatDurationShort(v),
    },
    distance_m: {
      label: "km",
      value: (y: { distance_m: number }) => y.distance_m,
      fmt: (v: number) => `${Math.round(v / 1000)} km`,
    },
  } as const;

  let years = $derived(
    data ? [...data.by_year].sort((a, b) => a.year - b.year) : [],
  );
  let maxV = $derived(Math.max(...years.map(METRICS[barMetric].value), 1));
</script>

<div class="view">
  {#if error}
    <div class="panel status error">{error}</div>
  {:else if data}
    <h1>All flights</h1>
    <div class="card-grid wide">
      <div class="card">
        <div class="stat-label">Flights</div>
        <div class="stat-value">{data.totals.flights}</div>
      </div>
      <div class="card">
        <div class="stat-label">Airtime</div>
        <div class="stat-value">{formatDurationShort(data.totals.duration_s)}</div>
      </div>
      <div class="card">
        <div class="stat-label">Track distance</div>
        <div class="stat-value">{formatDistance(data.totals.distance_m)}</div>
      </div>
      <div class="card">
        <div class="stat-label">Max altitude</div>
        <div class="stat-value">{formatAltitude(data.totals.max_alt_m)}</div>
      </div>
      <div class="card">
        <div class="stat-label">Total climb</div>
        <div class="stat-value">{formatAltitude(data.totals.alt_gain_m)}</div>
      </div>
      <div class="card">
        <div class="stat-label">Best climb</div>
        <div class="stat-value up">
          {data.totals.max_climb_ms > 0 ? `+${data.totals.max_climb_ms.toFixed(1)}` : "–"} m/s
        </div>
      </div>
    </div>

    <h1>Per year</h1>
    <div class="bars card">
      <div class="bar-head">
        {#each Object.entries(METRICS) as [key, m] (key)}
          <label class="check inline">
            <input type="radio" name="metric" value={key} bind:group={barMetric} />
            {m.label}
          </label>
        {/each}
      </div>
      {#if years.length > 0}
        <div class="chart">
          {#each years as y (y.year)}
            {@const v = METRICS[barMetric].value(y)}
            <div class="bar" style="height: {Math.max(2, (v / maxV) * 100)}%">
              <span class="bar-value">{METRICS[barMetric].fmt(v)}</span>
              <div class="bar-fill"></div>
            </div>
            <span class="bar-year">{y.year}</span>
          {/each}
        </div>
      {:else}
        <p class="hint">no flights yet — run <code>parastats ingest &lt;folder&gt;</code></p>
      {/if}
    </div>
  {:else}
    <div class="panel status">loading…</div>
  {/if}
</div>

<style>
  .view {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: 24px 28px;
    max-width: 900px;
  }

  .status {
    position: relative;
    top: 24px;
    left: 24px;
    width: fit-content;
    padding: 6px 14px;
    color: var(--text-dim);
  }

  .status.error {
    color: #ff8f8f;
  }

  h1 {
    font-size: 15px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-dim);
    margin: 20px 0 10px;
  }

  h1:first-child {
    margin-top: 0;
  }

  .wide {
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  }

  .up {
    color: #7dd97d;
  }

  .bars {
    padding-bottom: 8px;
  }

  .bar-head {
    display: flex;
    gap: 10px;
    margin-bottom: 10px;
  }

  .check.inline {
    font-size: 12px;
    color: var(--text-dim);
  }

  .chart {
    display: flex;
    align-items: flex-end;
    gap: 18px;
    height: 220px;
    padding: 0 6px;
  }

  .bar {
    position: relative;
    flex: 1;
    max-width: 90px;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
  }

  .bar-fill {
    width: 100%;
    background: linear-gradient(180deg, var(--accent) 0%, var(--accent-dim) 100%);
    border-radius: 4px 4px 0 0;
    min-height: 3px;
  }

  .bar-value {
    text-align: center;
    font-size: 12px;
    color: var(--text-dim);
    padding: 0 2px 3px;
  }

  .bar-year {
    flex: 1;
    max-width: 90px;
    text-align: center;
    font-size: 12px;
    color: var(--text-dim);
    margin-top: 4px;
  }

  .hint {
    color: var(--text-dim);
    font-size: 13px;
  }

  code {
    background: rgba(255, 255, 255, 0.07);
    padding: 1px 6px;
    border-radius: 4px;
  }
</style>

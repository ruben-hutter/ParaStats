<script lang="ts">
  // Overview (#/) — the paraglidingstats-style landing page: narrative
  // career summary in prose, headline facts, per-year bars (switchable
  // metric), monthly histogram, site leaderboard. Everything is computed
  // client-side from /api/flights.

  import { api, type Flight } from "../api";
  import { aggregate, careerSummary, humanPause, monthYear } from "../lib/aggregate";
  import {
    formatDuration,
    formatDurationShort,
    formatDistance,
    formatDistanceGrouped,
    formatAltitude,
    formatRate,
  } from "../lib/fmt";
  import BarChart from "../lib/BarChart.svelte";

  let flights = $state<Flight[]>([]);
  let error = $state("");
  let loading = $state(true);

  let metric = $state<"flights" | "duration_s" | "distance_m">("flights");

  void load();

  async function load() {
    try {
      flights = await api.flights();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  let agg = $derived(aggregate(flights));

  type SiteSort = "flights" | "duration_s" | "distance_m" | "max_alt_m";
  let siteSort = $state<SiteSort>("flights");
  let siteSortDir = $state<1 | -1>(-1);

  function setSiteSort(key: SiteSort) {
    if (siteSort === key) {
      siteSortDir = siteSortDir === 1 ? -1 : 1;
    } else {
      siteSort = key;
      siteSortDir = -1;
    }
  }

  let sortedSites = $derived(
    [...agg.sites].sort((a, b) => (a[siteSort] - b[siteSort]) * siteSortDir),
  );

  const METRICS = {
    flights: {
      label: "flights",
      value: (y: { flights: number }) => y.flights,
      display: (v: number) => String(v),
    },
    duration_s: {
      label: "hours",
      value: (y: { duration_s: number }) => y.duration_s,
      display: (v: number) => formatDurationShort(v),
    },
    distance_m: {
      label: "distance",
      value: (y: { distance_m: number }) => y.distance_m,
      display: (v: number) => `${Math.round(v / 1000)} km`,
    },
  } as const;

  let yearBars = $derived(
    agg.perYear.map((y) => ({
      label: y.year,
      value: METRICS[metric].value(y),
      display: METRICS[metric].display(METRICS[metric].value(y)),
    })),
  );

  const MONTHS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"];
  let monthBars = $derived(
    agg.perMonth.map((n, i) => ({ label: MONTHS[i], value: n, display: String(n) })),
  );

  let span = $derived(
    agg.activeYears.length > 1
      ? `${agg.activeYears[0]} – ${agg.activeYears[agg.activeYears.length - 1]}`
      : (agg.activeYears[0] ?? ""),
  );
</script>

<div class="overview">
  {#if loading}
    <p class="status">loading…</p>
  {:else if error}
    <p class="status error">{error}</p>
  {:else if agg.count === 0}
    <header class="page-head">
      <h1>ParaStats</h1>
      <p class="sub">paragliding flight log</p>
    </header>
    <p class="status">
      No flights yet — run <code>parastats ingest &lt;folder&gt;</code> to load your IGC files.
    </p>
  {:else}
    <header class="page-head">
      <h1>{agg.pilotName ?? "ParaStats"}</h1>
      <p class="sub">
        {agg.count} flights · paragliding flight log{#if span} · {span}{/if}
      </p>
    </header>

    <section class="card prose-card">
      <p class="career-summary">{careerSummary(agg)}</p>

      <ul class="facts">
        <li>
          Active years: <strong>{agg.activeYears.length}</strong>
          {#if span}({span}){/if}
        </li>
        {#if agg.longestPause}
          <li>
            Longest pause: <strong>{humanPause(agg.longestPause.days)}</strong>
            ({monthYear(agg.longestPause.from)} – {monthYear(agg.longestPause.to)})
          </li>
        {/if}
        {#if agg.longest}
          <li>
            Best flight:
            <strong>{formatDuration(agg.longest.duration_s)}</strong>
            {#if agg.longest.site}from {agg.longest.site}{/if}
            ({monthYear(agg.longest.date_utc)})
          </li>
        {/if}
        {#if agg.busiest}
          <li>
            Busiest site: <strong>{agg.busiest.site}</strong> ({agg.busiest.flights} flights)
          </li>
        {/if}
        {#if agg.highest}
          <li>
            Max altitude:
            <strong>{formatAltitude(agg.highest.max_alt_m)}</strong>
            ({monthYear(agg.highest.date_utc)})
          </li>
        {/if}
        <li>
          Total climb: <strong>{formatAltitude(agg.alt_gain_m)}</strong>
        </li>
        {#if agg.bestClimb && agg.bestClimb.max_climb_ms > 0}
          <li>
            Best climb: <strong>{formatRate(agg.bestClimb.max_climb_ms)}</strong>
            ({monthYear(agg.bestClimb.date_utc)})
          </li>
        {/if}
      </ul>
    </section>

    <h2 class="h-section">
      Per year
      <span class="dim">
        · {agg.count} flights / {formatDurationShort(agg.duration_s)} /
        {formatDistanceGrouped(agg.distance_m)}
      </span>
    </h2>
    <section class="card chart-card">
      <div class="chips">
        {#each Object.entries(METRICS) as [key, m] (key)}
          <button class="chip" class:active={metric === key} onclick={() => (metric = key as typeof metric)}>
            {m.label}
          </button>
        {/each}
      </div>
      <BarChart bars={yearBars} />
    </section>

    <h2 class="h-section">Flights by month <span class="dim">· all years</span></h2>
    <section class="card chart-card">
      <BarChart bars={monthBars} height={200} />
    </section>

    <h2 class="h-section">
      {agg.sites.length} sites
      {#if agg.busiest}<span class="dim">· busiest: {agg.busiest.site}</span>{/if}
    </h2>
    <section class="card table-card">
      <table class="dense">
        <thead>
          <tr>
            <th>Site</th>
            <th
              class="sortable num"
              onclick={() => setSiteSort("flights")}
              aria-sort={siteSort === "flights" ? (siteSortDir === 1 ? "ascending" : "descending") : "none"}
            >
              Flights{#if siteSort === "flights"}<span class="arrow">{siteSortDir === 1 ? "▲" : "▼"}</span>{/if}
            </th>
            <th
              class="sortable num"
              onclick={() => setSiteSort("duration_s")}
              aria-sort={siteSort === "duration_s" ? (siteSortDir === 1 ? "ascending" : "descending") : "none"}
            >
              Airtime{#if siteSort === "duration_s"}<span class="arrow">{siteSortDir === 1 ? "▲" : "▼"}</span>{/if}
            </th>
            <th
              class="sortable num"
              onclick={() => setSiteSort("distance_m")}
              aria-sort={siteSort === "distance_m" ? (siteSortDir === 1 ? "ascending" : "descending") : "none"}
            >
              Distance{#if siteSort === "distance_m"}<span class="arrow">{siteSortDir === 1 ? "▲" : "▼"}</span>{/if}
            </th>
            <th
              class="sortable num"
              onclick={() => setSiteSort("max_alt_m")}
              aria-sort={siteSort === "max_alt_m" ? (siteSortDir === 1 ? "ascending" : "descending") : "none"}
            >
              Max alt{#if siteSort === "max_alt_m"}<span class="arrow">{siteSortDir === 1 ? "▲" : "▼"}</span>{/if}
            </th>
            <th class="num">Best climb</th>
            <th class="num">Last flight</th>
          </tr>
        </thead>
        <tbody>
          {#each sortedSites as s (s.site)}
            <tr>
              <td>{s.site}</td>
              <td class="num">{s.flights}</td>
              <td class="num">{formatDurationShort(s.duration_s)}</td>
              <td class="num">{formatDistance(s.distance_m)}</td>
              <td class="num">{formatAltitude(s.max_alt_m)}</td>
              <td class="num">{s.best_climb_ms > 0 ? formatRate(s.best_climb_ms) : "–"}</td>
              <td class="num dim">{monthYear(s.last_date)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .overview {
    max-width: 1040px;
    margin: 0 auto;
    padding: clamp(16px, 3vw, 36px) clamp(14px, 3vw, 28px) 64px;
  }

  .page-head {
    text-align: center;
    margin-bottom: 20px;
  }

  .page-head h1 {
    margin: 12px 0 2px;
    font-size: clamp(1.5rem, 5vmin, 2.1rem);
    font-weight: 700;
    line-height: 1.3;
    text-wrap: balance;
  }

  .page-head .sub {
    margin: 0;
    color: var(--text-dim);
    font-size: 0.85rem;
  }

  .prose-card {
    padding: 18px 22px;
  }

  .career-summary {
    margin: 0 0 14px;
    font-size: 0.95rem;
    line-height: 1.75;
    text-wrap: pretty;
  }

  .chart-card {
    padding: 16px 18px 8px;
  }

  .chips {
    display: flex;
    gap: 8px;
    margin-bottom: 10px;
  }

  .table-card {
    padding: 8px 12px;
    overflow-x: auto;
  }

  .status {
    color: var(--text-dim);
    text-align: center;
    padding: 48px 12px;
  }
  .status.error {
    color: var(--red);
  }

  code {
    background: var(--panel-raised);
    border: 1px solid var(--border);
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 0.85em;
  }
</style>

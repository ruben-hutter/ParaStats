<script lang="ts">
  // Flights (#/flights) — dense sortable table of every flight. Minimal
  // filters: year select + text search. Row click → flight detail.

  import { api, type Flight } from "../api";
  import {
    localDate,
    formatDuration,
    formatDistance,
    formatRate,
    formatAltitude,
  } from "../lib/fmt";
  import { navigate } from "../router";

  type SortKey =
    | "date"
    | "site"
    | "duration_s"
    | "distance_track_m"
    | "max_alt_m"
    | "alt_gain_m"
    | "max_climb_ms";

  let flights = $state<Flight[]>([]);
  let error = $state("");
  let loading = $state(true);
  let year = $state("all");
  let search = $state("");
  let sortKey = $state<SortKey>("date");
  let sortDir = $state<1 | -1>(-1); // newest first by default

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

  let years = $derived(
    [...new Set(flights.map((f) => f.date_utc.slice(0, 4)))].sort().reverse(),
  );

  function cmp(a: Flight, b: Flight): number {
    const k = sortKey;
    if (k === "date") {
      return a.takeoff_time_utc.localeCompare(b.takeoff_time_utc);
    }
    if (k === "site") {
      return (a.site ?? "").localeCompare(b.site ?? "");
    }
    return a[k] - b[k];
  }

  let rows = $derived(
    flights
      .filter((f) => {
        if (year !== "all" && !f.date_utc.startsWith(year)) return false;
        const q = search.trim().toLowerCase();
        if (!q) return true;
        return (
          (f.site ?? "").toLowerCase().includes(q) ||
          f.filename.toLowerCase().includes(q) ||
          f.date_utc.includes(q)
        );
      })
      .sort((a, b) => cmp(a, b) * sortDir),
  );

  const columns: Array<{ key: SortKey | null; label: string; num?: boolean }> = [
    { key: "date", label: "Date" },
    { key: "site", label: "Site" },
    { key: "duration_s", label: "Duration", num: true },
    { key: "distance_track_m", label: "Distance", num: true },
    { key: "max_alt_m", label: "Max alt", num: true },
    { key: "alt_gain_m", label: "Alt gain", num: true },
    { key: "max_climb_ms", label: "Max climb", num: true },
  ];

  function setSort(key: SortKey) {
    if (sortKey === key) {
      sortDir = sortDir === 1 ? -1 : 1;
    } else {
      sortKey = key;
      sortDir = key === "date" || key === "site" ? -1 : 1; // text: Z→A, numbers: big→small
    }
  }

  function onRowKeydown(e: KeyboardEvent, id: string) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      navigate(`#/flight/${id}`);
    }
  }
</script>

<div class="flights">
  <header class="head">
    <h1 class="h-section">
      Flights <span class="dim">· {rows.length} of {flights.length}</span>
    </h1>
    <div class="controls">
      <select bind:value={year} aria-label="Filter by year">
        <option value="all">all years</option>
        {#each years as y (y)}
          <option value={y}>{y}</option>
        {/each}
      </select>
      <input type="search" placeholder="search site, file, date…" bind:value={search} />
    </div>
  </header>

  {#if loading}
    <p class="status">loading…</p>
  {:else if error}
    <p class="status error">{error}</p>
  {:else}
    <section class="card table-card">
      <table class="dense">
        <thead>
          <tr>
            {#each columns as c (c.label)}
              {#if c.key}
                <th class="sortable" class:num={c.num} onclick={() => setSort(c.key as SortKey)} aria-sort={sortKey === c.key ? (sortDir === 1 ? "ascending" : "descending") : "none"}>
                  {c.label}
                  {#if sortKey === c.key}<span class="arrow">{sortDir === 1 ? "▲" : "▼"}</span>{/if}
                </th>
              {/if}
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as f (f.id)}
            <tr
              class="row"
              role="link"
              tabindex="0"
              onclick={() => navigate(`#/flight/${f.id}`)}
              onkeydown={(e) => onRowKeydown(e, f.id)}
            >
              <td class="dim">{localDate(f.takeoff_time_utc)}</td>
              <td class="site">{f.site || f.filename}</td>
              <td class="num">{formatDuration(f.duration_s)}</td>
              <td class="num">{formatDistance(f.distance_track_m)}</td>
              <td class="num">{formatAltitude(f.max_alt_m)}</td>
              <td class="num">{formatAltitude(f.alt_gain_m)}</td>
              <td class="num">{f.max_climb_ms > 0 ? formatRate(f.max_climb_ms) : "–"}</td>
            </tr>
          {:else}
            <tr><td colspan="7" class="empty">no flights match</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  {/if}
</div>

<style>
  .flights {
    max-width: 1040px;
    margin: 0 auto;
    padding: clamp(16px, 3vw, 32px) clamp(14px, 3vw, 28px) 64px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px 16px;
  }

  .head .h-section {
    margin: 0;
  }

  .controls {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .controls input[type="search"] {
    width: min(280px, 60vw);
  }

  .table-card {
    padding: 8px 12px;
    overflow-x: auto;
  }

  .row {
    cursor: pointer;
  }

  .site {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-weight: 600;
    font-size: 0.86rem;
  }

  .empty {
    text-align: center;
    color: var(--text-dim);
    padding: 24px 0 !important;
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

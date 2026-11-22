<script lang="ts">
  import { route, navigate } from "./router";
  import { theme, toggleTheme } from "./theme";
  import Overview from "./views/Overview.svelte";
  import Flights from "./views/Flights.svelte";

  // The map views pull in maplibre-gl + deck.gl (~1.5 MB minified), so they
  // are code-split into their own chunks and only fetched when a map route
  // is opened. Overview and Flights stay in the initial bundle — both are
  // light, and keeping them eager avoids a blank frame on first paint.
  const Thermals = () => import("./views/Thermals.svelte").then((m) => m.default);
  const FlightDetail = () => import("./views/FlightDetail.svelte").then((m) => m.default);

  function go(e: MouseEvent, path: string) {
    e.preventDefault();
    navigate(path);
  }

  const links: Array<{ path: string; label: string; name: string }> = [
    { path: "#/", label: "Overview", name: "overview" },
    { path: "#/flights", label: "Flights", name: "flights" },
    { path: "#/thermals", label: "Thermals", name: "thermals" },
  ];
</script>

<header class="topbar">
  <a href="#/" class="brand" onclick={(e) => go(e, "#/")}>
    para<em>stats</em>
  </a>
  <nav>
    {#each links as l (l.path)}
      <a
        href={l.path}
        class:active={$route.name === l.name}
        onclick={(e) => go(e, l.path)}>{l.label}</a
      >
    {/each}
  </nav>
  <div class="spacer"></div>
  <button class="theme-toggle" onclick={toggleTheme} aria-label="Toggle light/dark theme" title="Toggle theme">
    {#if $theme === "dark"}
      <!-- sun -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
      </svg>
    {:else}
      <!-- moon -->
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8z" />
      </svg>
    {/if}
  </button>
</header>

<main class="page" class:fullbleed={$route.name === "thermals"}>
  {#if $route.name === "flight" && $route.id}
    {#await FlightDetail()}<p class="chunk-status">loading flight view…</p>{:then Detail}
      <Detail id={$route.id} />
    {:catch err}
      <p class="chunk-status error">failed to load flight view: {err}</p>
    {/await}
  {:else if $route.name === "flights"}
    <Flights />
  {:else if $route.name === "thermals"}
    {#await Thermals()}<p class="chunk-status">loading map…</p>{:then ThermalsView}
      <ThermalsView />
    {:catch err}
      <p class="chunk-status error">failed to load map view: {err}</p>
    {/await}
  {:else}
    <Overview />
  {/if}
</main>

<style>
  /* Chunk-loading fallback for the lazy map views. Kept minimal — each view
     styles its own .status once it has mounted. */
  .chunk-status {
    padding: 24px 0;
    color: var(--text-dim, #9aa5c4);
  }
  .chunk-status.error {
    color: #dc2626;
  }
</style>

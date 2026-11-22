<script lang="ts">
  import { route, navigate } from "./router";
  import { theme, toggleTheme } from "./theme";
  import Overview from "./views/Overview.svelte";
  import Flights from "./views/Flights.svelte";
  import Thermals from "./views/Thermals.svelte";
  import FlightDetail from "./views/FlightDetail.svelte";

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
    <FlightDetail id={$route.id} />
  {:else if $route.name === "flights"}
    <Flights />
  {:else if $route.name === "thermals"}
    <Thermals />
  {:else}
    <Overview />
  {/if}
</main>

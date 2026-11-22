<script lang="ts">
  import { route, navigate } from "./router";
  import Flights from "./views/Flights.svelte";
  import FlightDetail from "./views/FlightDetail.svelte";
  import Stats from "./views/Stats.svelte";

  function go(e: MouseEvent, path: string) {
    e.preventDefault();
    navigate(path);
  }
</script>

<header class="topbar">
  <a
    href="#/"
    class="brand"
    onclick={(e) => go(e, "#/")}
    aria-label="parastats home">parastats</a
  >
  <nav>
    <a href="#/" class:active={$route.name === "flights"} onclick={(e) => go(e, "#/")}>Map</a>
    <a
      href="#/stats"
      class:active={$route.name === "stats"}
      onclick={(e) => go(e, "#/stats")}>Stats</a>
  </nav>
</header>

<main>
  {#if $route.name === "flight" && $route.id}
    <FlightDetail id={$route.id} />
  {:else if $route.name === "stats"}
    <Stats />
  {:else}
    <Flights />
  {/if}
</main>

<style>
  main {
    position: absolute;
    inset: 48px 0 0 0;
  }
</style>

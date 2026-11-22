<script lang="ts">
  // Shared MapLibre + deck.gl canvas. One instance per view; deck layers are
  // passed in as a prop array and synced to the overlay whenever the array
  // identity changes (Svelte 5 $effect) — deck.gl wants stable layer props,
  // so parent views build fresh layer arrays only when their inputs change
  // and we just forward them with setProps.
  //
  // The basemap follows the theme: CARTO Positron (light) / Dark Matter.
  // (The original OpenFreeMap Liberty has no dark style; the CARTO GL pair
  // is equally free/keyless and matches the paper/ink + near-black faces.)

  import { onMount } from "svelte";
  import maplibregl from "maplibre-gl";
  import type { Layer } from "@deck.gl/core";
  import { MapboxOverlay } from "@deck.gl/mapbox";
  import "maplibre-gl/dist/maplibre-gl.css";

  let {
    layers,
    fitBounds = null,
    dark = false,
    kk7 = $bindable(false),
    onReady,
  }: {
    layers: Layer[];
    /** [[west, south], [east, north]] — camera fits this when it changes. */
    fitBounds?: [[number, number], [number, number]] | null;
    /** Dark basemap (CARTO Dark Matter) vs light (Positron). */
    dark?: boolean;
    /** kk7 thermal comparison raster overlay toggle. */
    kk7?: boolean;
    onReady?: (map: maplibregl.Map) => void;
  } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map;
  let overlay: MapboxOverlay | null = null;
  let ready = $state(false);
  let appliedStyle = "";

  const BASEMAP_STYLE = (dark: boolean) =>
    dark
      ? "https://basemaps.cartocdn.com/gl/dark-matter-gl-style/style.json"
      : "https://basemaps.cartocdn.com/gl/positron-gl-style/style.json";

  // kk7 thermal/skyways tile overlay. TMS y-scheme — MapLibre's
  // `scheme: "tms"` on the source handles the flip.
  const KK7_TILES_URL = "https://thermal.kk7.ch/tiles/skyways_all_all/{z}/{x}/{y}.png";
  const KK7_SOURCE_ID = "kk7-thermal";
  const KK7_LAYER_ID = "kk7-thermal-overlay";

  onMount(() => {
    map = new maplibregl.Map({
      container,
      style: BASEMAP_STYLE(dark),
      center: [8.9, 46.1], // Locarno area; recentered on data once loaded
      zoom: 9,
    });
    map.addControl(new maplibregl.NavigationControl(), "top-right");
    overlay = new MapboxOverlay({ layers: [] });
    map.addControl(overlay);
    onReady?.(map);
    ready = true;
    appliedStyle = BASEMAP_STYLE(dark);

    return () => {
      overlay?.finalize();
      map.remove();
      overlay = null;
      ready = false;
    };
  });

  // Swap basemap style with the theme. setStyle wipes runtime sources, so
  // the kk7 effect below re-runs (it depends on `dark`) and re-adds them.
  $effect(() => {
    const url = BASEMAP_STYLE(dark);
    if (!ready || !map || url === appliedStyle) return;
    appliedStyle = url;
    map.setStyle(url);
  });

  // Sync deck layers whenever the parent rebuilds the array.
  $effect(() => {
    void layers;
    overlay?.setProps({ layers });
  });

  // Fit camera to data bounds (once per new value).
  $effect(() => {
    if (fitBounds && map) {
      void fitBounds;
      map.fitBounds(fitBounds, { padding: 60, animate: false });
    }
  });

  // kk7 raster overlay on/off (and back on after a basemap swap).
  function syncKk7() {
    if (!map) return;
    if (kk7 && !map.getSource(KK7_SOURCE_ID)) {
      map.addSource(KK7_SOURCE_ID, {
        type: "raster",
        tiles: [KK7_TILES_URL],
        tileSize: 256,
        scheme: "tms",
        attribution: "thermal.kk7.ch",
        maxzoom: 14,
      });
      map.addLayer({
        id: KK7_LAYER_ID,
        type: "raster",
        source: KK7_SOURCE_ID,
        paint: { "raster-opacity": 0.45 },
      });
    } else if (!kk7 && map.getSource(KK7_SOURCE_ID)) {
      if (map.getLayer(KK7_LAYER_ID)) map.removeLayer(KK7_LAYER_ID);
      map.removeSource(KK7_SOURCE_ID);
    }
  }

  $effect(() => {
    void dark;
    void kk7;
    void ready;
    if (!map || !ready) return;
    if (map.isStyleLoaded()) {
      syncKk7();
    } else {
      // Style still loading (fresh setStyle) — sync once it settles.
      map.once("style.load", syncKk7);
    }
  });
</script>

<div bind:this={container} class="map"></div>

<style>
  .map {
    position: absolute;
    inset: 0;
  }
</style>

<script lang="ts">
  // Hand-rolled SVG altitude profile — no chart lib (keep the bundle light).
  // X axis is time, Y axis altitude; a vertical marker tracks the scrub
  // position and mouse movement scrubs back (synced with the slider).
  // All colours read the active theme via CSS custom properties.

  import { altitudeColor, rgbCss } from "./colors";
  import { formatAltitude } from "./fmt";

  let {
    times,
    alts,
    progress = $bindable(0),
    onScrub,
  }: {
    /** Unix timestamps, ascending. */
    times: number[];
    /** Baro-preferred altitudes, metres. */
    alts: number[];
    progress?: number;
    onScrub?: (p: number) => void;
  } = $props();

  const W = 1000;
  const H = 180;
  const PAD = 6;

  let svg: SVGSVGElement;

  // Downsample to ≤ PROFILE_MAX_POINTS vertices. Long flights log 10k+
  // fixes; that many SVG paths would stall the browser, and the profile is
  // a visual — 600 vertices carry all the shape.
  const PROFILE_MAX_POINTS = 600;

  let sampledIdx = $derived.by(() => {
    const n = times.length;
    if (n <= PROFILE_MAX_POINTS) return [...Array(n).keys()];
    const stride = (n - 1) / (PROFILE_MAX_POINTS - 1);
    const idx: number[] = [];
    for (let i = 0; i < PROFILE_MAX_POINTS - 1; i++) idx.push(Math.round(i * stride));
    idx.push(n - 1); // always include the last fix
    return idx;
  });

  let aMin = $derived(
    alts.reduce((m, a) => Math.min(m, a), Number.POSITIVE_INFINITY),
  );
  let aMax = $derived(
    alts.reduce((m, a) => Math.max(m, a), Number.NEGATIVE_INFINITY),
  );
  let aSpan = $derived(Math.max(1, aMax - aMin));
  let t0 = $derived(times[0] ?? 0);
  let t1 = $derived(times[times.length - 1] ?? 1);

  function x(t: number): number {
    return PAD + ((t - t0) / Math.max(1, t1 - t0)) * (W - 2 * PAD);
  }

  function y(a: number): number {
    return H - PAD - ((a - aMin) / aSpan) * (H - 2 * PAD);
  }

  // Line segments, coloured by altitude band so the profile matches the map.
  let segments = $derived.by(() => {
    const segs: Array<{ d: string; color: string }> = [];
    const idxs = sampledIdx;
    for (let i = 1; i < idxs.length; i++) {
      const ta = times[idxs[i - 1]];
      const tb = times[idxs[i]];
      segs.push({
        d: `M ${x(ta)} ${y(alts[idxs[i - 1]])} L ${x(tb)} ${y(alts[idxs[i]])}`,
        color: rgbCss(altitudeColor(alts[idxs[i - 1]])),
      });
    }
    return segs;
  });

  let currentIdx = $derived(Math.min(times.length - 1, Math.round(progress * (times.length - 1))));
  let markerX = $derived(times.length > 0 ? x(times[currentIdx]) : PAD);
  let markerY = $derived(times.length > 0 ? y(alts[currentIdx]) : H - PAD);

  function scrubFromEvent(e: MouseEvent) {
    const rect = svg.getBoundingClientRect();
    const p = (e.clientX - rect.left) / rect.width;
    onScrub?.(Math.min(1, Math.max(0, p)));
  }
</script>

<div class="profile">
  <svg
    bind:this={svg}
    viewBox="0 0 {W} {H}"
    preserveAspectRatio="none"
    role="img"
    aria-label="altitude profile"
    onmousemove={scrubFromEvent}
  >
    {#each segments as s, i (i)}
      <path d={s.d} stroke={s.color} stroke-width="1.5" fill="none" />
    {/each}
    <line class="marker-line" x1={markerX} x2={markerX} y1={0} y2={H} />
    <circle class="marker-dot" cx={markerX} cy={markerY} r="4" />
  </svg>
  <div class="axis">
    <span>{formatAltitude(aMin)}</span>
    <span>{formatAltitude(aMax)}</span>
  </div>
</div>

<style>
  .profile {
    position: relative;
    width: 100%;
    padding: 4px 0 0;
  }

  svg {
    width: 100%;
    height: 170px;
    display: block;
    cursor: crosshair;
    background: var(--panel-raised);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .marker-line {
    stroke: var(--text-dim);
    stroke-width: 1;
    opacity: 0.7;
  }

  .marker-dot {
    fill: var(--text);
    stroke: var(--bg);
    stroke-width: 1.5;
  }

  .axis {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-dim);
    padding: 3px 6px 0;
    font-variant-numeric: tabular-nums;
  }
</style>

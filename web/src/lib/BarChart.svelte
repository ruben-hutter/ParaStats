<script lang="ts">
  // Hand-rolled SVG bar chart (no chart lib). Used for the per-year and
  // per-month sections on the Overview page. Colours read the theme via
  // CSS custom properties — no JS colour logic here.

  let {
    bars,
    height = 240,
  }: {
    /** One bar: label under the bar, `display` printed above it. */
    bars: Array<{ label: string; value: number; display: string }>;
    height?: number;
  } = $props();

  const W = 820;
  const PAD_TOP = 26;
  const PAD_BOTTOM = 26;
  const PAD_X = 14;

  let max = $derived(Math.max(...bars.map((b) => b.value), 1));

  // Pre-computed geometry, recomputed when bars change.
  let geo = $derived.by(() => {
    const n = bars.length;
    const innerW = W - 2 * PAD_X;
    const slot = innerW / Math.max(1, n);
    const barW = Math.min(72, slot * 0.62);
    const plotH = height - PAD_TOP - PAD_BOTTOM;
    return bars.map((b, i) => {
      const x = PAD_X + slot * i + (slot - barW) / 2;
      const h = Math.max(b.value > 0 ? 3 : 0, (b.value / max) * plotH);
      const y = PAD_TOP + plotH - h;
      const cx = x + barW / 2;
      return { ...b, x, y, w: barW, h, cx };
    });
  });

  // 3 faint horizontal gridlines at 25/50/75% of the tallest bar.
  let gridlines = $derived.by(() => {
    const plotH = height - PAD_TOP - PAD_BOTTOM;
    return [0.25, 0.5, 0.75].map((f) => ({ y: PAD_TOP + plotH * (1 - f), v: f * max }));
  });
</script>

<svg
  viewBox="0 0 {W} {height}"
  preserveAspectRatio="xMidYMid meet"
  role="img"
  aria-label="bar chart"
>
  <defs>
    <linearGradient id="ps-cool-bar" x1="0" y1="0" x2="1" y2="1">
      <stop class="cool-1" offset="0%" />
      <stop class="cool-2" offset="100%" />
    </linearGradient>
  </defs>
  {#each gridlines as g, i (i)}
    <line class="grid" x1={PAD_X} x2={W - PAD_X} y1={g.y} y2={g.y} />
  {/each}
  {#each geo as b, i (i)}
    {#if b.value > 0}
      <text class="val" x={b.cx} y={b.y - 7}>{b.display}</text>
    {/if}
    <rect class="bar" x={b.x} y={b.y} width={b.w} height={Math.max(b.h, 1)} rx="5" />
    <text class="lbl" x={b.cx} y={height - 8}>{b.label}</text>
  {/each}
  <line class="axis" x1={PAD_X} x2={W - PAD_X} y1={height - PAD_BOTTOM} y2={height - PAD_BOTTOM} />
</svg>

<style>
  .cool-1 {
    stop-color: var(--chart-c1);
  }
  .cool-2 {
    stop-color: var(--chart-c2);
  }
  .bar {
    fill: url(#ps-cool-bar);
    opacity: 0.92;
  }
  .bar:hover {
    opacity: 1;
  }
  .grid {
    stroke: var(--chart-grid);
    stroke-width: 1;
  }
  .axis {
    stroke: var(--border);
    stroke-width: 1;
  }
  .val {
    fill: var(--text-dim);
    font-size: 11.5px;
    text-anchor: middle;
    font-variant-numeric: tabular-nums;
  }
  .lbl {
    fill: var(--text-dim);
    font-size: 12px;
    text-anchor: middle;
  }
</style>

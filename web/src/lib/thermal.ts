// Thermal density rendering. Ported from flightmap web/src/main.ts.
//
// Build a true density field (Float32 accumulation per pixel), map through
// the colour ramp, render to a canvas once per filter/knob change — pan/zoom
// just transform a quad (deck.gl BitmapLayer), no per-frame work, no lag.
//
// Why not canvas radial gradients + 'lighter' compositing: the colour ramp
// has light middle tones (pale blue, peach) that saturate to white under
// additive blending → the "white stains" look. Per-pixel
// density-then-colormap avoids that — each output pixel looks up its colour
// from a single density value, so saturating to white is impossible.
//
// Why not deck.gl HeatmapLayer: it recomputes density every frame — laggy
// with a full season of climbs. This canvas approach was the fix in
// flightmap and is kept verbatim in spirit; the only change: input is binned
// cells from /api/layers/thermals rather than raw per-climb points (the
// backend pre-aggregates via Mercator binning).

import type { ThermalCellProperties } from "../api";
import { densityColor } from "./colors";

export interface ThermalKnobs {
  /** Gaussian kernel width in pixels. Higher = smoother but blurrier. */
  sigma: number;
  /** m/s mapped to the top of the colour ramp. */
  maxRate: number;
  /** m/s where alpha saturates. */
  fullOpacityRate: number;
}

export const DEFAULT_KNOBS: ThermalKnobs = {
  sigma: 8,
  maxRate: 6.0,
  fullOpacityRate: 2.5,
};

export interface ThermalTexture {
  canvas: HTMLCanvasElement;
  /** [left, bottom, right, top] in lon/lat — deck.gl BitmapLayer bounds. */
  bounds: [number, number, number, number];
}

type CellFeature = GeoJSON.Feature<GeoJSON.Point, ThermalCellProperties>;

export function buildThermalTexture(
  cells: CellFeature[],
  knobs: ThermalKnobs,
): ThermalTexture | null {
  if (cells.length === 0) return null;

  // ---- Bounds ----
  let minLon = Infinity;
  let minLat = Infinity;
  let maxLon = -Infinity;
  let maxLat = -Infinity;
  for (const f of cells) {
    const [lon, lat] = f.geometry.coordinates;
    if (lon < minLon) minLon = lon;
    if (lat < minLat) minLat = lat;
    if (lon > maxLon) maxLon = lon;
    if (lat > maxLat) maxLat = lat;
  }
  const pad = 0.01;
  minLon -= pad;
  minLat -= pad;
  maxLon += pad;
  maxLat += pad;

  const aspect = (maxLon - minLon) / (maxLat - minLat);
  const width = 1024;
  const height = Math.max(1, Math.round(width / aspect));

  // ---- Gaussian kernel (precomputed) ----
  const { sigma } = knobs;
  const kernelRadius = Math.ceil(sigma * 3);
  const kernelSize = kernelRadius * 2 + 1;
  const kernel = new Float32Array(kernelSize * kernelSize);
  for (let dy = -kernelRadius; dy <= kernelRadius; dy++) {
    for (let dx = -kernelRadius; dx <= kernelRadius; dx++) {
      const r2 = dx * dx + dy * dy;
      kernel[(dy + kernelRadius) * kernelSize + (dx + kernelRadius)] = Math.exp(
        -r2 / (2 * sigma * sigma),
      );
    }
  }

  // ---- Accumulate density (MAX, not SUM) ----
  // Per pixel, store the strongest peak climb rate that touched it (scaled
  // by the Gaussian falloff). This is "what's the best thermal I could
  // expect at this spot" — multiple weak thermals don't combine into a
  // strong colour. A single 5 m/s climb paints red; ten 1 m/s climbs stay
  // blue. Cell `count` does NOT contribute: repetition isn't strength.
  const density = new Float32Array(width * height);
  const xScale = width / (maxLon - minLon);
  const yScale = height / (maxLat - minLat);

  for (const f of cells) {
    const [lon, lat] = f.geometry.coordinates;
    const px = Math.round((lon - minLon) * xScale);
    const py = Math.round((maxLat - lat) * yScale); // Y flipped
    const peak = Math.max(0.5, f.properties.peak_climb_ms);

    for (let dy = -kernelRadius; dy <= kernelRadius; dy++) {
      const y = py + dy;
      if (y < 0 || y >= height) continue;
      const rowBase = y * width;
      const kernelRow = (dy + kernelRadius) * kernelSize;
      for (let dx = -kernelRadius; dx <= kernelRadius; dx++) {
        const x = px + dx;
        if (x < 0 || x >= width) continue;
        const k = kernel[kernelRow + (dx + kernelRadius)];
        const contribution = peak * k;
        const idx = rowBase + x;
        if (contribution > density[idx]) density[idx] = contribution;
      }
    }
  }

  // ---- Density → RGBA (absolute scale, no normalisation) ----
  // Colours are tied to absolute m/s values, so the meaning is stable
  // across knob changes: 5+ m/s is always red, 1 m/s is always blue.
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d")!;
  const imageData = ctx.createImageData(width, height);
  const data = imageData.data;

  for (let i = 0; i < density.length; i++) {
    const rate = density[i]; // peak m/s at this pixel (gauss-scaled)
    if (rate < 0.3) continue; // skip near-zero — keep canvas transparent
    const colorT = Math.min(1, rate / knobs.maxRate);
    const [r, g, b] = densityColor(colorT);
    const alphaT = Math.min(1, rate / knobs.fullOpacityRate);
    data[i * 4] = r;
    data[i * 4 + 1] = g;
    data[i * 4 + 2] = b;
    data[i * 4 + 3] = Math.round(alphaT * 255);
  }
  ctx.putImageData(imageData, 0, 0);

  return { canvas, bounds: [minLon, minLat, maxLon, maxLat] };
}

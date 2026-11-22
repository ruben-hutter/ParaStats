// Colour ramps shared by the map layers. Ported from flightmap web/src/main.ts.
//
// Identity (thermal.kk7.ch): the COOL ramp (cyan → blue → indigo, kk7's
// --cool) carries skyway/altitude data; the HOT ramp (amber → orange →
// red, kk7's --hot) carries thermal/climb data. The thermal *density*
// canvas keeps its spectral blue→red ramp (same family kk7 itself uses
// for thermal probability).

export type RGB = [number, number, number];

function rampColor(stops: Array<[number, RGB]>, v: number): RGB {
  if (v <= stops[0][0]) return stops[0][1];
  for (let i = 1; i < stops.length; i++) {
    if (v <= stops[i][0]) {
      const [t0, c0] = stops[i - 1];
      const [t1, c1] = stops[i];
      const f = (v - t0) / (t1 - t0);
      return [
        Math.round(c0[0] + (c1[0] - c0[0]) * f),
        Math.round(c0[1] + (c1[1] - c0[1]) * f),
        Math.round(c0[2] + (c1[2] - c0[2]) * f),
      ];
    }
  }
  return stops[stops.length - 1][1];
}

// Altitude colour ramp for track layers (metres MSL) — the kk7 COOL ramp:
// valley blues → cyan → indigo tint → near-white at wave altitudes.
const ALTITUDE_STOPS: Array<[number, RGB]> = [
  [0, [16, 64, 140]], // valley floor: deep blue
  [800, [0, 110, 210]], // scratch: blue
  [1600, [0, 178, 255]], // cloudbase-ish: cyan (#00b2ff)
  [2400, [93, 128, 255]], // high: blue-violet
  [3200, [138, 155, 255]], // wave: kk7 tint (#8a9bff)
  [4000, [235, 240, 255]], // very high: near-white
];

export function altitudeColor(altM: number): RGB {
  return rampColor(ALTITUDE_STOPS, altM);
}

// HOT ramp (kk7 --hot: amber → orange → red) for climb-rate data —
// hotspot markers, climb tints. v ∈ [0,1], 1 = hottest.
const HOT_STOPS: Array<[number, RGB]> = [
  [0.0, [255, 198, 0]], // amber #ffc600
  [0.5, [255, 104, 0]], // orange #ff6800
  [1.0, [242, 8, 0]], // red #f20800
];

/** Climb-rate fraction (m/s mapped over [0, maxRate]) → kk7 hot colour. */
export function hotColor(t: number): RGB {
  return rampColor(HOT_STOPS, Math.min(1, Math.max(0, t)));
}

// Spectral ramp for the thermal density canvas (t ∈ [0,1] = climb-rate
// fraction) — blue (low) → cyan → yellow → red (high), the same family
// kk7 uses for its thermal-probability ramp.
const DENSITY_STOPS: Array<[number, RGB]> = [
  [0.0, [33, 102, 172]], // blue (low density)
  [0.2, [103, 169, 207]],
  [0.4, [209, 229, 240]],
  [0.6, [253, 219, 199]],
  [0.8, [239, 138, 98]],
  [1.0, [178, 24, 43]], // red (high density)
];

export function densityColor(t: number): RGB {
  return rampColor(DENSITY_STOPS, t);
}

export function rgbCss([r, g, b]: RGB): string {
  return `rgb(${r}, ${g}, ${b})`;
}

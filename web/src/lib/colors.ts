// Colour ramps shared by the map layers. Ported from flightmap web/src/main.ts.

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

// Altitude colour ramp for track layers (metres MSL). Thresholds cover the
// paragliding envelope: 0=ground (green), 1000=scratch (yellow),
// 2000=cloudbase-ish (orange), 3500+=wave/high (purple/white).
const ALTITUDE_STOPS: Array<[number, RGB]> = [
  [0, [80, 140, 60]],
  [800, [180, 200, 80]],
  [1600, [240, 180, 60]],
  [2400, [220, 110, 50]],
  [3200, [160, 80, 160]],
  [4000, [240, 240, 255]],
];

export function altitudeColor(altM: number): RGB {
  return rampColor(ALTITUDE_STOPS, altM);
}

// Same 6-stop RdYlBu_r ramp as deck.gl's HeatmapLayer colorRange, for the
// thermal density canvas (t ∈ [0,1] = climb-rate fraction).
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

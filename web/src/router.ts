// Tiny hash router. Routes:
//   #/             → overview (paraglidingstats-style landing page)
//   #/flights      → dense sortable flight table
//   #/thermals     → thermal heatmap + skyway map
//   #/flight/:id   → flight detail
//
// Kept hand-rolled — four routes don't justify a router dependency.

import { readable } from "svelte/store";

export interface Route {
  name: "overview" | "flights" | "thermals" | "flight";
  id?: string;
}

function parse(hash: string): Route {
  const path = hash.replace(/^#/, "");
  const m = path.match(/^\/flight\/([^/?]+)/);
  if (m) return { name: "flight", id: decodeURIComponent(m[1]) };
  if (path.startsWith("/flights")) return { name: "flights" };
  if (path.startsWith("/thermals")) return { name: "thermals" };
  return { name: "overview" };
}

export const route = readable<Route>(parse(location.hash), (set) => {
  const onHash = () => set(parse(location.hash));
  window.addEventListener("hashchange", onHash);
  return () => window.removeEventListener("hashchange", onHash);
});

export function navigate(path: string) {
  location.hash = path;
}

// Tiny hash router. Routes:
//   #/            → flights map
//   #/flight/:id  → flight detail
//   #/stats       → stats overview
//
// Kept hand-rolled — three routes don't justify a router dependency.

import { readable } from "svelte/store";

export interface Route {
  name: "flights" | "flight" | "stats";
  id?: string;
}

function parse(hash: string): Route {
  const path = hash.replace(/^#/, "");
  const m = path.match(/^#?\/flight\/([^/?]+)/);
  if (m) return { name: "flight", id: decodeURIComponent(m[1]) };
  if (path.startsWith("/stats")) return { name: "stats" };
  return { name: "flights" };
}

export const route = readable<Route>(parse(location.hash), (set) => {
  const onHash = () => set(parse(location.hash));
  window.addEventListener("hashchange", onHash);
  return () => window.removeEventListener("hashchange", onHash);
});

export function navigate(path: string) {
  location.hash = path;
}

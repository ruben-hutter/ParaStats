// Theme state: light / dark with system default, persisted in
// localStorage ("parastats-theme": "system" | "light" | "dark").
//
// The inline bootstrap script in index.html sets `data-theme` on <html>
// before first paint; this module owns the reactive state afterwards and
// keeps the attribute + theme-color meta in sync (including live system
// changes while in "system" mode).

import { writable } from "svelte/store";

export type ThemePref = "system" | "light" | "dark";
export type Theme = "light" | "dark";

const STORAGE_KEY = "parastats-theme";
const THEME_COLORS: Record<Theme, string> = {
  dark: "#070b24",
  light: "#f5f7fb",
};

function readPref(): ThemePref {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

export const themePref = writable<ThemePref>(readPref());
export const theme = writable<Theme>(resolve(readPref()));

function resolve(pref: ThemePref): Theme {
  if (pref !== "system") return pref;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function apply(t: Theme) {
  document.documentElement.setAttribute("data-theme", t);
  // Both meta tags (dark + light media variants) follow the effective theme
  // so the browser chrome matches even after a manual override.
  for (const meta of document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')) {
    meta.content = THEME_COLORS[t];
  }
}

// Follow the OS while in "system" mode.
const media = window.matchMedia("(prefers-color-scheme: dark)");
media.addEventListener("change", () => {
  const pref = readPref();
  themePref.set(pref);
  theme.set(resolve(pref));
});

themePref.subscribe((pref) => {
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    /* private mode — keep in-memory only */
  }
  theme.set(resolve(pref));
});

theme.subscribe((t) => {
  if (document.documentElement.getAttribute("data-theme") !== t) apply(t);
});

export function toggleTheme() {
  themePref.update((pref) => {
    // From "system", toggle to the opposite of what's currently effective.
    const current = resolve(pref);
    return current === "dark" ? "light" : "dark";
  });
}

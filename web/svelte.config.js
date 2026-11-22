import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// Without this file vite-plugin-svelte falls back to bare defaults (and logs
// a warning on every build). Svelte 5 handles <script lang="ts"> natively,
// but vitePreprocess is still needed for TS/PostCSS in markup and styles.
export default {
  preprocess: vitePreprocess(),
};

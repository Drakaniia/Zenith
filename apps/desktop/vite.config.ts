import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

const dir = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// The chrome UI is served to the main Tauri webview only. Tab webviews never load it.
export default defineConfig({
  plugins: [svelte()],
  // Keep in sync with `paths` in tsconfig.json. Plain .ts modules that Node's test runner loads
  // use relative imports instead (Node resolves no aliases).
  resolve: {
    alias: {
      $features: dir("./src/features"),
      $shared: dir("./src/shared"),
      $platform: dir("./src/platform"),
    },
  },
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome120",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: false,
    cssCodeSplit: true,
    modulePreload: { polyfill: false },
    chunkSizeWarningLimit: 400,
  },
});

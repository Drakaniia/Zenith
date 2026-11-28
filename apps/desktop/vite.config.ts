import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";
import { cpSync } from "node:fs";

const dir = (path: string) => fileURLToPath(new URL(path, import.meta.url));

// pdf.js data the PDF viewer loads at run time: CMaps, standard fonts, colour profiles and
// image decoders (not the scripting sandbox: PDF JavaScript stays off).
const pdfjsResources = () => ({
  name: "pdfjs-resources",
  apply: "build" as const,
  closeBundle() {
    for (const d of ["cmaps", "standard_fonts", "iccs", "wasm"]) {
      cpSync(dir(`./node_modules/pdfjs-dist/${d}`), dir(`./dist/pdfjs/${d}`), {
        recursive: true,
        filter: (src) => !/quickjs/i.test(src),
      });
    }
  },
});

// index.html is the chrome UI, served to the main window only. pdf.html is the PDF viewer,
// served at zenith://pdf/ to tab webviews (see src-tauri/src/pdf).
export default defineConfig({
  plugins: [svelte(), pdfjsResources()],
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
    rollupOptions: {
      input: { main: dir("./index.html"), pdf: dir("./pdf.html") },
    },
  },
});

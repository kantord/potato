import { defineConfig } from "vite";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    host: host || false,
    port: 1420,
    strictPort: true,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src/**"] },
  },
  build: {
    lib: {
      entry: resolve(__dirname, "src-frontend/polyfill.ts"),
      name: "PotatoPolyfill",
      formats: ["iife"],
      fileName: () => "polyfill.js",
    },
    outDir: "dist",
    minify: true,
  },
});

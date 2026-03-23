import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  build: {
    lib: {
      entry: resolve(__dirname, "src/polyfill.ts"),
      name: "PotatoPolyfill",
      formats: ["iife"],
      fileName: () => "polyfill.js",
    },
    outDir: "dist",
    minify: true,
  },
});

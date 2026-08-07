// Copyright (c) 2026 Michael Saunders
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;
const packageVersion = JSON.parse(
  readFileSync(new URL("./package.json", import.meta.url), "utf8"),
) as { version: string };

export default defineConfig({
  // Relative URLs let the static browser bundle work at a domain root or in
  // any uploaded subdirectory.
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  define: {
    __TAPCONDUCTOR_VERSION__: JSON.stringify(packageVersion.version),
  },
  build: {
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: process.env.TAURI_ENV_DEBUG ? false : "esbuild",
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    // Release packaging handles compression; recompressing every asset merely
    // to print size estimates adds work after an otherwise complete build.
    reportCompressedSize: false,
  },
});

import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const dist = resolve(root, "dist");
const indexPath = resolve(dist, "index.html");
let html = readFileSync(indexPath, "utf8");

const scriptMatch = html.match(
  /<script type="module"[^>]*src="([^"]+)"[^>]*><\/script>/,
);
const styleMatch = html.match(
  /<link rel="stylesheet"[^>]*href="([^"]+)"[^>]*>/,
);
if (!scriptMatch?.[1] || !styleMatch?.[1]) {
  throw new Error("Unable to locate Vite's script and stylesheet in dist/index.html.");
}

function distFile(relativeUrl) {
  return resolve(dist, relativeUrl.replace(/^\.\//, ""));
}

function dataUrl(mimeType, filePath) {
  return `data:${mimeType};base64,${readFileSync(filePath).toString("base64")}`;
}

const applicationScript = readFileSync(distFile(scriptMatch[1]), "utf8")
  .replace(/<\/script/gi, "<\\\\/script");
const stylesheet = readFileSync(distFile(styleMatch[1]), "utf8")
  .replace(/<\/style/gi, "<\\\\/style");
const wasmDirectory = resolve(dist, "wasm");
const wasmBindingsUrl = dataUrl(
  "text/javascript",
  resolve(wasmDirectory, "tapconductor_web.js"),
);
const wasmBinaryUrl = dataUrl(
  "application/wasm",
  resolve(wasmDirectory, "tapconductor_web_bg.wasm"),
);

const pianoDemoUrl = dataUrl(
  "application/vnd.recordare.musicxml+xml",
  resolve(root, "assets", "demo", "Prelude in C Minor - Chopin 1839.musicxml"),
);
const choirDemoUrl = dataUrl(
  "application/vnd.recordare.musicxml",
  resolve(root, "assets", "demo", "All-Night Vigil - Rachmaninoff 1915.musicxml"),
);
const fingerUrl = dataUrl(
  "image/png",
  resolve(root, "assets", "finger transparent-background.png"),
);

const bootstrap = [
  `globalThis.__TAPCONDUCTOR_WASM_JS__=${JSON.stringify(wasmBindingsUrl)};`,
  `globalThis.__TAPCONDUCTOR_WASM_BINARY__=${JSON.stringify(wasmBinaryUrl)};`,
  `globalThis.__TAPCONDUCTOR_PIANO_DEMO_URL__=${JSON.stringify(pianoDemoUrl)};`,
  `globalThis.__TAPCONDUCTOR_CHOIR_DEMO_URL__=${JSON.stringify(choirDemoUrl)};`,
  `globalThis.__TAPCONDUCTOR_FINGER_URL__=${JSON.stringify(fingerUrl)};`,
].join("\n");

html = html
  .replace(styleMatch[0], () => `<style>${stylesheet}</style>`)
  .replace(
    scriptMatch[0],
    () => `<script type="module">${bootstrap}\n${applicationScript}</script>`,
  );

writeFileSync(indexPath, html);
console.log("Created a self-contained dist/index.html that also works from file://.");

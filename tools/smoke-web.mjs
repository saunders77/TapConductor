import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const root = resolve(import.meta.dirname, "..");
const wasmDirectory = resolve(root, "public", "wasm");
const bindings = await import(pathToFileURL(resolve(wasmDirectory, "tapconductor_web.js")));
bindings.initSync({
  module: readFileSync(resolve(wasmDirectory, "tapconductor_web_bg.wasm")),
});

const demoPath = resolve(root, "assets", "demo", "Prelude in C Minor - Chopin 1839.musicxml");
const score = new bindings.WebScore(
  new Uint8Array(readFileSync(demoPath)),
  "Prelude in C Minor - Chopin 1839.musicxml",
);
const dto = JSON.parse(score.dto_json());
if (dto.format !== "music_xml" || dto.events.length < 10 || dto.parts.length === 0) {
  throw new Error("The browser score importer returned an incomplete demo score.");
}
score.free();
console.log(
  `Browser importer smoke test passed: ${dto.events.length} events, ${dto.parts.length} part(s).`,
);

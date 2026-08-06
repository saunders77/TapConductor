// Copyright (c) 2026 Michael Saunders
import { mkdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const wasmOutput = resolve(root, "public", "wasm");
const wasmBinary = resolve(
  root,
  "target",
  "wasm32-unknown-unknown",
  "release",
  "tapconductor_web.wasm",
);

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.error?.code === "ENOENT") {
    throw new Error(
      `'${command}' is required for the web build. See docs/WEB_BROWSER.md for setup.`,
    );
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
}

mkdirSync(wasmOutput, { recursive: true });
run("cargo", [
  "build",
  "-p",
  "tapconductor-web",
  "--target",
  "wasm32-unknown-unknown",
  "--release",
]);
run("wasm-bindgen", [
  wasmBinary,
  "--target",
  "web",
  "--out-dir",
  wasmOutput,
  "--no-typescript",
]);
run(process.platform === "win32" ? "npm.cmd" : "npm", ["run", "build"]);
await import("./make-standalone-web.mjs");

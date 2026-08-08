// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

test("every required UI element is present in the application markup", () => {
  const markupIds = new Set(
    [...source.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]!),
  );
  const requiredIds = [...source.matchAll(/byId(?:<[^;\n]+?>)?\("([^"]+)"\)/g)]
    .map((match) => match[1]!);

  assert.deepEqual(requiredIds.filter((id) => !markupIds.has(id)), []);
});

test("the Info privacy section links to a separate telemetry settings view", () => {
  const privacySection = source.match(/<section id="privacy"[\s\S]*?<\/section>/)?.[0] ?? "";
  const settingsView = source.match(/<div id="telemetry-settings"[\s\S]*?<div id="telemetry-consent"/)?.[0] ?? "";

  assert.match(privacySection, /id="telemetry-settings-link"/);
  assert.doesNotMatch(privacySection, /id="telemetry-toggle"/);
  assert.match(settingsView, /id="telemetry-toggle"/);
  assert.doesNotMatch(settingsView, /telemetry-(?:copy-id|reset)/);
});

test("iPad transient dialogs stay content-sized", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(
    styles,
    /\.platform-ipados \.telemetry-consent,\s*\.platform-ipados #announcement-overlay\s*{[^}]*align-items:\s*center;/,
  );
});

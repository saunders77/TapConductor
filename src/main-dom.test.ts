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
  const settingsView = source.match(/<div id="telemetry-settings"[\s\S]*?<div id="announcement-overlay"/)?.[0] ?? "";

  assert.match(privacySection, /id="telemetry-settings-link"/);
  assert.doesNotMatch(privacySection, /id="telemetry-toggle"/);
  assert.match(settingsView, /id="telemetry-toggle"/);
  assert.doesNotMatch(settingsView, /telemetry-(?:copy-id|reset)/);
});

test("iPad transient dialogs stay content-sized", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(
    styles,
    /\.platform-ipados #announcement-overlay\s*{[^}]*align-items:\s*center;/,
  );
});

test("iPad performance UI suppresses browser gestures but leaves dialogs alone", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.shell > :not\(\[role="dialog"\]\)\s*{[^}]*-webkit-user-select:\s*none;[^}]*touch-action:\s*pan-x pan-y;/s);
  assert.match(source, /appleUiPlatform === "ipados"[\s\S]*?"selectstart"[\s\S]*?"gesturestart"[\s\S]*?event\.touches\.length > 1/);
  assert.match(source, /target\.closest\('\[role="dialog"\]'\)/);
});

test("iPad TAP button is wider and its transport buttons use adjacent columns", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-ipados \.performance-strip\s*{[^}]*grid-template-columns:[^;]*420px/s);
  assert.match(styles, /\.platform-ipados \.tap-button\s*{[^}]*grid-column:\s*3;[^}]*width:\s*min\(420px, 100%\);/s);
  assert.match(styles, /\.platform-ipados #back-button\s*{[^}]*grid-column:\s*2;/s);
  assert.match(styles, /\.platform-ipados #forward-button\s*{[^}]*grid-column:\s*4;/s);
});

test("Apple header controls use platform-specific visual corrections", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.platform-macos \.panic-button:not\(\.midi-free-play\)::before\s*{[^}]*transform:\s*translateY\(6px\);/s);
  assert.match(styles, /\.control-deck > \.select-field\s*{[^}]*padding-inline:\s*2\.5px;/s);
  assert.match(styles, /\.platform-ipados \.control-deck > \.field\s*{[^}]*justify-content:\s*flex-start;/s);
  assert.match(source, /appleUiPlatform === "ipados" \? "─{10}" : "─{12}"/);
});

test("score action rows have a fixed fallback and are positioned before engraving", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /\.slice-controls\s*{[^}]*top:\s*20px;[^}]*row-gap:\s*20px;/s);

  const positionCall = source.indexOf("positionScoreActionRows();");
  const engravingCall = source.indexOf("fitFirstSystemEngravingToActions(activeOsmd);");
  assert.ok(positionCall >= 0);
  assert.ok(engravingCall > positionCall);
});

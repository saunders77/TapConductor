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

test("iPad landscape widens TAP by 60 percent and keeps circular transports symmetric", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /@media \(max-width: 1100px\) and \(orientation: landscape\)[\s\S]*?\.platform-ipados \.performance-strip\s*{[^}]*grid-template-columns:\s*minmax\(0, 1fr\) 44px minmax\(150px, 672px\) 44px minmax\(0, 1fr\);[^}]*column-gap:\s*10px;/s);
  assert.match(styles, /@media \(max-width: 1100px\) and \(orientation: landscape\)[\s\S]*?\.platform-ipados \.tap-button\s*{[^}]*width:\s*min\(672px, 100%\);/s);
  assert.match(styles, /\.transport\s*{[^}]*display:\s*grid;[^}]*place-items:\s*center;/s);
  assert.match(styles, /@media \(max-width: 1100px\)[\s\S]*?\.transport\s*{[^}]*width:\s*44px;[^}]*min-width:\s*44px;[^}]*height:\s*44px;[^}]*min-height:\s*44px;[^}]*aspect-ratio:\s*1;/s);
});

test("compact footer controls leave vertical room for slider labels", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /@media \(max-width: 1100px\)[\s\S]*?\.performance-strip\s*{[^}]*grid-template-rows:\s*minmax\(92px, 1fr\) 46px;[^}]*padding:\s*4px [^;]* max\(4px, env\(safe-area-inset-bottom, 0px\)\)/s);
  assert.match(styles, /\.bottom-controls\s*{[^}]*height:\s*46px;/s);
  assert.match(styles, /\.bottom-controls \.range-field\s*{[^}]*gap:\s*0;[^}]*padding-block:\s*0;/s);
  assert.match(styles, /\.bottom-controls \.zoom-controls\s*{[^}]*row-gap:\s*0;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.range-field\s*{[^}]*gap:\s*4px;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls \.zoom-controls\s*{[^}]*row-gap:\s*4px;/s);
  assert.match(styles, /\.platform-ipados \.bottom-controls input\[type="range"\]\s*{[^}]*height:\s*28px;[^}]*min-height:\s*28px;[^}]*margin:\s*0;/s);
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
  assert.match(styles, /\.slice-controls\s*{[^}]*top:\s*20px;[^}]*row-gap:\s*12px;/s);

  const positionCall = source.indexOf("positionScoreActionRows();");
  const engravingCall = source.indexOf("fitFirstSystemEngravingToActions(activeOsmd);");
  assert.ok(positionCall >= 0);
  assert.ok(engravingCall > positionCall);
});

test("desktop footer is 40 pixels shorter without shrinking its contents", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  assert.match(styles, /\.workspace\s*{[^}]*grid-template-rows:\s*minmax\(0, 1fr\) 110px;/s);
  assert.match(styles, /\.performance-strip\s*{[^}]*padding:\s*5px [^;]* max\(5px, env\(safe-area-inset-bottom, 0px\)\)/s);
  assert.match(styles, /\.tap-button\s*{[^}]*height:\s*100px;/s);
});

test("the rhythm position highlight spans only the full score layer", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

  assert.match(styles, /\.score-scroll\s*{[^}]*overflow-x:\s*auto;[^}]*overflow-y:\s*auto;/s);
  assert.match(styles, /\.score-highlights\s*{[^}]*position:\s*absolute;[^}]*inset:\s*0;/s);
  assert.match(styles, /\.score-position-highlight\s*{[^}]*top:\s*0\s*!important;[^}]*height:\s*100%\s*!important;/s);
  assert.match(styles, /\.slice-controls\s*{[^}]*background:\s*transparent;/s);
  assert.match(styles, /\.slice-action\s*{[^}]*background:\s*transparent;/s);
});

test("normal position changes do not mutate OSMD or animate the engraving stack", () => {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const updateVisualPosition = source.match(
    /function updateVisualPosition\([^)]*\): void \{([\s\S]*?)\n\}/,
  )?.[1] ?? "";

  assert.doesNotMatch(updateVisualPosition, /osmd\.cursor|moveOsmdCursor/);
  assert.match(styles, /\.score-position-highlight\s*{[^}]*visibility:\s*hidden;/s);
  assert.match(styles, /\.score-position-highlight\.current\s*{[^}]*visibility:\s*visible;/s);
  assert.doesNotMatch(styles, /\.score-position-highlight\s*{[^}]*transition:/s);
});

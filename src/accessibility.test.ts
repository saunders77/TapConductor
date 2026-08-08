// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

test("interactive controls have labels and connected disclosure state", () => {
  for (const id of ["regular-roll", "volume", "audition-roll", "zoom-range"]) {
    assert.match(source, new RegExp(`id="${id}"[^>]+aria-label="[^"]+"`));
  }
  for (const [button, controlled] of [
    ["parts-button", "parts-popover"],
    ["diagnostics-button", "diagnostics-popover"],
  ]) {
    assert.match(
      source,
      new RegExp(`id="${button}"[^>]+aria-controls="${controlled}"[^>]+aria-expanded="false"`),
    );
  }
  assert.match(source, /id="status-pill"[^>]+role="status"[^>]+aria-live="polite"/);
  assert.match(source, /id="score-status"[^>]+role="status"[^>]+aria-live="polite"/);
});

test("global performance commands work regardless of focused app control", () => {
  assert.doesNotMatch(source, /isInteractiveShortcutTarget/);
  assert.match(source, /tapKeyCodes\.has\(event\.code\) && !commandModifier/);
  assert.match(source, /heldTokens\.has\(`key:\$\{event\.code\}`\)/);
  assert.match(source, /if \(event\.code === "ArrowLeft"\)/);
  assert.match(source, /if \(event\.code === "ArrowRight"\)/);
  assert.match(source, /event\.code === "Space" && !commandModifier/);
  assert.match(source, /event\.code === "Period" && commandModifier/);
  assert.doesNotMatch(source, /const tapKeyCodes = new Set\(\[\s*"Enter"/);
  assert.match(source, /elements\.legatoMode\.addEventListener\("keydown"/);
  assert.match(source, /document\.addEventListener\("keydown",[\s\S]+?\}, \{ capture: true \}\);/);
  assert.match(source, /dialog !== elements\.helpOverlay/);
  assert.match(source, /event\.code === "Escape" && hasBlockingModal\(\)/);
  assert.doesNotMatch(source, /event\.defaultPrevented \|\| hasBlockingModal\(\)/);
  const pianoShortcutHandler = source.match(/async function handlePianoShortcut[\s\S]*?\n\}/)?.[0] ?? "";
  assert.doesNotMatch(pianoShortcutHandler, /helpOverlay|activeElement|:focus/);
});

test("modal and score navigation manage focus without enormous tab sequences", () => {
  assert.match(source, /function syncModalIsolation/);
  assert.match(source, /child\.inert = activeDialog !== null/);
  assert.match(source, /const SCORE_ACTION_SELECTOR/);
  assert.match(source, /button\.tabIndex = button === active \? 0 : -1/);
  assert.match(source, /\["ArrowUp", "ArrowDown", "Home", "End"\]/);
});

test("cross-platform visual accessibility preferences are honored", () => {
  assert.match(styles, /\*:focus-visible\s*\{/);
  assert.match(styles, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(styles, /@media \(forced-colors: active\)/);
  assert.match(styles, /\.note-target[^}]+min-width: 24px[^}]+min-height: 24px/);
  assert.doesNotMatch(styles, /var\(--green\)/);
});

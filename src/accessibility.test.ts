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

test("global performance commands preserve native control keyboard behavior", () => {
  assert.match(source, /function isInteractiveShortcutTarget/);
  assert.match(source, /if \(isInteractiveShortcutTarget\(event\.target\)\) return;/);
  assert.match(source, /tapKeyCodes\.has\(event\.code\) && !commandModifier/);
  assert.match(source, /heldTokens\.has\(`key:\$\{event\.code\}`\)/);
});

test("modal and score navigation manage focus without enormous tab sequences", () => {
  assert.match(source, /function syncModalIsolation/);
  assert.match(source, /child\.inert = activeDialog !== null/);
  assert.match(source, /const SCORE_ACTION_SELECTOR/);
  assert.match(source, /button\.tabIndex = button === active \? 0 : -1/);
  assert.match(source, /\["ArrowLeft", "ArrowRight", "Home", "End"\]/);
});

test("cross-platform visual accessibility preferences are honored", () => {
  assert.match(styles, /\*:focus-visible\s*\{/);
  assert.match(styles, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(styles, /@media \(forced-colors: active\)/);
  assert.match(styles, /\.note-target[^}]+min-width: 24px[^}]+min-height: 24px/);
  assert.doesNotMatch(styles, /var\(--green\)/);
});

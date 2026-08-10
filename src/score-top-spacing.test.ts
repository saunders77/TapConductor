// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  scoreActionRowGap,
  scoreActionTop,
  scoreContentTopShift,
} from "./score-top-spacing.ts";

test("places audition action tops 20 pixels below the header", () => {
  assert.equal(scoreActionTop(20), 20);
});

test("places start-here actions 12 pixels below the audition button bottoms", () => {
  assert.equal(scoreActionRowGap(24, 24 + 12), 12);
});

test("moves high engraving down until it clears the fixed action rows", () => {
  assert.equal(scoreContentTopShift(42, 80), 38);
});

test("removes excess renderer whitespace without moving the action rows", () => {
  assert.equal(scoreContentTopShift(104, 80), -24);
});

test("ignores unavailable geometry while incremental rendering starts", () => {
  assert.equal(scoreContentTopShift(Number.NaN, 80), 0);
});

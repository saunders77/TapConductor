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

test("places start-here actions 28 pixels below the audition button bottoms", () => {
  assert.equal(scoreActionRowGap(24, 24 + 28), 28);
});

test("moves high engraving down until it clears the fixed action rows", () => {
  assert.equal(scoreContentTopShift(42, 96), 54);
});

test("removes excess renderer whitespace without moving the action rows", () => {
  assert.equal(scoreContentTopShift(120, 96), -24);
});

test("ignores unavailable geometry while incremental rendering starts", () => {
  assert.equal(scoreContentTopShift(Number.NaN, 96), 0);
});

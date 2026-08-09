// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  scoreActionRowGap,
  scoreActionTop,
  scoreContentTopShift,
} from "./score-top-spacing.ts";

test("places audition actions two and a half icon heights below the header", () => {
  assert.equal(scoreActionTop(24, 2.5), 60);
});

test("places start-here actions two icon heights below the audition row", () => {
  assert.equal(scoreActionRowGap(24, 2), 24);
});

test("moves high engraving down until it clears the fixed action rows", () => {
  assert.equal(scoreContentTopShift(42, 132), 90);
});

test("removes excess renderer whitespace without moving the action rows", () => {
  assert.equal(scoreContentTopShift(156, 132), -24);
});

test("ignores unavailable geometry while incremental rendering starts", () => {
  assert.equal(scoreContentTopShift(Number.NaN, 132), 0);
});

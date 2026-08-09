// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { scoreActionTop, scoreContentTopShift } from "./score-top-spacing.ts";

test("places audition actions one and a half icon heights below the header", () => {
  assert.equal(scoreActionTop(24, 1.5), 36);
});

test("moves high engraving down until it clears the fixed action rows", () => {
  assert.equal(scoreContentTopShift(42, 84), 42);
});

test("removes excess renderer whitespace without moving the action rows", () => {
  assert.equal(scoreContentTopShift(126, 84), -42);
});

test("ignores unavailable geometry while incremental rendering starts", () => {
  assert.equal(scoreContentTopShift(Number.NaN, 84), 0);
});

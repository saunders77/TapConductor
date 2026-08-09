// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { scoreContentTopShift, topClearingNotation } from "./score-top-spacing.ts";

test("removes renderer and platform whitespace above score content", () => {
  assert.equal(scoreContentTopShift([126, 88, 42], 8), -34);
});

test("adds headroom when rendered content would begin above the desired padding", () => {
  assert.equal(scoreContentTopShift([14, -3, 22], 8), 11);
});

test("moves action icons above notation in their column with a stable gap", () => {
  assert.equal(
    topClearingNotation({ top: 30, bottom: 76 }, 68, 6),
    16,
  );
  assert.equal(
    topClearingNotation({ top: 18, bottom: 62 }, 80, 6),
    18,
  );
});

test("ignores unavailable geometry while incremental rendering starts", () => {
  assert.equal(scoreContentTopShift([Number.NaN, 40], 8), -32);
});

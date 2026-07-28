import assert from "node:assert/strict";
import test from "node:test";
import { releaseBoundaryIndex } from "./web-note-gate.ts";

const at = (numerator: number, denominator = 1) => ({
  absolute: { numerator, denominator },
});
const events = [at(0), at(1), at(2), at(3)];

test("short notes may damp as soon as the input is released", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 1, denominator: 1 }), null);
});

test("notes spanning later onsets remain held through their last crossed onset", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 5, denominator: 2 }), 2);
});

test("an onset exactly at the written end is not crossed", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 2, denominator: 1 }), 1);
});

test("exact rational comparisons do not round triplets", () => {
  const triplets = [at(0), at(1, 3), at(2, 3), at(1)];
  assert.equal(releaseBoundaryIndex(triplets, 0, { numerator: 2, denominator: 3 }), 1);
});

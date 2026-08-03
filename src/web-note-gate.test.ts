import assert from "node:assert/strict";
import test from "node:test";
import { releaseBoundaryIndex, releasesOnInput } from "./web-note-gate.ts";

const at = (numerator: number, denominator = 1, ends: number[] = [numerator + 1]) => ({
  absolute: { numerator, denominator },
  notes: ends.map((end) => ({ end: { numerator: end, denominator: 1 } })),
});
const events = [at(0), at(1), at(2), at(3)];

test("short notes may damp as soon as the input is released", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 1, denominator: 2 }), null);
});

test("a note releases when its end exactly matches a future onset", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 2, denominator: 1 }), 2);
});

test("a future overlapping note can release a held note", () => {
  const overlap = [at(0, 1, [3]), at(1, 1, [4])];
  assert.equal(releaseBoundaryIndex(overlap, 0, { numerator: 3, denominator: 1 }), 1);
});

test("an intervening note boundary prevents an overlapping release", () => {
  const overlap = [at(0, 1, [4]), at(1, 1, [2, 5])];
  assert.equal(
    releaseBoundaryIndex(overlap, 0, { numerator: 4, denominator: 1 }),
    Number.POSITIVE_INFINITY,
  );
});

test("staccato notes release on key-up", () => {
  assert.equal(releaseBoundaryIndex(events, 0, { numerator: 3, denominator: 1 }, true), null);
});

test("staccato release accepts only the originating key-up", () => {
  const staccato = { boundaryIndex: null, originalInputOnly: true };
  assert.equal(releasesOnInput(staccato, false), false);
  assert.equal(releasesOnInput(staccato, true), true);
  assert.equal(
    releasesOnInput({ boundaryIndex: null, originalInputOnly: false }, false),
    true,
  );
});

test("exact rational comparisons do not round triplets", () => {
  const triplets = [at(0), at(1, 3), at(2, 3), at(1)];
  assert.equal(releaseBoundaryIndex(triplets, 0, { numerator: 2, denominator: 3 }), 2);
});

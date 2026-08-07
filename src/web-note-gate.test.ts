// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  releaseBoundaryIndex,
  releaseBoundaryTimeline,
  releasesOnInput,
} from "./web-note-gate.ts";

const at = (numerator: number, denominator = 1, ends: number[] = [numerator + 1]) => ({
  absolute: { numerator, denominator },
  notes: ends.map((end) => ({ end: { numerator: end, denominator: 1 } })),
});
const events = [at(0), at(1), at(2), at(3)];

function referenceReleaseBoundaryIndex(
  source: typeof events,
  playedIndex: number,
  end: { numerator: number; denominator: number },
): number | null {
  const compare = (
    left: { numerator: number; denominator: number },
    right: { numerator: number; denominator: number },
  ): number => left.numerator * right.denominator - right.numerator * left.denominator;
  const next = source[playedIndex + 1];
  if (!next || compare(next.absolute, end) > 0) return null;
  const exact = source.findIndex((event, index) => index > playedIndex && compare(event.absolute, end) === 0);
  if (exact >= 0) return exact;
  for (let index = playedIndex + 1; index < source.length; index += 1) {
    const candidate = source[index]!;
    if (compare(candidate.absolute, end) >= 0) break;
    if (!candidate.notes.some((note) => compare(note.end, end) > 0)) continue;
    const intervenes = source.some((event) => (
      compare(event.absolute, candidate.absolute) > 0 && compare(event.absolute, end) < 0
    ) || event.notes.some((note) => (
      compare(note.end, candidate.absolute) > 0 && compare(note.end, end) < 0
    )));
    if (!intervenes) return index;
  }
  return Number.POSITIVE_INFINITY;
}

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

test("cached boundary timelines preserve equivalent rational boundaries", () => {
  const overlap = [at(0, 1, [4]), at(1, 1, [2, 5]), at(3, 2, [6])];
  const timeline = releaseBoundaryTimeline(overlap);
  assert.equal(
    releaseBoundaryIndex(overlap, 0, { numerator: 4, denominator: 1 }, false, timeline),
    Number.POSITIVE_INFINITY,
  );
  assert.equal(
    timeline.filter((boundary) => boundary.numerator / boundary.denominator === 2).length,
    1,
  );
});

test("cached boundary lookup matches the reference scan across a dense score", () => {
  const dense = Array.from({ length: 48 }, (_, index) => at(
    index,
    1,
    [index + 1 + (index % 7), index + 2 + (index % 3)],
  ));
  const timeline = releaseBoundaryTimeline(dense);
  dense.forEach((event, playedIndex) => {
    event.notes.forEach((note) => {
      assert.equal(
        releaseBoundaryIndex(dense, playedIndex, note.end, false, timeline),
        referenceReleaseBoundaryIndex(dense, playedIndex, note.end),
      );
    });
  });
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

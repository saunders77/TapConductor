// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  beatIndexAtOrBefore,
  countInBeatCount,
  planBeatInterval,
  type RationalPoint,
} from "./beat-scheduler.ts";

const at = (numerator: number, denominator = 1): { absolute: RationalPoint } => ({
  absolute: { numerator, denominator },
});

test("fires the downbeat now and schedules written subdivisions from the last tap interval", () => {
  const events = [at(1), at(2), at(5, 2), at(11, 4), at(3)];
  const plan = planBeatInterval(
    events,
    1,
    { ...at(2), beatType: 4 },
    { ...at(3), beatType: 4 },
    600,
  );

  assert.deepEqual(plan, {
    events: [
      { eventIndex: 1, delayMs: 0, holdMs: 250 },
      { eventIndex: 2, delayMs: 300, holdMs: 250 },
      { eventIndex: 3, delayMs: 450, holdMs: 250 },
    ],
    nextEventIndex: 4,
  });
});

test("does not consume the chord on the following beat", () => {
  const events = [at(2), at(5, 2), at(3)];
  const plan = planBeatInterval(
    events,
    0,
    { ...at(2), beatType: 4 },
    { ...at(3), beatType: 4 },
    500,
  );

  assert.deepEqual(plan.events.map(({ eventIndex }) => eventIndex), [0, 1]);
  assert.equal(plan.nextEventIndex, 2);
});

test("keeps grace and principal events as separate moments at one written position", () => {
  const events = [at(2), at(2), at(2), at(3)];
  const plan = planBeatInterval(
    events,
    0,
    { ...at(2), beatType: 4 },
    { ...at(3), beatType: 4 },
    500,
  );

  assert.deepEqual(plan.events, [
    { eventIndex: 0, delayMs: 0, holdMs: 250 },
    { eventIndex: 1, delayMs: 0, holdMs: 250 },
    { eventIndex: 2, delayMs: 0, holdMs: 250 },
  ]);
  assert.equal(plan.nextEventIndex, 3);
});

test("uses the written beat length for subdivisions after the final beat marker", () => {
  const events = [at(7), at(15, 2), at(8)];
  const plan = planBeatInterval(
    events,
    0,
    { ...at(7), beatType: 4 },
    undefined,
    400,
  );

  assert.deepEqual(plan.events, [
    { eventIndex: 0, delayMs: 0, holdMs: 250 },
    { eventIndex: 1, delayMs: 200, holdMs: 250 },
  ]);
  assert.equal(plan.nextEventIndex, 2);
});

test("shortens a non-legato chord hold when its earliest note ends first", () => {
  const plan = planBeatInterval(
    [
      { ...at(2), notes: [{ end: at(9, 4).absolute }, { end: at(3).absolute }] },
      { ...at(3), notes: [{ end: at(4).absolute }] },
    ],
    0,
    { ...at(2), beatType: 4 },
    { ...at(3), beatType: 4 },
    600,
  );

  assert.equal(plan.events[0]!.holdMs, 150);
});

test("counts in a full bar plus the elapsed half-bar before a pickup", () => {
  const pickupBeat = { ...at(0), beatType: 4, beatIndex: 2, beatsInMeasure: 4 };
  assert.equal(countInBeatCount(pickupBeat), 6);
});

test("locates the correct beat when restarting at an on-beat or offbeat chord", () => {
  const beats = [
    { ...at(0), beatType: 4 },
    { ...at(1), beatType: 4 },
    { ...at(2), beatType: 4 },
    { ...at(3), beatType: 4 },
  ];
  assert.equal(beatIndexAtOrBefore(beats, at(2).absolute), 2);
  assert.equal(beatIndexAtOrBefore(beats, at(5, 2).absolute), 2);
});

import assert from "node:assert/strict";
import test from "node:test";
import { planBeatInterval, type RationalPoint } from "./beat-scheduler.ts";

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
      { eventIndex: 1, delayMs: 0 },
      { eventIndex: 2, delayMs: 300 },
      { eventIndex: 3, delayMs: 450 },
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
    { eventIndex: 0, delayMs: 0 },
    { eventIndex: 1, delayMs: 0 },
    { eventIndex: 2, delayMs: 0 },
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
    { eventIndex: 0, delayMs: 0 },
    { eventIndex: 1, delayMs: 200 },
  ]);
  assert.equal(plan.nextEventIndex, 2);
});

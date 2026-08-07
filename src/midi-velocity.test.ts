// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { volumeToMidiVelocity } from "./midi-velocity.ts";

test("volume maps linearly onto the full MIDI velocity range", () => {
  assert.equal(volumeToMidiVelocity(0), 0);
  assert.equal(volumeToMidiVelocity(0.5), 64);
  assert.equal(volumeToMidiVelocity(1), 127);
});

test("MIDI output velocity clamps invalid volume bounds", () => {
  assert.equal(volumeToMidiVelocity(-1), 0);
  assert.equal(volumeToMidiVelocity(2), 127);
  assert.equal(volumeToMidiVelocity(Number.NaN), 127);
});

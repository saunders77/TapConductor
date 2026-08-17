// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { midiInstrumentSelectionMessage } from "./midi-instrument-selection.ts";

test("passes bank select and program change while preserving their channels", () => {
  assert.deepEqual(
    midiInstrumentSelectionMessage(new Uint8Array([0xb3, 0, 2])),
    new Uint8Array([0xb3, 0, 2]),
  );
  assert.deepEqual(
    midiInstrumentSelectionMessage(new Uint8Array([0xbf, 32, 0])),
    new Uint8Array([0xbf, 32, 0]),
  );
  assert.deepEqual(
    midiInstrumentSelectionMessage(new Uint8Array([0xc8, 40])),
    new Uint8Array([0xc8, 40]),
  );
});

test("does not pass unrelated or malformed MIDI input", () => {
  assert.equal(midiInstrumentSelectionMessage(new Uint8Array([0xb0, 64, 127])), null);
  assert.equal(midiInstrumentSelectionMessage(new Uint8Array([0x90, 60, 100])), null);
  assert.equal(midiInstrumentSelectionMessage(new Uint8Array([0xb0, 0])), null);
  assert.equal(midiInstrumentSelectionMessage(new Uint8Array([0xc0])), null);
});

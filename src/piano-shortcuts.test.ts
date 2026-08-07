// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import { PianoShortcutGate } from "./piano-shortcuts.ts";

test("passes ordinary notes and consumes the function pitch", () => {
  const gate = new PianoShortcutGate(36);
  assert.deepEqual(gate.process({ type: "down", token: "ordinary", pitch: 60 }), { type: "pass" });
  assert.deepEqual(gate.process({ type: "down", token: "function", pitch: 36 }), { type: "consume" });
  assert.deepEqual(gate.process({ type: "up", token: "function" }), { type: "consume" });
});

test("maps command pitch classes in any octave while the function key is held", () => {
  const gate = new PianoShortcutGate(36);
  gate.process({ type: "down", token: "function", pitch: 36 });
  for (const [pitch, command] of [[64, "forward"], [50, "back"], [75, "replay"], [37, "beginning"], [95, "toggle_free_play"]] as const) {
    assert.deepEqual(gate.process({ type: "down", token: command, pitch }), {
      type: "consume",
      event: { command, token: command, pressed: true },
    });
    assert.deepEqual(gate.process({ type: "up", token: command }), {
      type: "consume",
      event: { command, token: command, pressed: false },
    });
  }
});

test("consumes unmapped notes in a shortcut combination", () => {
  const gate = new PianoShortcutGate(36);
  gate.process({ type: "down", token: "function", pitch: 36 });
  assert.deepEqual(gate.process({ type: "down", token: "unmapped", pitch: 60 }), { type: "consume" });
  assert.deepEqual(gate.process({ type: "up", token: "unmapped" }), { type: "consume" });
});

// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";

import { midiSelectionAfterRefresh } from "./midi-device-selection.ts";

const keyboard = { id: "keyboard", name: "Keyboard" };

test("disconnecting an active MIDI port turns it Off but keeps restoration pending", () => {
  assert.deepEqual(
    midiSelectionAfterRefresh("keyboard", "keyboard", [], "keyboard"),
    { selectedId: "", disconnected: true, restorePending: true },
  );
});

test("a persisted port absent at launch can still be restored later", () => {
  assert.deepEqual(
    midiSelectionAfterRefresh("", undefined, [], "keyboard"),
    { selectedId: "", disconnected: false, restorePending: true },
  );
  assert.deepEqual(
    midiSelectionAfterRefresh("", "keyboard", [keyboard], "keyboard"),
    { selectedId: "keyboard", disconnected: false, restorePending: false },
  );
});

test("a discovery failure is not mistaken for a physical disconnect", () => {
  assert.deepEqual(
    midiSelectionAfterRefresh("keyboard", "keyboard", [], "keyboard", false),
    { selectedId: "", disconnected: false, restorePending: true },
  );
});

test("manual Off has no desired port to restore", () => {
  assert.deepEqual(
    midiSelectionAfterRefresh("", undefined, [keyboard], undefined),
    { selectedId: "", disconnected: false, restorePending: false },
  );
});

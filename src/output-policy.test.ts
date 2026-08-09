import assert from "node:assert/strict";
import test from "node:test";
import { shouldAutoMuteAudio } from "./output-policy.ts";

test("audio initially mutes when MIDI output changes from Off to a device", () => {
  assert.equal(shouldAutoMuteAudio("", "midi-device"), true);
});

test("audio remains user-controlled while MIDI output is already enabled", () => {
  assert.equal(shouldAutoMuteAudio("midi-device", "other-device"), false);
  assert.equal(shouldAutoMuteAudio("midi-device", ""), false);
  assert.equal(shouldAutoMuteAudio("", ""), false);
});

// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";

import {
  APP_SETTINGS_KEY,
  DEFAULT_APP_SETTINGS,
  MIN_LAUNCH_VOLUME_PERCENT,
  loadAppSettings,
  resolveDevicePreference,
  saveAppSettings,
} from "./app-settings.ts";

function memoryStorage(initial?: string) {
  let value = initial ?? null;
  return {
    getItem: (key: string) => key === APP_SETTINGS_KEY ? value : null,
    setItem: (key: string, next: string) => {
      if (key === APP_SETTINGS_KEY) value = next;
    },
    serialized: () => value,
  };
}

test("round trips all application settings", () => {
  const storage = memoryStorage();
  const settings = {
    ...DEFAULT_APP_SETTINGS,
    audioOutput: { id: "audio-2", name: "Speakers" },
    midiInput: { id: "midi-in", name: "Keyboard" },
    midiOutput: { id: "midi-out", name: "Piano" },
    instrument: "synth" as const,
    tapMode: "beat" as const,
    legato: true,
    volumePercent: 63,
    tapRollMs: 21,
    chordRollMs: 87,
    zoomPercent: 125,
    chromeHidden: true,
    midiFreePlay: true,
    pianoShortcutPitch: 48,
  };
  assert.equal(saveAppSettings(storage, settings), true);
  assert.deepEqual(loadAppSettings(storage), settings);
});

test("invalid or out-of-range persisted values fall back or clamp safely", () => {
  const storage = memoryStorage(JSON.stringify({
    version: 1,
    instrument: "organ",
    tapMode: "other",
    legato: "yes",
    volumePercent: 900,
    tapRollMs: -2,
    chordRollMs: null,
    zoomPercent: 12,
    chromeHidden: 1,
    midiFreePlay: "yes",
    pianoShortcutPitch: 200,
    language: "klingon",
  }));
  assert.deepEqual(loadAppSettings(storage), {
    ...DEFAULT_APP_SETTINGS,
    volumePercent: 100,
    tapRollMs: 0,
    zoomPercent: 50,
    pianoShortcutPitch: 127,
  });
});

test("launch volume defaults to full and never restores below ten percent", () => {
  assert.equal(loadAppSettings(memoryStorage()).volumePercent, 100);
  for (const persistedVolume of [0, 1, 9]) {
    const storage = memoryStorage(JSON.stringify({
      ...DEFAULT_APP_SETTINGS,
      volumePercent: persistedVolume,
    }));
    assert.equal(loadAppSettings(storage).volumePercent, MIN_LAUNCH_VOLUME_PERCENT);
  }
  const storage = memoryStorage(JSON.stringify({
    ...DEFAULT_APP_SETTINGS,
    volumePercent: 42,
  }));
  assert.equal(loadAppSettings(storage).volumePercent, 42);
});

test("unavailable storage does not prevent defaults or active setting changes", () => {
  const storage = {
    getItem: () => { throw new Error("blocked"); },
    setItem: () => { throw new Error("blocked"); },
  };
  assert.deepEqual(loadAppSettings(storage), DEFAULT_APP_SETTINGS);
  assert.equal(saveAppSettings(storage, DEFAULT_APP_SETTINGS), false);
});

test("device restoration survives a changed platform ID only for a unique matching name", () => {
  const preference = { id: "old-id", name: "Studio Piano" };
  assert.equal(resolveDevicePreference(preference, [
    { id: "new-id", name: "Studio Piano" },
  ])?.id, "new-id");
  assert.equal(resolveDevicePreference(preference, [
    { id: "new-id-1", name: "Studio Piano" },
    { id: "new-id-2", name: "Studio Piano" },
  ]), undefined);
  assert.equal(resolveDevicePreference({ id: "exact", name: "Old name" }, [
    { id: "exact", name: "New name" },
  ])?.id, "exact");
});

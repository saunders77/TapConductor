// Copyright (c) 2026 Michael Saunders
import type { DeviceDto } from "./types";

export const APP_SETTINGS_KEY = "tapconductor.app-settings-v1";

export type DevicePreference = {
  id: string;
  name?: string;
};

export type AppSettings = {
  version: 1;
  audioOutput?: DevicePreference;
  midiInput?: DevicePreference;
  midiOutput?: DevicePreference;
  instrument: "piano" | "synth";
  tapMode: "rhythm" | "beat";
  legato: boolean;
  volumePercent: number;
  tapRollMs: number;
  chordRollMs: number;
  zoomPercent: number;
  chromeHidden: boolean;
  midiFreePlay: boolean;
  pianoShortcutPitch: number;
};

export const DEFAULT_APP_SETTINGS: AppSettings = {
  version: 1,
  instrument: "piano",
  tapMode: "rhythm",
  legato: false,
  volumePercent: 100,
  tapRollMs: 0,
  chordRollMs: 120,
  zoomPercent: 90,
  chromeHidden: false,
  midiFreePlay: false,
  pianoShortcutPitch: 36,
};

type SettingsStorage = Pick<Storage, "getItem" | "setItem">;

function boundedInteger(value: unknown, minimum: number, maximum: number, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.max(minimum, Math.min(maximum, Math.round(value)))
    : fallback;
}

function devicePreference(value: unknown): DevicePreference | undefined {
  if (!value || typeof value !== "object") return undefined;
  const candidate = value as { id?: unknown; name?: unknown };
  if (typeof candidate.id !== "string") return undefined;
  return {
    id: candidate.id,
    ...(typeof candidate.name === "string" && candidate.name.length > 0
      ? { name: candidate.name }
      : {}),
  };
}

export function loadAppSettings(storage: SettingsStorage): AppSettings {
  try {
    const serialized = storage.getItem(APP_SETTINGS_KEY);
    if (!serialized) return { ...DEFAULT_APP_SETTINGS };
    const value = JSON.parse(serialized) as Record<string, unknown>;
    if (!value || value.version !== 1) return { ...DEFAULT_APP_SETTINGS };
    const audioOutput = devicePreference(value.audioOutput);
    const midiInput = devicePreference(value.midiInput);
    const midiOutput = devicePreference(value.midiOutput);
    return {
      version: 1,
      ...(audioOutput ? { audioOutput } : {}),
      ...(midiInput ? { midiInput } : {}),
      ...(midiOutput ? { midiOutput } : {}),
      instrument: value.instrument === "synth" ? "synth" : "piano",
      tapMode: value.tapMode === "beat" ? "beat" : "rhythm",
      legato: typeof value.legato === "boolean" ? value.legato : DEFAULT_APP_SETTINGS.legato,
      volumePercent: boundedInteger(value.volumePercent, 0, 100, DEFAULT_APP_SETTINGS.volumePercent),
      tapRollMs: boundedInteger(value.tapRollMs, 0, 250, DEFAULT_APP_SETTINGS.tapRollMs),
      chordRollMs: boundedInteger(value.chordRollMs, 0, 250, DEFAULT_APP_SETTINGS.chordRollMs),
      zoomPercent: boundedInteger(value.zoomPercent, 50, 175, DEFAULT_APP_SETTINGS.zoomPercent),
      chromeHidden: typeof value.chromeHidden === "boolean"
        ? value.chromeHidden
        : DEFAULT_APP_SETTINGS.chromeHidden,
      midiFreePlay: typeof value.midiFreePlay === "boolean"
        ? value.midiFreePlay
        : DEFAULT_APP_SETTINGS.midiFreePlay,
      pianoShortcutPitch: boundedInteger(
        value.pianoShortcutPitch,
        0,
        127,
        DEFAULT_APP_SETTINGS.pianoShortcutPitch,
      ),
    };
  } catch {
    return { ...DEFAULT_APP_SETTINGS };
  }
}

export function saveAppSettings(storage: SettingsStorage, settings: AppSettings): boolean {
  try {
    storage.setItem(APP_SETTINGS_KEY, JSON.stringify(settings));
    return true;
  } catch {
    return false;
  }
}

/** Resolve an exact device first, then tolerate platform IDs changing by using a unique name. */
export function resolveDevicePreference(
  preference: DevicePreference | undefined,
  devices: readonly DeviceDto[],
): DeviceDto | undefined {
  if (!preference || preference.id.length === 0) return undefined;
  const exact = devices.find((device) => device.id === preference.id);
  if (exact) return exact;
  if (!preference.name) return undefined;
  const nameMatches = devices.filter((device) => device.name === preference.name);
  return nameMatches.length === 1 ? nameMatches[0] : undefined;
}

// Copyright (c) 2026 Michael Saunders
import type { DeviceDto } from "./types";

export type MidiSelectionAfterRefresh = {
  selectedId: string;
  disconnected: boolean;
  restorePending: boolean;
};

/**
 * Reconcile the runtime's selected port with a fresh device snapshot.
 *
 * A preference that was unavailable when the app launched remains eligible
 * for restoration. Once an active port disappears, however, Off is the new
 * durable state: reconnecting hardware must not silently reactivate MIDI.
 */
export function midiSelectionAfterRefresh(
  previousSelectedId: string,
  runtimeSelectedId: string | undefined,
  devices: readonly DeviceDto[],
  desiredId: string | undefined,
  discoverySucceeded = true,
): MidiSelectionAfterRefresh {
  const selectedId = devices.some((device) => device.id === runtimeSelectedId)
    ? runtimeSelectedId ?? ""
    : "";
  const disconnected = discoverySucceeded
    && previousSelectedId.length > 0
    && selectedId.length === 0;
  return {
    selectedId,
    disconnected,
    restorePending: !disconnected
      && Boolean(desiredId && desiredId !== selectedId),
  };
}

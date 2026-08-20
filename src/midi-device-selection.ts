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
 * The live selection becomes Off whenever its runtime port is unavailable,
 * while the separately persisted desired ID remains eligible for restoration.
 * A manual Off supplies no desired ID and therefore cannot auto-reactivate.
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
    restorePending: Boolean(desiredId && desiredId !== selectedId),
  };
}

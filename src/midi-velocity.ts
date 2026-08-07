// Copyright (c) 2026 Michael Saunders

export function volumeToMidiVelocity(volume: number): number {
  if (!Number.isFinite(volume)) return 127;
  return Math.round(Math.max(0, Math.min(1, volume)) * 127);
}

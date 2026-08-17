// Copyright (c) 2026 Michael Saunders

/**
 * Returns a complete Bank Select or Program Change message for MIDI OUT.
 * Other MIDI input messages remain owned by TapConductor's input mapping.
 */
export function midiInstrumentSelectionMessage(data: Uint8Array): Uint8Array | null {
  const status = data[0];
  if (status === undefined || status < 0x80 || status >= 0xf0) return null;

  const kind = status & 0xf0;
  if (kind === 0xb0) {
    const controller = data[1];
    const value = data[2];
    if ((controller !== 0 && controller !== 32) || value === undefined || value >= 0x80) {
      return null;
    }
    return new Uint8Array([status, controller, value]);
  }
  if (kind === 0xc0) {
    const program = data[1];
    if (program === undefined || program >= 0x80) return null;
    return new Uint8Array([status, program]);
  }
  return null;
}

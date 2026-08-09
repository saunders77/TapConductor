export function shouldAutoMuteAudio(previousMidiOutput: string, nextMidiOutput: string): boolean {
  return previousMidiOutput.length === 0 && nextMidiOutput.length > 0;
}

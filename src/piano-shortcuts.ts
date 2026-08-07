// Copyright (c) 2026 Michael Saunders
export type PianoShortcutCommand = "forward" | "back" | "replay" | "beginning" | "toggle_free_play";

export type PianoShortcutEvent = {
  command: PianoShortcutCommand;
  token: string;
  pressed: boolean;
};

export type PianoShortcutInput =
  | { type: "down"; token: string; pitch: number }
  | { type: "up"; token: string };

export type PianoShortcutResult =
  | { type: "pass" }
  | { type: "consume"; event?: PianoShortcutEvent };

const commandForPitch = (pitch: number): PianoShortcutCommand | undefined => {
  switch (pitch % 12) {
    case 4: return "forward";
    case 2: return "back";
    case 3: return "replay";
    case 1: return "beginning";
    case 11: return "toggle_free_play";
    default: return undefined;
  }
};

export class PianoShortcutGate {
  private functionPitch: number;
  private readonly functionTokens = new Set<string>();
  private readonly consumedTokens = new Set<string>();
  private readonly commandTokens = new Map<string, PianoShortcutCommand>();

  constructor(functionPitch = 36) {
    this.functionPitch = functionPitch;
  }

  setFunctionPitch(pitch: number): void {
    this.functionPitch = Math.max(0, Math.min(127, Math.round(pitch)));
  }

  reset(): void {
    this.functionTokens.clear();
    this.consumedTokens.clear();
    this.commandTokens.clear();
  }

  process(input: PianoShortcutInput): PianoShortcutResult {
    if (input.type === "up") {
      if (this.functionTokens.delete(input.token)) return { type: "consume" };
      if (!this.consumedTokens.delete(input.token)) return { type: "pass" };
      const command = this.commandTokens.get(input.token);
      if (!command) return { type: "consume" };
      this.commandTokens.delete(input.token);
      return {
        type: "consume",
        event: { command, token: input.token, pressed: false },
      };
    }

    if (input.pitch === this.functionPitch) {
      this.functionTokens.add(input.token);
      return { type: "consume" };
    }
    if (this.functionTokens.size === 0) return { type: "pass" };

    this.consumedTokens.add(input.token);
    const command = commandForPitch(input.pitch);
    if (!command) return { type: "consume" };
    this.commandTokens.set(input.token, command);
    return {
      type: "consume",
      event: { command, token: input.token, pressed: true },
    };
  }
}

// Copyright (c) 2026 Michael Saunders
import type { ImportWarningCode, ImportWarningDto } from "./types";

export const SCORE_WARNING_GROUP_THRESHOLD = 3;

const GROUP_DESCRIPTIONS: Record<ImportWarningCode, string> = {
  graceNoteSkipped: "grace notes were skipped",
  cueNoteSkipped: "cue notes were skipped",
  hiddenNoteSkipped: "hidden or muted notes were skipped",
  unpitchedNoteSkipped: "unpitched notes were skipped",
  microtonalPitchSkipped: "microtonal notes could not be played",
  pitchOutOfRange: "notes fell outside the MIDI pitch range",
  missingPitch: "notes were missing valid pitches",
  invalidDuration: "notes had invalid durations",
  overfullMeasure: "note durations exceed bar lengths",
  inconsistentMeasureDuration: "aligned parts disagree on bar lengths",
  unmatchedTieStop: "tie endings had no matching starts",
  unterminatedTie: "tie starts had no matching endings",
  replacedOpenTie: "new tie starts replaced unfinished ties",
  unsupportedElement: "unsupported score elements were skipped",
  emptyPart: "score parts contained no measures",
  midiNoteWithoutOff: "MIDI notes had no matching Note Off",
  midiNoteOffWithoutOn: "MIDI Note Off events had no matching notes",
  midiMetaIgnored: "MIDI metadata events were ignored",
};

export interface ScoreWarningGroup {
  code: ImportWarningCode;
  description: string;
  warnings: ImportWarningDto[];
}

export type ScoreWarningDisplayItem =
  | { kind: "single"; warning: ImportWarningDto }
  | { kind: "group"; group: ScoreWarningGroup };

export function groupScoreWarnings(
  warnings: ImportWarningDto[],
  threshold = SCORE_WARNING_GROUP_THRESHOLD,
): ScoreWarningDisplayItem[] {
  const byCode = new Map<ImportWarningCode, ImportWarningDto[]>();
  for (const warning of warnings) {
    const group = byCode.get(warning.code);
    if (group) group.push(warning);
    else byCode.set(warning.code, [warning]);
  }

  const emittedGroups = new Set<ImportWarningCode>();
  const items: ScoreWarningDisplayItem[] = [];
  for (const warning of warnings) {
    const matching = byCode.get(warning.code) ?? [warning];
    if (matching.length < threshold) {
      items.push({ kind: "single", warning });
    } else if (!emittedGroups.has(warning.code)) {
      emittedGroups.add(warning.code);
      items.push({
        kind: "group",
        group: {
          code: warning.code,
          description: GROUP_DESCRIPTIONS[warning.code],
          warnings: matching,
        },
      });
    }
  }
  return items;
}

export function warningContext(warning: ImportWarningDto): string | null {
  const pieces: string[] = [];
  if (warning.context.partId) pieces.push(`Part ${warning.context.partId}`);
  if (warning.context.measureId) pieces.push(`measure ${warning.context.measureId}`);
  else if (warning.context.measureIndex !== undefined) {
    pieces.push(`measure ${warning.context.measureIndex + 1}`);
  }
  return pieces.length > 0 ? pieces.join(", ") : null;
}

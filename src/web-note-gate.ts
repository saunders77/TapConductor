// Copyright (c) 2026 Michael Saunders
import type { RationalDto, TapEventDto } from "./types";

export type ScoreReleasePlan = {
  boundaryIndex: number | null;
  originalInputOnly: boolean;
};

export function releasesOnInput(plan: ScoreReleasePlan, isOriginalInput: boolean): boolean {
  return plan.boundaryIndex === null && (!plan.originalInputOnly || isOriginalInput);
}

/** Mirrors the native legato boundary calculation. Null means that any
 * physical release may end the note; Infinity means that no qualifying
 * score gesture occurs before the end of this performance sequence. */
export function releaseBoundaryIndex(
  events: readonly Pick<TapEventDto, "absolute" | "notes">[],
  playedIndex: number,
  end: RationalDto,
  staccato = false,
  boundaryTimeline?: readonly RationalDto[],
): number | null {
  if (staccato) return null;
  const next = events[playedIndex + 1];
  if (!next || compareRational(next.absolute, end) > 0) return null;

  let endIndex = playedIndex + 1;
  while (endIndex < events.length && compareRational(events[endIndex]!.absolute, end) < 0) {
    endIndex += 1;
  }
  if (endIndex < events.length && compareRational(events[endIndex]!.absolute, end) === 0) {
    return endIndex;
  }

  const boundaries = boundaryTimeline ?? releaseBoundaryTimeline(events);
  const precedingBoundaryIndex = lowerBound(boundaries, end) - 1;
  const lastInterveningBoundary = boundaries[precedingBoundaryIndex];

  for (let index = playedIndex + 1; index < endIndex; index += 1) {
    const candidate = events[index]!;
    const startToEnd = compareRational(candidate.absolute, end);
    if (startToEnd >= 0) break;
    if (!candidate.notes.some((note) => compareRational(note.end, end) > 0)) continue;

    const anotherBoundaryIntervenes = lastInterveningBoundary !== undefined
      && compareRational(lastInterveningBoundary, candidate.absolute) > 0;
    if (!anotherBoundaryIntervenes) return index;
  }
  return Number.POSITIVE_INFINITY;
}

/** Sorted onset/note-end boundaries reused by every tap in a loaded score. */
export function releaseBoundaryTimeline(
  events: readonly Pick<TapEventDto, "absolute" | "notes">[],
): RationalDto[] {
  const boundaries = events.flatMap((event) => [event.absolute, ...event.notes.map((note) => note.end)]);
  boundaries.sort(compareRational);
  return boundaries.filter((boundary, index) =>
    index === 0 || compareRational(boundary, boundaries[index - 1]!) !== 0
  );
}

function lowerBound(values: readonly RationalDto[], target: RationalDto): number {
  let low = 0;
  let high = values.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (compareRational(values[middle]!, target) < 0) low = middle + 1;
    else high = middle;
  }
  return low;
}

function compareRational(left: RationalDto, right: RationalDto): number {
  return left.numerator * right.denominator - right.numerator * left.denominator;
}

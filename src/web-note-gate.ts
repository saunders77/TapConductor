import type { RationalDto, TapEventDto } from "./types";

/** Mirrors the native legato boundary calculation. Null means that any
 * physical release may end the note; Infinity means that no qualifying
 * score gesture occurs before the end of this performance sequence. */
export function releaseBoundaryIndex(
  events: readonly Pick<TapEventDto, "absolute" | "notes">[],
  playedIndex: number,
  end: RationalDto,
  staccato = false,
): number | null {
  if (staccato) return null;
  const next = events[playedIndex + 1];
  if (!next || compareRational(next.absolute, end) > 0) return null;

  const exactIndex = events.findIndex((event, index) => (
    index > playedIndex && compareRational(event.absolute, end) === 0
  ));
  if (exactIndex >= 0) return exactIndex;

  for (let index = playedIndex + 1; index < events.length; index += 1) {
    const candidate = events[index]!;
    const startToEnd = compareRational(candidate.absolute, end);
    if (startToEnd >= 0) break;
    if (!candidate.notes.some((note) => compareRational(note.end, end) > 0)) continue;

    const anotherBoundaryIntervenes = events.some((event) => (
      compareRational(event.absolute, candidate.absolute) > 0
        && compareRational(event.absolute, end) < 0
    ) || event.notes.some((note) => (
      compareRational(note.end, candidate.absolute) > 0
        && compareRational(note.end, end) < 0
    )));
    if (!anotherBoundaryIntervenes) return index;
  }
  return Number.POSITIVE_INFINITY;
}

function compareRational(left: RationalDto, right: RationalDto): number {
  return left.numerator * right.denominator - right.numerator * left.denominator;
}

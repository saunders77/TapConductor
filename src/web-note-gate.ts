import type { RationalDto, TapEventDto } from "./types";

/**
 * Mirrors the native rhythm_release_boundary rule: return the last later
 * playable event strictly before a note's resolved written/tied end.
 * A null boundary means finger-up may begin damping immediately.
 */
export function releaseBoundaryIndex(
  events: readonly Pick<TapEventDto, "absolute">[],
  playedIndex: number,
  end: RationalDto,
): number | null {
  let boundary: number | null = null;
  for (let index = playedIndex + 1; index < events.length; index += 1) {
    if (compareRational(events[index]!.absolute, end) >= 0) break;
    boundary = index;
  }
  return boundary;
}

function compareRational(left: RationalDto, right: RationalDto): number {
  return left.numerator * right.denominator - right.numerator * left.denominator;
}

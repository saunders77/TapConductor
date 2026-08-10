// Copyright (c) 2026 Michael Saunders
export interface RationalPoint {
  numerator: number;
  denominator: number;
}

export interface TimelineEvent {
  absolute: RationalPoint;
  notes?: readonly { end: RationalPoint }[];
}

export interface TimelineBeat {
  absolute: RationalPoint;
  beatType: number;
}

export interface ConductedTimelineBeat extends TimelineBeat {
  beatIndex: number;
  beatsInMeasure: number;
}

export interface PlannedBeatEvent {
  eventIndex: number;
  delayMs: number;
  holdMs: number;
}

export interface BeatIntervalPlan {
  events: PlannedBeatEvent[];
  nextEventIndex: number;
}

const POSITION_EPSILON = 1e-9;
export const MAX_NON_LEGATO_BEAT_HOLD_MS = 250;

export const rationalValue = (value: RationalPoint): number =>
  value.denominator === 0 ? 0 : value.numerator / value.denominator;

/** Returns the beat containing a score position on an ordered beat grid. */
export function beatIndexAtOrBefore(
  beats: readonly TimelineBeat[],
  position: RationalPoint,
): number {
  if (beats.length === 0) return 0;
  const target = rationalValue(position);
  let low = 0;
  let high = beats.length - 1;
  let result = 0;
  while (low <= high) {
    const middle = low + Math.floor((high - low) / 2);
    if (rationalValue(beats[middle]!.absolute) <= target + POSITION_EPSILON) {
      result = middle;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return result;
}

/** One complete bar, followed by the elapsed beats before the selected starting beat. */
export function countInBeatCount(beat: ConductedTimelineBeat): number {
  return Math.max(2, beat.beatsInMeasure + beat.beatIndex);
}

/**
 * Reserves every score event in [currentBeat, nextBeat) and assigns its delay
 * from the current tap. The caller supplies the duration measured between the
 * two most recent taps, so written subdivisions follow the performer's tempo.
 */
export function planBeatInterval(
  events: readonly TimelineEvent[],
  fromEventIndex: number,
  currentBeat: TimelineBeat,
  nextBeat: TimelineBeat | undefined,
  measuredBeatMs: number,
): BeatIntervalPlan {
  const currentPosition = rationalValue(currentBeat.absolute);
  const writtenBeatLength = currentBeat.beatType > 0 ? 4 / currentBeat.beatType : 1;
  const nextPosition = nextBeat
    ? rationalValue(nextBeat.absolute)
    : currentPosition + writtenBeatLength;
  const writtenInterval = Math.max(POSITION_EPSILON, nextPosition - currentPosition);
  const eventsInBeat: PlannedBeatEvent[] = [];
  let nextEventIndex = fromEventIndex;

  while (nextEventIndex < events.length) {
    const eventPosition = rationalValue(events[nextEventIndex]!.absolute);
    if (eventPosition >= nextPosition - POSITION_EPSILON) break;

    const fraction = eventPosition <= currentPosition + POSITION_EPSILON
      ? 0
      : (eventPosition - currentPosition) / writtenInterval;
    const earliestEnd = events[nextEventIndex]!.notes?.reduce(
      (earliest, note) => Math.min(earliest, rationalValue(note.end)),
      Number.POSITIVE_INFINITY,
    );
    const writtenDuration = earliestEnd === undefined || !Number.isFinite(earliestEnd)
      ? Number.POSITIVE_INFINITY
      : Math.max(0, earliestEnd - eventPosition);
    eventsInBeat.push({
      eventIndex: nextEventIndex,
      delayMs: Math.max(0, measuredBeatMs * fraction),
      holdMs: Math.min(
        MAX_NON_LEGATO_BEAT_HOLD_MS,
        measuredBeatMs * writtenDuration / writtenInterval,
      ),
    });
    nextEventIndex += 1;
  }

  return { events: eventsInBeat, nextEventIndex };
}

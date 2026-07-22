export interface RationalPoint {
  numerator: number;
  denominator: number;
}

export interface TimelineEvent {
  absolute: RationalPoint;
}

export interface TimelineBeat {
  absolute: RationalPoint;
  beatType: number;
}

export interface PlannedBeatEvent {
  eventIndex: number;
  delayMs: number;
}

export interface BeatIntervalPlan {
  events: PlannedBeatEvent[];
  nextEventIndex: number;
}

const POSITION_EPSILON = 1e-9;

export const rationalValue = (value: RationalPoint): number =>
  value.denominator === 0 ? 0 : value.numerator / value.denominator;

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
    eventsInBeat.push({
      eventIndex: nextEventIndex,
      delayMs: Math.max(0, measuredBeatMs * fraction),
    });
    nextEventIndex += 1;
  }

  return { events: eventsInBeat, nextEventIndex };
}

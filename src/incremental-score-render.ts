// Copyright (c) 2026 Michael Saunders
export type MeasureEvent = {
  measureIndex: number;
};

/**
 * Return the source-measure frontier that should already be engraved while
 * the event at playbackIndex is active. Looking at the next semantic event is
 * important because repeats, endings, and coda instructions make playback
 * order differ from source-measure order.
 */
export function semanticRenderTarget(
  events: readonly MeasureEvent[],
  playbackIndex: number,
  localAheadMeasures: number,
): number {
  if (events.length === 0) return -1;
  const index = Math.max(0, Math.min(events.length - 1, playbackIndex));
  const currentMeasure = events[index]?.measureIndex ?? 0;
  const nextMeasure = events[index + 1]?.measureIndex ?? currentMeasure;
  return Math.max(currentMeasure, nextMeasure) + Math.max(0, localAheadMeasures);
}

/** Incremental OSMD rendering is append-only, so already-rendered repeats need no redraw. */
export function shouldAdvanceRenderFrontier(
  renderedThroughMeasure: number,
  targetMeasure: number,
): boolean {
  return targetMeasure > renderedThroughMeasure;
}

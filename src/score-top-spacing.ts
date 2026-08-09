// Copyright (c) 2026 Michael Saunders

export type VerticalExtent = {
  top: number;
  bottom: number;
};

/**
 * Move an action stack just far enough upward to clear the notation in its
 * column. Keeping this calculation in pixels makes it follow OSMD zoom and
 * the browser's actual font metrics on every platform.
 */
export function topClearingNotation(
  controls: VerticalExtent,
  notationTop: number,
  gap: number,
): number {
  if (![controls.top, controls.bottom, notationTop, gap].every(Number.isFinite)) {
    return controls.top;
  }
  return controls.top - Math.max(0, controls.bottom + Math.max(0, gap) - notationTop);
}

/**
 * Translate the rendered score so the topmost required item (notation,
 * action controls, or the staff itself) starts at the desired outer padding.
 */
export function scoreContentTopShift(
  requiredTops: readonly number[],
  outerPadding: number,
): number {
  const finiteTops = requiredTops.filter(Number.isFinite);
  if (finiteTops.length === 0) return 0;
  return Math.max(0, outerPadding) - Math.min(...finiteTops);
}

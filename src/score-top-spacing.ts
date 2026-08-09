// Copyright (c) 2026 Michael Saunders

/**
 * Place the audition row a fixed number of its own button heights below the
 * score viewport.
 */
export function scoreActionTop(
  iconHeight: number,
  iconHeightsBelowTop: number,
): number {
  if (![iconHeight, iconHeightsBelowTop].every(Number.isFinite)) return 0;
  return Math.max(0, iconHeight) * Math.max(0, iconHeightsBelowTop);
}

/** Return the flex gap needed for row tops to be N icon heights apart. */
export function scoreActionRowGap(
  iconHeight: number,
  rowOffsetIconHeights: number,
): number {
  if (![iconHeight, rowOffsetIconHeights].every(Number.isFinite)) return 0;
  return Math.max(0, iconHeight) * Math.max(0, rowOffsetIconHeights - 1);
}

/**
 * Translate the engraving so its highest visible ink begins at the first
 * position that clears the fixed action rows.
 */
export function scoreContentTopShift(
  currentContentTop: number,
  requiredContentTop: number,
): number {
  if (![currentContentTop, requiredContentTop].every(Number.isFinite)) return 0;
  return requiredContentTop - currentContentTop;
}

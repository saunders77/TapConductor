// Copyright (c) 2026 Michael Saunders

/**
 * Place the audition row at a fixed pixel offset below the score viewport.
 */
export function scoreActionTop(
  topOffset: number,
): number {
  return Number.isFinite(topOffset) ? Math.max(0, topOffset) : 0;
}

/** Return the flex gap needed for row tops to be the requested pixels apart. */
export function scoreActionRowGap(
  iconHeight: number,
  rowTopOffset: number,
): number {
  if (![iconHeight, rowTopOffset].every(Number.isFinite)) return 0;
  return Math.max(0, Math.max(0, rowTopOffset) - Math.max(0, iconHeight));
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

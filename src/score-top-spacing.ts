// Copyright (c) 2026 Michael Saunders

/**
 * Place the audition row a fixed number of its own button heights below the
 * score viewport. The start-here row follows immediately in the same stack.
 */
export function scoreActionTop(
  iconHeight: number,
  iconHeightsBelowTop: number,
): number {
  if (![iconHeight, iconHeightsBelowTop].every(Number.isFinite)) return 0;
  return Math.max(0, iconHeight) * Math.max(0, iconHeightsBelowTop);
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

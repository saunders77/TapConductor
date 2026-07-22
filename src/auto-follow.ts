/**
 * Returns the scroll position needed to keep a slice from running off the
 * right edge of the score viewport. `undefined` means the current view is
 * already suitable.
 */
export function autoFollowTarget(
  sliceInScrollContent: number,
  scrollLeft: number,
  viewportWidth: number,
  measureWidth: number,
): number | undefined {
  const sliceInViewport = sliceInScrollContent - scrollLeft;
  if (sliceInViewport < viewportWidth - measureWidth) return undefined;
  return Math.max(0, sliceInScrollContent - measureWidth);
}

// Copyright (c) 2026 Michael Saunders
/**
 * Returns the scroll position needed to keep a slice visible with one measure
 * of context to its left. Forward playback follows before entering the final
 * measure-width of the viewport; backward jumps (such as repeats) follow once
 * the slice is off the left edge. `undefined` means the current view is
 * already suitable.
 */
export function autoFollowTarget(
  sliceInScrollContent: number,
  scrollLeft: number,
  viewportWidth: number,
  measureWidth: number,
): number | undefined {
  const sliceInViewport = sliceInScrollContent - scrollLeft;
  if (sliceInViewport < 0 || sliceInViewport >= viewportWidth - measureWidth) {
    return Math.max(0, sliceInScrollContent - measureWidth);
  }
  return undefined;
}

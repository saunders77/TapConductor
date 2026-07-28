import assert from "node:assert/strict";
import test from "node:test";
import { autoFollowTarget } from "./auto-follow.ts";

test("auto-follow advances when the slice reaches the final measure width", () => {
  assert.equal(autoFollowTarget(900, 0, 1_000, 180), 720);
});

test("auto-follow leaves the viewport alone while the slice has room", () => {
  assert.equal(autoFollowTarget(700, 0, 1_000, 180), undefined);
});

test("auto-follow returns to a repeated slice that jumped off the left edge", () => {
  assert.equal(autoFollowTarget(360, 800, 1_000, 180), 180);
});

test("auto-follow leaves a visible slice alone after a backward jump", () => {
  assert.equal(autoFollowTarget(900, 800, 1_000, 180), undefined);
});

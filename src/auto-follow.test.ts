import assert from "node:assert/strict";
import test from "node:test";
import { autoFollowTarget } from "./auto-follow.ts";

test("auto-follow advances when the slice reaches the final measure width", () => {
  assert.equal(autoFollowTarget(900, 0, 1_000, 180), 720);
});

test("auto-follow leaves the viewport alone while the slice has room", () => {
  assert.equal(autoFollowTarget(700, 0, 1_000, 180), undefined);
});

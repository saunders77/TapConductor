// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  semanticRenderTarget,
  shouldAdvanceRenderFrontier,
} from "./incremental-score-render.ts";

test("normal playback renders a local window ahead", () => {
  const events = [{ measureIndex: 10 }, { measureIndex: 11 }];
  assert.equal(semanticRenderTarget(events, 0, 12), 23);
});

test("al coda renders through the forward semantic destination", () => {
  const events = [{ measureIndex: 72 }, { measureIndex: 148 }];
  assert.equal(semanticRenderTarget(events, 0, 12), 160);
});

test("a backward repeat reuses notation retained behind the frontier", () => {
  const events = [{ measureIndex: 48 }, { measureIndex: 12 }];
  const target = semanticRenderTarget(events, 0, 12);
  assert.equal(target, 60);
  assert.equal(shouldAdvanceRenderFrontier(72, target), false);
  assert.equal(shouldAdvanceRenderFrontier(40, target), true);
});

test("leaving a first ending renders the second ending before the jump", () => {
  const events = [{ measureIndex: 31 }, { measureIndex: 44 }];
  assert.equal(semanticRenderTarget(events, 0, 12), 56);
});

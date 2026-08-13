// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";

import { detectAppleUiPlatform, initialTelemetryAction } from "./ui-platform.ts";

test("detects an iPhone from its device family without screen dimensions", () => {
  assert.equal(
    detectAppleUiPlatform("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)", 5),
    "ios",
  );
  assert.equal(
    detectAppleUiPlatform("Mozilla/5.0 (iPod touch; CPU iPhone OS 17_0 like Mac OS X)", 5),
    "ios",
  );
});

test("detects an iPad using its native user agent", () => {
  assert.equal(detectAppleUiPlatform("Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)", 5), "ipados");
});

test("detects an iPad using its desktop-class user agent", () => {
  assert.equal(detectAppleUiPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)", 5), "ipados");
});

test("distinguishes macOS from an iPad desktop-class user agent", () => {
  assert.equal(detectAppleUiPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", 0), "macos");
});

test("does not classify other platforms as Apple UI platforms", () => {
  assert.equal(detectAppleUiPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", 0), null);
});

test("honors an explicit installer opt-out", () => {
  assert.equal(initialTelemetryAction(false), "disable");
});

test("defaults telemetry on with no installer choice", () => {
  assert.equal(initialTelemetryAction(null), "enable");
  assert.equal(initialTelemetryAction(true), "enable");
});

// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";

import { detectAppleUiPlatform, initialTelemetryAction } from "./ui-platform.ts";

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

test("prompts fresh native Apple installs when no installer choice exists", () => {
  assert.equal(initialTelemetryAction(null, false, "macos"), "prompt");
  assert.equal(initialTelemetryAction(null, false, "ipados"), "prompt");
});

test("honors an explicit installer choice before applying platform defaults", () => {
  assert.equal(initialTelemetryAction(true, false, null), "enable");
  assert.equal(initialTelemetryAction(false, false, "macos"), "disable");
});

test("keeps telemetry off by default for web and native builds without a consent surface", () => {
  assert.equal(initialTelemetryAction(null, true, "macos"), "disable");
  assert.equal(initialTelemetryAction(null, false, null), "disable");
});

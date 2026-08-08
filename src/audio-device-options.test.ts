// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";

import { audioDeviceOptions } from "./audio-device-options.ts";

test("collapses the macOS default endpoint and system-default route", () => {
  assert.deepEqual(audioDeviceOptions([
    { id: "cpal:0:Mac Mini Speakers", name: "Mac Mini Speakers", isDefault: true },
  ]), [
    { label: "Mac Mini Speakers (default)", value: "" },
  ]);
});

test("collapses the iPad default endpoint and system-default route", () => {
  assert.deepEqual(audioDeviceOptions([
    { id: "cpal:0:Default Device", name: "Default Device", isDefault: true },
  ]), [
    { label: "Default Device (default)", value: "" },
  ]);
});

test("keeps ASIO and non-ASIO routes distinct from the Windows system default", () => {
  assert.deepEqual(audioDeviceOptions([
    { id: "asio:Studio Interface", name: "Studio Interface (ASIO)", isDefault: false },
    { id: "cpal:0:Studio Interface", name: "Studio Interface", isDefault: true },
    { id: "cpal:1:HDMI", name: "HDMI", isDefault: false },
  ]), [
    { label: "Studio Interface (default)", value: "" },
    { label: "Studio Interface (ASIO)", value: "asio:Studio Interface" },
    { label: "HDMI", value: "cpal:1:HDMI" },
  ]);
});

test("retains the generic system-default route when discovery finds no default", () => {
  assert.deepEqual(audioDeviceOptions([]), [
    { label: "System default", value: "" },
  ]);
});

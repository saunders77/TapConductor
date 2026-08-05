import assert from "node:assert/strict";
import test from "node:test";
import { validateBatch } from "./worker.mjs";

function event(properties = {}) {
  return {
    event: "session_started",
    timestamp: "2026-08-05T12:00:00.000Z",
    properties: {
      schema_version: 1,
      event_id: "event-1",
      $insert_id: "event-1",
      distinct_id: "device-1",
      device_instance_id: "device-1",
      installation_id: "installation-1",
      session_id: "session-1",
      app_version: "0.1.0",
      build_number: "1",
      release_channel: "test",
      distribution: "native",
      app_platform: "windows",
      os_family: "Windows",
      os_version: "11",
      cpu_arch: "x86_64",
      locale: "en-US",
      telemetry_sdk_version: "tapconductor-ts/1",
      $process_person_profile: false,
      launch_kind: "normal",
      previous_session_unclean: false,
      ...properties,
    },
  };
}

test("accepts the allow-listed envelope", () => {
  assert.deepEqual(validateBatch({ batch: [event()] }), { ok: true });
});

test("rejects file paths and arbitrary properties", () => {
  assert.deepEqual(
    validateBatch({ batch: [event({ file_path: "C:/private/score.musicxml" })] }),
    { ok: false, reason: "forbidden_property" },
  );
});

test("rejects mismatched idempotency IDs", () => {
  assert.deepEqual(
    validateBatch({ batch: [event({ $insert_id: "different" })] }),
    { ok: false, reason: "invalid_envelope" },
  );
});

test("requires application version and correlation identifiers", () => {
  const incomplete = event();
  delete incomplete.properties.app_version;
  assert.deepEqual(
    validateBatch({ batch: [incomplete] }),
    { ok: false, reason: "incomplete_envelope" },
  );
});

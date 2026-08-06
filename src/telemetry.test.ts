// Copyright (c) 2026 Michael Saunders
import assert from "node:assert/strict";
import test from "node:test";
import {
  TelemetryClient,
  countBucket,
  durationQuarterNotes,
  millisecondsBucket,
  type StorageLike,
  type TelemetryConfig,
  type TelemetryDependencies,
} from "./telemetry.ts";

class MemoryStorage implements StorageLike {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

function harness() {
  let wallNow = Date.parse("2026-08-05T12:00:00.000Z");
  let monotonicNow = 0;
  let nextId = 0;
  let nextTimer = 0;
  const storage = new MemoryStorage();
  const requests: Array<Record<string, unknown>> = [];
  const requestUrls: string[] = [];
  const timeouts = new Map<number, () => void>();
  const intervals = new Map<number, () => void>();
  const dependencies: TelemetryDependencies = {
    storage,
    fetch: (async (input, init) => {
      requestUrls.push(String(input));
      requests.push(JSON.parse(String(init?.body)) as Record<string, unknown>);
      return new Response(null, { status: 200 });
    }) as typeof fetch,
    now: () => wallNow,
    monotonicNow: () => monotonicNow,
    randomId: () => `00000000-0000-4000-8000-${String(++nextId).padStart(12, "0")}`,
    isForeground: () => true,
    locale: () => "en-US",
    platform: () => ({ appPlatform: "windows", osFamily: "Windows", osVersion: "11", cpuArch: "x86_64" }),
    sendBeacon: () => true,
    setTimeout: ((callback: () => void) => {
      const id = ++nextTimer;
      timeouts.set(id, callback);
      return id;
    }) as typeof window.setTimeout,
    clearTimeout: ((id: number) => timeouts.delete(id)) as typeof window.clearTimeout,
    setInterval: ((callback: () => void) => {
      const id = ++nextTimer;
      intervals.set(id, callback);
      return id;
    }) as typeof window.setInterval,
    clearInterval: ((id: number) => intervals.delete(id)) as typeof window.clearInterval,
  };
  const config: TelemetryConfig = {
    posthogProjectKey: "phc_test",
    endpoint: "https://us.i.posthog.com/batch/",
    appVersion: "1.2.3",
    buildNumber: "45",
    releaseChannel: "test",
    distribution: "native",
  };
  return {
    client: new TelemetryClient(config, dependencies),
    requests,
    requestUrls,
    storage,
    timeouts,
    intervals,
    advance(milliseconds: number) {
      wallNow += milliseconds;
      monotonicNow += milliseconds;
    },
  };
}

async function settle(): Promise<void> {
  await new Promise<void>((resolve) => setImmediate(resolve));
}

function events(request: Record<string, unknown>): Array<Record<string, unknown>> {
  return request.batch as Array<Record<string, unknown>>;
}

test("first opt-in immediately sends one install and one launch with app version", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  assert.equal(state.requests.length, 1);
  assert.deepEqual(state.requestUrls, ["https://us.i.posthog.com/batch/"]);
  assert.equal(state.requests[0]!.api_key, "phc_test");
  assert.deepEqual(events(state.requests[0]!).map((event) => event.event), ["app_installed", "session_started"]);
  for (const event of events(state.requests[0]!)) {
    assert.equal((event.properties as Record<string, unknown>).app_version, "1.2.3");
  }
});

test("opt-out sends no request and removes identifiers and spool", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  state.client.capture("rhythm_settings_changed", { performance_mode: "rhythm" });
  state.client.disable();
  await settle();
  assert.equal(state.requests.length, 1);
  assert.equal([...state.storage.values.keys()].some((key) => key.includes("device_id")), false);
  assert.equal([...state.storage.values.keys()].some((key) => key.includes("queue")), false);
});

test("declining on the initial run sends neither install nor launch", async () => {
  const state = harness();
  state.client.disable();
  await settle();
  assert.equal(state.requests.length, 0);
  assert.equal(state.storage.values.get("tapconductor.telemetry.v1.consent"), "disabled");
});

test("idle checkpoints do not send heartbeats", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  state.advance(10 * 60_000);
  for (const checkpoint of state.intervals.values()) checkpoint();
  await settle();
  assert.equal(state.requests.length, 1);
});

test("ordinary events share a batch and cannot upload more than every five minutes", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  state.client.capture("rhythm_settings_changed", { performance_mode: "rhythm" });
  state.client.capture("roll_settings_changed", { roll_enabled: true });
  assert.equal(state.requests.length, 1);
  state.advance(5 * 60_000);
  for (const callback of [...state.timeouts.values()]) callback();
  await settle();
  assert.equal(state.requests.length, 2);
  assert.deepEqual(events(state.requests[1]!).map((event) => event.event), [
    "rhythm_settings_changed",
    "roll_settings_changed",
  ]);
});

test("repeated handled errors become one aggregate", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  state.client.recordError({ errorCode: "audio.not_ready", component: "audio", operation: "open" });
  state.client.recordError({ errorCode: "audio.not_ready", component: "audio", operation: "open" });
  state.advance(5 * 60_000);
  for (const callback of [...state.timeouts.values()]) callback();
  await settle();
  const aggregate = events(state.requests[1]!)[0]!;
  assert.equal(aggregate.event, "app_error");
  assert.equal((aggregate.properties as Record<string, unknown>).occurrence_count, 2);
});

test("the client strips non-schema property names before persistence", async () => {
  const state = harness();
  state.client.enable();
  await settle();
  state.client.capture("score_loaded", {
    source_kind: "user_file",
    file_path: "C:/private/score.musicxml",
    "invalid-property": "not allowed",
  });
  state.advance(5 * 60_000);
  for (const callback of [...state.timeouts.values()]) callback();
  await settle();
  const properties = events(state.requests[1]!)[0]!.properties as Record<string, unknown>;
  assert.equal(properties.source_kind, "user_file");
  assert.equal("invalid-property" in properties, false);
});

test("score length and bucket helpers are bounded and deterministic", () => {
  assert.equal(durationQuarterNotes({ events: [{ notes: [{ end: { numerator: 9, denominator: 2 } }] }] }), 4.5);
  assert.equal(countBucket(51), "51-200");
  assert.equal(millisecondsBucket(2_500), "2000-9999");
});

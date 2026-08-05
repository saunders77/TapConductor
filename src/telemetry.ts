/**
 * Cross-platform product telemetry for both Tauri WebViews and the static web build.
 *
 * This module deliberately avoids vendor SDKs. Product events are small, typed at
 * their call sites, persisted in a bounded queue, and sent through PostHog's batch
 * endpoint. It never sees score contents, paths,
 * device names, MIDI messages, or real-time audio callbacks.
 */

export type TelemetryConsent = "unknown" | "enabled" | "disabled";

type JsonPrimitive = string | number | boolean | null;
export type TelemetryProperties = Record<string, JsonPrimitive | readonly JsonPrimitive[]>;

interface StoredEvent {
  eventName: string;
  eventId: string;
  occurredAtUtc: string;
  properties: TelemetryProperties;
}

interface SessionRecord {
  sessionId: string;
  startedAtUtc: string;
  wallDurationSeconds: number;
  activeDurationSeconds: number;
  tapCount: number;
  scoreLoadCount: number;
  errorCount: number;
  closed: boolean;
}

interface ErrorAggregate {
  errorId: string;
  fingerprint: string;
  errorCode: string;
  component: string;
  operation: string;
  severity: "warning" | "error";
  occurrenceCount: number;
  firstOccurredAtUtc: string;
  lastOccurredAtUtc: string;
  context: TelemetryProperties;
}

export interface TelemetryError {
  errorCode: string;
  component: string;
  operation: string;
  severity?: "warning" | "error";
  context?: TelemetryProperties;
}

export interface TelemetryConfig {
  posthogProjectKey: string;
  endpoint: string;
  appVersion: string;
  buildNumber: string;
  releaseChannel: string;
  distribution: string;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export interface TelemetryDependencies {
  storage: StorageLike;
  fetch: typeof fetch;
  now: () => number;
  monotonicNow: () => number;
  randomId: () => string;
  isForeground: () => boolean;
  locale: () => string;
  platform: () => ReturnType<typeof platformProperties>;
  sendBeacon: (url: string, body: Blob) => boolean;
  setTimeout: typeof window.setTimeout;
  clearTimeout: typeof window.clearTimeout;
  setInterval: typeof window.setInterval;
  clearInterval: typeof window.clearInterval;
}

const KEY_PREFIX = "tapconductor.telemetry.v1";
// PostHog project tokens are public ingestion identifiers intended to ship in
// client applications. This is not a personal or administrative API key.
const DEFAULT_POSTHOG_PROJECT_KEY = "phc_vrFBPUnAAgVUhWxViveC38TjS4LKuqJQ88C8WnsMZhkH";
const CONSENT_KEY = `${KEY_PREFIX}.consent`;
const DEVICE_ID_KEY = `${KEY_PREFIX}.device_id`;
const INSTALLATION_ID_KEY = `${KEY_PREFIX}.installation_id`;
const INSTALLED_VERSION_KEY = `${KEY_PREFIX}.installed_version`;
const QUEUE_KEY = `${KEY_PREFIX}.queue`;
const OPEN_SESSION_KEY = `${KEY_PREFIX}.open_session`;
const RESET_COUNTER_KEY = `${KEY_PREFIX}.reset_counter`;
const SCHEMA_VERSION = 1;
const MAX_QUEUE_EVENTS = 500;
const MAX_QUEUE_BYTES = 1_048_576;
const MAX_UPLOAD_EVENTS = 100;
const MAX_ERROR_FINGERPRINTS = 32;
const ORDINARY_UPLOAD_INTERVAL_MS = 5 * 60_000;
const LOCAL_CHECKPOINT_INTERVAL_MS = 60_000;
const IDLE_AFTER_MS = 5 * 60_000;
const LIFECYCLE_EVENTS = new Set([
  "app_installed",
  "browser_instance_created",
  "app_updated",
  "session_started",
  "session_recovered",
  "app_crashed",
  "session_ended",
]);
const EVENT_PROPERTY_ALLOWLIST: Readonly<Record<string, ReadonlySet<string>>> = {
  app_installed: new Set(["initial_app_version", "distribution"]),
  browser_instance_created: new Set(["initial_app_version", "distribution"]),
  app_updated: new Set(["from_version", "to_version"]),
  session_started: new Set(["launch_kind", "previous_session_unclean"]),
  score_loaded: new Set([
    "source_kind", "file_format", "duration_seconds", "structural_duration_quarter_notes",
    "duration_bucket", "part_count_bucket", "tap_event_count_bucket", "load_duration_ms_bucket",
    "warning_count_bucket", "result",
  ]),
  midi_settings_changed: new Set([
    "input_enabled", "output_enabled", "input_connection", "output_connection",
    "channel_filter_mode", "velocity_curve", "sustain_enabled",
  ]),
  audio_settings_changed: new Set([
    "backend", "output_kind", "sample_rate_hz", "buffer_frames", "channel_count",
    "internal_audio_enabled", "estimated_latency_ms_bucket",
  ]),
  rhythm_settings_changed: new Set([
    "performance_mode", "beat_mode", "legato_enabled", "meter_family", "subdivision", "tempo_source",
  ]),
  roll_settings_changed: new Set([
    "roll_enabled", "roll_order", "tap_spread_ms_bucket", "chord_spread_ms_bucket", "gate_policy",
  ]),
  app_error: new Set([
    "error_id", "error_code", "component", "severity", "handled", "operation", "fingerprint",
    "occurrence_count", "first_occurred_at_utc", "last_occurred_at_utc", "source_kind",
    "file_format", "backend", "output_kind", "input_enabled", "output_enabled",
  ]),
  app_crashed: new Set([
    "error_id", "crash_kind", "component", "signal_or_exception_class",
    "last_checkpoint_age_bucket", "sentry_event_id",
  ]),
  session_recovered: new Set([
    "active_duration_seconds", "wall_duration_seconds", "tap_count", "score_load_count",
    "error_count", "last_checkpoint_age_bucket", "end_reason",
  ]),
  session_ended: new Set([
    "end_reason", "active_duration_seconds", "wall_duration_seconds", "tap_count",
    "score_load_count", "error_count",
  ]),
};

const unavailableStorage: StorageLike = {
  getItem: () => null,
  setItem: () => undefined,
  removeItem: () => undefined,
};

function browserStorage(): StorageLike {
  try {
    return window.localStorage;
  } catch {
    return unavailableStorage;
  }
}

function randomUuid(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = [...bytes].map((value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

const defaultDependencies = (): TelemetryDependencies => ({
  storage: browserStorage(),
  fetch: window.fetch.bind(window),
  now: () => Date.now(),
  monotonicNow: () => performance.now(),
  randomId: randomUuid,
  isForeground: () => !document.hidden,
  locale: () => navigator.language || "unknown",
  platform: platformProperties,
  sendBeacon: (url, body) => navigator.sendBeacon?.(url, body) ?? false,
  setTimeout: window.setTimeout.bind(window),
  clearTimeout: window.clearTimeout.bind(window),
  setInterval: window.setInterval.bind(window),
  clearInterval: window.clearInterval.bind(window),
});

export class TelemetryClient {
  private readonly config: TelemetryConfig;
  private readonly dependencies: TelemetryDependencies;
  private consent: TelemetryConsent;
  private session: SessionRecord | null = null;
  private sessionStartedMonotonic = 0;
  private lastAccountedMonotonic = 0;
  private lastActivityMonotonic = 0;
  private foreground: boolean;
  private errors = new Map<string, ErrorAggregate>();
  private flushTimer: number | null = null;
  private checkpointTimer: number | null = null;
  private lastUploadMonotonic = Number.NEGATIVE_INFINITY;
  private flushing = false;
  private ended = false;

  constructor(
    config: TelemetryConfig,
    dependencies?: TelemetryDependencies,
  ) {
    this.config = config;
    this.dependencies = dependencies ?? defaultDependencies();
    this.consent = this.readConsent();
    this.foreground = this.dependencies.isForeground();
  }

  getConsent(): TelemetryConsent {
    return this.consent;
  }

  isConfigured(): boolean {
    return this.config.posthogProjectKey.length > 0 && this.config.endpoint.length > 0;
  }

  getDeviceInstanceId(): string | null {
    return this.consent === "enabled" ? this.read(DEVICE_ID_KEY) : null;
  }

  start(): void {
    if (this.consent === "enabled") this.startConsentedSession();
  }

  enable(): void {
    if (this.consent === "enabled") return;
    this.consent = "enabled";
    this.write(CONSENT_KEY, "enabled");
    this.ended = false;
    this.startConsentedSession();
  }

  disable(): void {
    if (this.session) this.accountTime();
    this.stopTimers();
    this.consent = "disabled";
    this.session = null;
    this.errors.clear();
    this.remove(QUEUE_KEY);
    this.remove(OPEN_SESSION_KEY);
    this.remove(DEVICE_ID_KEY);
    this.remove(INSTALLATION_ID_KEY);
    this.remove(INSTALLED_VERSION_KEY);
    this.write(CONSENT_KEY, "disabled");
  }

  resetIdentifiers(): void {
    const wasEnabled = this.consent === "enabled";
    this.stopTimers();
    this.session = null;
    this.errors.clear();
    this.remove(QUEUE_KEY);
    this.remove(OPEN_SESSION_KEY);
    this.remove(DEVICE_ID_KEY);
    this.remove(INSTALLATION_ID_KEY);
    this.remove(INSTALLED_VERSION_KEY);
    const resetCount = Number(this.read(RESET_COUNTER_KEY) ?? "0");
    this.write(RESET_COUNTER_KEY, String(Number.isFinite(resetCount) ? resetCount + 1 : 1));
    if (wasEnabled) {
      this.ended = false;
      this.startConsentedSession();
    }
  }

  markActivity(): void {
    if (!this.session) return;
    this.accountTime();
    this.lastActivityMonotonic = this.dependencies.monotonicNow();
  }

  setForeground(foreground: boolean): void {
    if (!this.session || foreground === this.foreground) return;
    this.accountTime();
    this.foreground = foreground;
    if (foreground) this.lastActivityMonotonic = this.dependencies.monotonicNow();
    this.checkpoint();
  }

  recordTap(): void {
    if (!this.session) return;
    this.markActivity();
    this.session.tapCount += 1;
  }

  recordScoreLoaded(properties: TelemetryProperties): void {
    if (!this.session) return;
    this.markActivity();
    this.session.scoreLoadCount += 1;
    this.capture("score_loaded", properties);
  }

  capture(eventName: string, properties: TelemetryProperties = {}): void {
    if (!this.session || this.consent !== "enabled") return;
    this.markActivity();
    this.enqueue(this.makeEvent(eventName, properties));
    this.scheduleOrdinaryFlush();
  }

  recordError(error: TelemetryError): void {
    if (!this.session || this.consent !== "enabled") return;
    this.session.errorCount += 1;
    const safeContext = sanitizeEventProperties("app_error", error.context ?? {});
    const fingerprint = [
      error.errorCode,
      error.component,
      error.operation,
      JSON.stringify(safeContext),
    ].join("|");
    const now = new Date(this.dependencies.now()).toISOString();
    const existing = this.errors.get(fingerprint);
    if (existing) {
      existing.occurrenceCount += 1;
      existing.lastOccurredAtUtc = now;
    } else if (this.errors.size < MAX_ERROR_FINGERPRINTS - 1) {
      this.errors.set(fingerprint, {
        errorId: this.dependencies.randomId(),
        fingerprint: `${error.errorCode}:${error.component}:${error.operation}`,
        errorCode: error.errorCode,
        component: error.component,
        operation: error.operation,
        severity: error.severity ?? "error",
        occurrenceCount: 1,
        firstOccurredAtUtc: now,
        lastOccurredAtUtc: now,
        context: safeContext,
      });
    } else {
      const overflowKey = "telemetry.error_fingerprint_overflow";
      const overflow = this.errors.get(overflowKey);
      if (overflow) {
        overflow.occurrenceCount += 1;
        overflow.lastOccurredAtUtc = now;
      } else {
        this.errors.set(overflowKey, {
          errorId: this.dependencies.randomId(),
          fingerprint: overflowKey,
          errorCode: overflowKey,
          component: "telemetry",
          operation: "aggregate_errors",
          severity: "warning",
          occurrenceCount: 1,
          firstOccurredAtUtc: now,
          lastOccurredAtUtc: now,
          context: {},
        });
      }
    }
    this.scheduleOrdinaryFlush();
  }

  async flushNow(reason: "launch" | "ordinary" | "manual" = "manual"): Promise<void> {
    if (this.consent !== "enabled" || !this.isConfigured() || this.flushing) return;
    if (reason === "ordinary") {
      const elapsed = this.dependencies.monotonicNow() - this.lastUploadMonotonic;
      if (elapsed < ORDINARY_UPLOAD_INTERVAL_MS) {
        this.scheduleOrdinaryFlush(ORDINARY_UPLOAD_INTERVAL_MS - elapsed);
        return;
      }
    }
    this.materializeErrors();
    const queue = selectUploadBatch(this.readQueue());
    if (queue.length === 0) return;
    this.flushing = true;
    try {
      const response = await this.dependencies.fetch(this.config.endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(this.batchPayload(queue)),
        credentials: "omit",
        cache: "no-store",
        referrerPolicy: "no-referrer",
      });
      if (!response.ok) throw new Error(`Telemetry endpoint returned ${response.status}.`);
      const sentIds = new Set(queue.map((event) => event.eventId));
      this.writeQueue(this.readQueue().filter((event) => !sentIds.has(event.eventId)));
      this.lastUploadMonotonic = this.dependencies.monotonicNow();
    } catch {
      // Telemetry delivery is deliberately silent and never affects the product.
    } finally {
      this.flushing = false;
      if (this.readQueue().length > 0) this.scheduleOrdinaryFlush();
    }
  }

  endSession(): void {
    if (!this.session || this.ended || this.consent !== "enabled") return;
    this.ended = true;
    this.accountTime();
    this.materializeErrors();
    this.session.closed = true;
    this.enqueue(this.makeEvent("session_ended", this.sessionProperties("normal")));
    this.write(OPEN_SESSION_KEY, JSON.stringify(this.session));
    this.stopTimers();
    this.sendCloseBatch();
  }

  private startConsentedSession(): void {
    if (this.session || !this.isConfigured()) return;
    const previous = this.readJson<SessionRecord>(OPEN_SESSION_KEY);
    const now = this.dependencies.now();
    const monotonic = this.dependencies.monotonicNow();
    const sessionId = this.dependencies.randomId();
    this.session = {
      sessionId,
      startedAtUtc: new Date(now).toISOString(),
      wallDurationSeconds: 0,
      activeDurationSeconds: 0,
      tapCount: 0,
      scoreLoadCount: 0,
      errorCount: 0,
      closed: false,
    };
    this.sessionStartedMonotonic = monotonic;
    this.lastAccountedMonotonic = monotonic;
    this.lastActivityMonotonic = monotonic;
    this.foreground = this.dependencies.isForeground();

    this.ensureId(INSTALLATION_ID_KEY);
    this.ensureId(DEVICE_ID_KEY);
    const installedVersion = this.read(INSTALLED_VERSION_KEY);
    if (!installedVersion) {
      this.enqueue(this.makeEvent(
        this.config.distribution === "web_static" ? "browser_instance_created" : "app_installed",
        {
          initial_app_version: this.config.appVersion,
          distribution: this.config.distribution,
        },
      ));
      this.write(INSTALLED_VERSION_KEY, this.config.appVersion);
    } else if (installedVersion !== this.config.appVersion) {
      this.enqueue(this.makeEvent("app_updated", {
        from_version: installedVersion,
        to_version: this.config.appVersion,
      }));
      this.write(INSTALLED_VERSION_KEY, this.config.appVersion);
    }
    if (previous && !previous.closed && previous.sessionId !== sessionId) {
      this.enqueue(this.makeEvent("session_recovered", {
        active_duration_seconds: previous.activeDurationSeconds,
        wall_duration_seconds: previous.wallDurationSeconds,
        tap_count: previous.tapCount,
        score_load_count: previous.scoreLoadCount,
        error_count: previous.errorCount,
        end_reason: "unclean",
      }));
    }
    this.enqueue(this.makeEvent("session_started", {
      launch_kind: previous && !previous.closed ? "recovery" : "normal",
      previous_session_unclean: Boolean(previous && !previous.closed),
    }));
    this.checkpoint();
    this.checkpointTimer = this.dependencies.setInterval(
      () => this.checkpoint(),
      LOCAL_CHECKPOINT_INTERVAL_MS,
    );
    void this.flushNow("launch");
  }

  private accountTime(): void {
    if (!this.session) return;
    const now = this.dependencies.monotonicNow();
    const activeUntil = this.foreground
      ? Math.min(now, this.lastActivityMonotonic + IDLE_AFTER_MS)
      : this.lastAccountedMonotonic;
    this.session.activeDurationSeconds += Math.max(
      0,
      activeUntil - this.lastAccountedMonotonic,
    ) / 1_000;
    this.session.wallDurationSeconds = Math.max(
      this.session.wallDurationSeconds,
      (now - this.sessionStartedMonotonic) / 1_000,
    );
    this.lastAccountedMonotonic = now;
  }

  private checkpoint(): void {
    if (!this.session || this.consent !== "enabled") return;
    this.accountTime();
    this.write(OPEN_SESSION_KEY, JSON.stringify({
      ...this.session,
      wallDurationSeconds: Math.round(this.session.wallDurationSeconds),
      activeDurationSeconds: Math.round(this.session.activeDurationSeconds),
    }));
  }

  private sessionProperties(endReason: string): TelemetryProperties {
    const session = this.session!;
    return {
      end_reason: endReason,
      active_duration_seconds: Math.round(session.activeDurationSeconds),
      wall_duration_seconds: Math.round(session.wallDurationSeconds),
      tap_count: session.tapCount,
      score_load_count: session.scoreLoadCount,
      error_count: session.errorCount,
    };
  }

  private materializeErrors(): void {
    for (const aggregate of this.errors.values()) {
      this.enqueue(this.makeEvent("app_error", {
        error_id: aggregate.errorId,
        fingerprint: aggregate.fingerprint,
        error_code: aggregate.errorCode,
        component: aggregate.component,
        operation: aggregate.operation,
        severity: aggregate.severity,
        handled: true,
        occurrence_count: aggregate.occurrenceCount,
        first_occurred_at_utc: aggregate.firstOccurredAtUtc,
        last_occurred_at_utc: aggregate.lastOccurredAtUtc,
        ...aggregate.context,
      }));
    }
    this.errors.clear();
  }

  private makeEvent(eventName: string, properties: TelemetryProperties): StoredEvent {
    return {
      eventName,
      eventId: this.dependencies.randomId(),
      occurredAtUtc: new Date(this.dependencies.now()).toISOString(),
      properties: sanitizeEventProperties(eventName, properties),
    };
  }

  private commonProperties(): TelemetryProperties {
    const platform = this.dependencies.platform();
    return {
      schema_version: SCHEMA_VERSION,
      device_instance_id: this.ensureId(DEVICE_ID_KEY),
      installation_id: this.ensureId(INSTALLATION_ID_KEY),
      session_id: this.session?.sessionId ?? "none",
      app_version: this.config.appVersion,
      build_number: this.config.buildNumber,
      release_channel: this.config.releaseChannel,
      distribution: this.config.distribution,
      app_platform: platform.appPlatform,
      os_family: platform.osFamily,
      os_version: platform.osVersion,
      cpu_arch: platform.cpuArch,
      locale: this.dependencies.locale(),
      telemetry_sdk_version: "tapconductor-ts/1",
      $process_person_profile: false,
    };
  }

  private batchPayload(events: StoredEvent[]): Record<string, unknown> {
    const common = this.commonProperties();
    const distinctId = String(common.device_instance_id);
    return {
      api_key: this.config.posthogProjectKey,
      historical_migration: false,
      batch: events.map((event) => ({
        event: event.eventName,
        timestamp: event.occurredAtUtc,
        properties: {
          ...common,
          ...event.properties,
          distinct_id: distinctId,
          event_id: event.eventId,
          $insert_id: event.eventId,
        },
      })),
    };
  }

  private scheduleOrdinaryFlush(delay = ORDINARY_UPLOAD_INTERVAL_MS): void {
    if (this.flushTimer !== null || this.consent !== "enabled") return;
    this.flushTimer = this.dependencies.setTimeout(() => {
      this.flushTimer = null;
      void this.flushNow("ordinary");
    }, Math.max(0, delay));
  }

  private sendCloseBatch(): void {
    if (!this.isConfigured()) return;
    const queue = selectUploadBatch(this.readQueue());
    if (queue.length === 0) return;
    const body = JSON.stringify(this.batchPayload(queue));
    try {
      if (this.dependencies.sendBeacon(
          this.config.endpoint,
          new Blob([body], { type: "application/json" }),
        )) {
        return;
      }
    } catch {
      // Fall through to a keepalive fetch.
    }
    void this.dependencies.fetch(this.config.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
      credentials: "omit",
      keepalive: true,
    }).catch(() => undefined);
  }

  private enqueue(event: StoredEvent): void {
    const queue = this.readQueue();
    queue.push(event);
    while (queue.length > MAX_QUEUE_EVENTS || JSON.stringify(queue).length > MAX_QUEUE_BYTES) {
      const removable = queue.findIndex((candidate) => !LIFECYCLE_EVENTS.has(candidate.eventName));
      queue.splice(removable >= 0 ? removable : 0, 1);
    }
    this.writeQueue(queue);
  }

  private readQueue(): StoredEvent[] {
    const value = this.readJson<unknown>(QUEUE_KEY);
    if (!Array.isArray(value)) return [];
    return value.filter(isStoredEvent).slice(-MAX_QUEUE_EVENTS);
  }

  private writeQueue(queue: StoredEvent[]): void {
    if (queue.length === 0) this.remove(QUEUE_KEY);
    else this.write(QUEUE_KEY, JSON.stringify(queue));
  }

  private readConsent(): TelemetryConsent {
    const value = this.read(CONSENT_KEY);
    return value === "enabled" || value === "disabled" ? value : "unknown";
  }

  private ensureId(key: string): string {
    const existing = this.read(key);
    if (existing) return existing;
    const value = this.dependencies.randomId();
    this.write(key, value);
    return value;
  }

  private read(key: string): string | null {
    try {
      return this.dependencies.storage.getItem(key);
    } catch {
      return null;
    }
  }

  private readJson<T>(key: string): T | null {
    const value = this.read(key);
    if (!value) return null;
    try {
      return JSON.parse(value) as T;
    } catch {
      return null;
    }
  }

  private write(key: string, value: string): void {
    try {
      this.dependencies.storage.setItem(key, value);
    } catch {
      // Storage denial makes telemetry best-effort and non-disruptive.
    }
  }

  private remove(key: string): void {
    try {
      this.dependencies.storage.removeItem(key);
    } catch {
      // Storage denial makes telemetry best-effort and non-disruptive.
    }
  }

  private stopTimers(): void {
    if (this.flushTimer !== null) this.dependencies.clearTimeout(this.flushTimer);
    if (this.checkpointTimer !== null) this.dependencies.clearInterval(this.checkpointTimer);
    this.flushTimer = null;
    this.checkpointTimer = null;
  }
}

export function createTelemetryConfig(webBuild: boolean): TelemetryConfig {
  const projectKey = import.meta.env.VITE_POSTHOG_PROJECT_KEY?.trim()
    || DEFAULT_POSTHOG_PROJECT_KEY;
  const host = import.meta.env.VITE_POSTHOG_HOST?.trim() || "https://us.i.posthog.com";
  return {
    posthogProjectKey: projectKey,
    endpoint: `${host.replace(/\/$/, "")}/batch/`,
    appVersion: __TAPCONDUCTOR_VERSION__,
    buildNumber: import.meta.env.VITE_BUILD_NUMBER?.trim() || __TAPCONDUCTOR_VERSION__,
    releaseChannel: import.meta.env.VITE_RELEASE_CHANNEL?.trim() || "production",
    distribution: webBuild ? "web_static" : "native",
  };
}

export function durationQuarterNotes(properties: {
  structuralDuration?: { numerator: number; denominator: number };
  events: readonly { notes: readonly { end: { numerator: number; denominator: number } }[] }[];
}): number | null {
  if (properties.structuralDuration
    && Number.isFinite(properties.structuralDuration.numerator)
    && Number.isFinite(properties.structuralDuration.denominator)
    && properties.structuralDuration.denominator !== 0) {
    return properties.structuralDuration.numerator / properties.structuralDuration.denominator;
  }
  let maximum: number | null = null;
  for (const event of properties.events) {
    for (const note of event.notes) {
      if (!Number.isFinite(note.end.numerator) || !Number.isFinite(note.end.denominator) || note.end.denominator === 0) continue;
      const value = note.end.numerator / note.end.denominator;
      maximum = maximum === null ? value : Math.max(maximum, value);
    }
  }
  return maximum;
}

export function countBucket(value: number): string {
  if (value <= 0) return "0";
  if (value <= 10) return "1-10";
  if (value <= 50) return "11-50";
  if (value <= 200) return "51-200";
  if (value <= 1_000) return "201-1000";
  return "1001+";
}

export function millisecondsBucket(value: number): string {
  if (value < 100) return "<100";
  if (value < 500) return "100-499";
  if (value < 2_000) return "500-1999";
  if (value < 10_000) return "2000-9999";
  return "10000+";
}

function sanitizeProperties(properties: TelemetryProperties): TelemetryProperties {
  const safe: TelemetryProperties = {};
  for (const [key, value] of Object.entries(properties)) {
    if (!/^[a-z][a-z0-9_]{0,63}$/.test(key)) continue;
    if (Array.isArray(value)) {
      safe[key] = value.slice(0, 32).map(sanitizePrimitive);
    } else {
      safe[key] = sanitizePrimitive(value as JsonPrimitive);
    }
  }
  return safe;
}

function sanitizeEventProperties(
  eventName: string,
  properties: TelemetryProperties,
): TelemetryProperties {
  const allowed = EVENT_PROPERTY_ALLOWLIST[eventName];
  if (!allowed) return {};
  const safe = sanitizeProperties(properties);
  return Object.fromEntries(
    Object.entries(safe).filter(([key]) => allowed.has(key)),
  );
}

function sanitizePrimitive(value: JsonPrimitive): JsonPrimitive {
  if (typeof value === "string") return value.slice(0, 128);
  if (typeof value === "number") return Number.isFinite(value) ? value : 0;
  return value;
}

function isStoredEvent(value: unknown): value is StoredEvent {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<StoredEvent>;
  return typeof candidate.eventName === "string"
    && typeof candidate.eventId === "string"
    && typeof candidate.occurredAtUtc === "string"
    && Boolean(candidate.properties)
    && typeof candidate.properties === "object";
}

function selectUploadBatch(queue: StoredEvent[]): StoredEvent[] {
  const lifecycle = queue.filter((event) => LIFECYCLE_EVENTS.has(event.eventName));
  const ordinary = queue.filter((event) => !LIFECYCLE_EVENTS.has(event.eventName));
  return [...lifecycle, ...ordinary].slice(0, MAX_UPLOAD_EVENTS);
}

function platformProperties(): {
  appPlatform: string;
  osFamily: string;
  osVersion: string;
  cpuArch: string;
} {
  const userAgent = navigator.userAgent;
  const platform = navigator.platform || "unknown";
  const ios = /iPad|iPhone|iPod/.test(userAgent) || (platform === "MacIntel" && navigator.maxTouchPoints > 1);
  const windowsMatch = userAgent.match(/Windows NT ([\d.]+)/);
  const androidMatch = userAgent.match(/Android ([\d.]+)/);
  const appleMatch = userAgent.match(/(?:Mac OS X|CPU (?:iPhone )?OS) ([\d_]+)/);
  const osFamily = ios
    ? "iPadOS/iOS"
    : windowsMatch
      ? "Windows"
      : androidMatch
        ? "Android"
        : /Mac/.test(platform)
          ? "macOS"
          : /Linux/.test(platform)
            ? "Linux"
            : "unknown";
  const osVersion = (windowsMatch?.[1] ?? androidMatch?.[1] ?? appleMatch?.[1] ?? "unknown")
    .replaceAll("_", ".")
    .slice(0, 32);
  const cpuArch = /arm|aarch64/i.test(userAgent)
    ? "arm"
    : /x86_64|Win64|x64|amd64/i.test(userAgent)
      ? "x86_64"
      : "unknown";
  return {
    appPlatform: ios ? "ipados" : osFamily.toLowerCase(),
    osFamily,
    osVersion,
    cpuArch,
  };
}

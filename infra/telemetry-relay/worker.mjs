const ALLOWED_EVENTS = new Set([
  "app_installed",
  "browser_instance_created",
  "app_updated",
  "session_started",
  "score_loaded",
  "midi_settings_changed",
  "audio_settings_changed",
  "rhythm_settings_changed",
  "roll_settings_changed",
  "app_error",
  "app_crashed",
  "session_recovered",
  "session_ended",
]);

// Public PostHog ingestion token. An environment value may override it for a
// staging/fork deployment; never place a personal API key here.
const DEFAULT_POSTHOG_PROJECT_TOKEN = "phc_vrFBPUnAAgVUhWxViveC38TjS4LKuqJQ88C8WnsMZhkH";

const COMMON_PROPERTIES = new Set([
  "schema_version", "device_instance_id", "installation_id", "session_id",
  "app_version", "build_number", "release_channel", "distribution",
  "app_platform", "os_family", "os_version", "cpu_arch", "locale",
  "telemetry_sdk_version", "distinct_id", "event_id", "$insert_id",
  "$process_person_profile",
]);
const REQUIRED_COMMON_PROPERTIES = [
  "schema_version", "device_instance_id", "installation_id", "session_id",
  "app_version", "build_number", "release_channel", "distribution",
  "app_platform", "os_family", "os_version", "cpu_arch", "locale",
  "telemetry_sdk_version", "distinct_id", "event_id", "$insert_id",
  "$process_person_profile",
];

const EVENT_PROPERTIES = {
  app_installed: ["initial_app_version", "distribution"],
  browser_instance_created: ["initial_app_version", "distribution"],
  app_updated: ["from_version", "to_version"],
  session_started: ["launch_kind", "previous_session_unclean"],
  score_loaded: [
    "source_kind", "file_format", "duration_seconds",
    "structural_duration_quarter_notes", "duration_bucket", "part_count_bucket",
    "tap_event_count_bucket", "load_duration_ms_bucket", "warning_count_bucket", "result",
  ],
  midi_settings_changed: [
    "input_enabled", "output_enabled", "input_connection", "output_connection",
    "channel_filter_mode", "velocity_curve", "sustain_enabled",
  ],
  audio_settings_changed: [
    "backend", "output_kind", "sample_rate_hz", "buffer_frames", "channel_count",
    "internal_audio_enabled", "estimated_latency_ms_bucket",
  ],
  rhythm_settings_changed: [
    "performance_mode", "beat_mode", "legato_enabled", "meter_family", "subdivision", "tempo_source",
  ],
  roll_settings_changed: [
    "roll_enabled", "roll_order", "tap_spread_ms_bucket", "chord_spread_ms_bucket", "gate_policy",
  ],
  app_error: [
    "error_id", "error_code", "component", "severity", "handled", "operation",
    "fingerprint", "occurrence_count", "first_occurred_at_utc", "last_occurred_at_utc",
    "source_kind", "file_format", "backend", "output_kind", "input_enabled", "output_enabled",
  ],
  app_crashed: [
    "error_id", "crash_kind", "component", "signal_or_exception_class",
    "last_checkpoint_age_bucket", "sentry_event_id",
  ],
  session_recovered: [
    "active_duration_seconds", "wall_duration_seconds", "tap_count", "score_load_count",
    "error_count", "last_checkpoint_age_bucket", "end_reason",
  ],
  session_ended: [
    "end_reason", "active_duration_seconds", "wall_duration_seconds", "tap_count",
    "score_load_count", "error_count",
  ],
};

const MAX_BODY_BYTES = 256 * 1024;
const MAX_BATCH_EVENTS = 100;

function corsHeaders(origin) {
  return {
    "Access-Control-Allow-Origin": origin,
    "Access-Control-Allow-Methods": "POST, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type",
    "Access-Control-Max-Age": "86400",
    Vary: "Origin",
  };
}

function acceptedOrigin(request, env) {
  const origin = request.headers.get("Origin") ?? "";
  const allowed = String(env.ALLOWED_ORIGINS ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  if (origin && allowed.includes(origin)) return origin;
  if (!origin && request.headers.get("User-Agent")?.includes("TapConductor")) return "*";
  return null;
}

function isPrimitive(value) {
  return value === null || ["string", "number", "boolean"].includes(typeof value);
}

function validPropertyValue(value) {
  if (isPrimitive(value)) return typeof value !== "string" || value.length <= 128;
  return Array.isArray(value) && value.length <= 32 && value.every((item) => isPrimitive(item));
}

export function validateBatch(payload) {
  if (!payload || typeof payload !== "object" || !Array.isArray(payload.batch)) {
    return { ok: false, reason: "invalid_batch" };
  }
  if (payload.batch.length === 0 || payload.batch.length > MAX_BATCH_EVENTS) {
    return { ok: false, reason: "invalid_batch_size" };
  }
  for (const item of payload.batch) {
    if (!item || typeof item !== "object" || !ALLOWED_EVENTS.has(item.event)) {
      return { ok: false, reason: "unknown_event" };
    }
    if (typeof item.timestamp !== "string" || Number.isNaN(Date.parse(item.timestamp))) {
      return { ok: false, reason: "invalid_timestamp" };
    }
    if (!item.properties || typeof item.properties !== "object" || Array.isArray(item.properties)) {
      return { ok: false, reason: "invalid_properties" };
    }
    const allowed = new Set([...COMMON_PROPERTIES, ...(EVENT_PROPERTIES[item.event] ?? [])]);
    for (const [key, value] of Object.entries(item.properties)) {
      if (!allowed.has(key) || !validPropertyValue(value)) {
        return { ok: false, reason: "forbidden_property" };
      }
    }
    if (!REQUIRED_COMMON_PROPERTIES.every((key) => Object.hasOwn(item.properties, key))) {
      return { ok: false, reason: "incomplete_envelope" };
    }
    if (item.properties.schema_version !== 1
      || typeof item.properties.event_id !== "string"
      || typeof item.properties.distinct_id !== "string"
      || item.properties.device_instance_id !== item.properties.distinct_id
      || item.properties.event_id !== item.properties.$insert_id) {
      return { ok: false, reason: "invalid_envelope" };
    }
  }
  return { ok: true };
}

export async function handleRequest(request, env) {
  const origin = acceptedOrigin(request, env);
  if (!origin) return new Response("Origin not allowed", { status: 403 });
  if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders(origin) });
  if (request.method !== "POST") return new Response("Method not allowed", { status: 405 });

  const length = Number(request.headers.get("Content-Length") ?? "0");
  if (length > MAX_BODY_BYTES) return new Response("Payload too large", { status: 413 });
  const body = await request.text();
  if (new TextEncoder().encode(body).byteLength > MAX_BODY_BYTES) {
    return new Response("Payload too large", { status: 413 });
  }

  if (env.RATE_LIMITER) {
    const source = request.headers.get("CF-Connecting-IP") ?? "unknown";
    const result = await env.RATE_LIMITER.limit({ key: source });
    if (!result.success) return new Response("Rate limited", { status: 429, headers: corsHeaders(origin) });
  }

  let payload;
  try {
    payload = JSON.parse(body);
  } catch {
    return new Response("Invalid JSON", { status: 400, headers: corsHeaders(origin) });
  }
  const validation = validateBatch(payload);
  if (!validation.ok) {
    return new Response(validation.reason, { status: 400, headers: corsHeaders(origin) });
  }

  const posthogProjectToken = String(env.POSTHOG_PROJECT_TOKEN || DEFAULT_POSTHOG_PROJECT_TOKEN);

  const countryCandidate = String(request.cf?.country ?? "").toUpperCase();
  const country = /^[A-Z]{2}$/.test(countryCandidate) ? countryCandidate : "ZZ";
  const forwarded = {
    api_key: posthogProjectToken,
    historical_migration: false,
    batch: payload.batch.map((item) => ({
      event: item.event,
      timestamp: item.timestamp,
      properties: {
        ...item.properties,
        country_code: country,
        $geoip_disable: true,
        $process_person_profile: false,
      },
    })),
  };
  const host = String(env.POSTHOG_HOST ?? "https://us.i.posthog.com").replace(/\/$/, "");
  let response;
  try {
    response = await fetch(`${host}/batch/`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(forwarded),
    });
  } catch {
    return new Response("Upstream unavailable", { status: 502, headers: corsHeaders(origin) });
  }
  if (!response.ok) return new Response("Upstream unavailable", { status: 502, headers: corsHeaders(origin) });
  return new Response(null, { status: 204, headers: corsHeaders(origin) });
}

export default { fetch: handleRequest };

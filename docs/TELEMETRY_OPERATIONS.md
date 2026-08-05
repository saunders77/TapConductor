# Telemetry operations

## Release configuration

The client is deliberately unconfigured when `VITE_POSTHOG_PROJECT_KEY` is absent. In that state it
sends nothing and does not show the first-run telemetry choice. Do not put credentials in tracked
source or commit `.env.local`.

For local release testing, copy `.env.example` to `.env.local` and set:

```dotenv
VITE_POSTHOG_PROJECT_KEY=phc_your_public_project_token
VITE_TELEMETRY_ENDPOINT=https://telemetry.tapconductor.app/v1/events
VITE_BUILD_NUMBER=your_build_number
VITE_RELEASE_CHANNEL=production
```

For GitHub builds, create an Actions secret named `TAPCONDUCTOR_POSTHOG_PROJECT_KEY`. The workflows
map it to the Vite variable. Project ID **544266** is useful for PostHog administration/API tooling
but is not needed by the capture client. Confirm that this project is in the US region; otherwise
change both `VITE_POSTHOG_HOST` and the relay's `POSTHOG_HOST` to the EU ingestion host.

## Deploy the relay

Follow `infra/telemetry-relay/README.md`. In outline:

1. Put `telemetry.tapconductor.app` on a Cloudflare-managed zone.
2. Review the allow-listed browser and Tauri origins in `wrangler.jsonc`.
3. Store the PostHog `phc_...` project token with `wrangler secret put POSTHOG_PROJECT_TOKEN`.
4. Deploy, then send a schema-valid canary batch and confirm the country code is present and no IP
   property is stored in PostHog.
5. Keep the Cloudflare rate-limit binding/WAF rule enabled and ensure Worker request logging does not
   retain request bodies or source IPs beyond Cloudflare's necessary service logs.

Until the relay is ready, omitting `VITE_TELEMETRY_ENDPOINT` uses the configured PostHog host's
`/batch/` endpoint directly. That is suitable only for beta testing; configure PostHog to discard IP
data and understand that country derivation then follows PostHog's processing rather than the relay.

## PostHog project checklist

- Disable autocapture, person profiles, session replay, surveys, cookies, and feature flags.
- Set product-event retention to no more than 12 months and document the exact configured value.
- Create dashboards for installs/launches by app version/OS/country, active time and taps, score
  format/source/length/load result, settings adoption, and `app_error` occurrence counts.
- Define alerts at 50%, 75%, and 90% of the monthly product-event allowance. A batch request still
  contains individually billable events.
- Verify `$insert_id` deduplication by replaying a canary event ID.
- Test opt-out with a network inspector: zero relay/PostHog requests, including while idle.

## Sentry decision and credentials

No Sentry SDK or DSN is enabled in the base build. Handled errors and JavaScript failures are
sanitized, deduplicated, and sent to PostHog in ordinary five-minute batches. Consent-gated Rust
panics leave a fixed local marker; the next launch reports `app_crashed` without panic text.

Only add Sentry when a deliberate native crash test demonstrates a need for its native minidumps and
symbolication. At that point:

1. Create Sentry projects for the required native targets and obtain each **project DSN**. A Sentry
   organization token is not a DSN and must never be embedded in an app.
2. Integrate and test the platform-native crash SDK on Windows, macOS, and iPadOS; use the same
   consent gate and correlation tags, with tracing/replay/logs/automatic breadcrumbs disabled.
3. Put the DSN in protected release-build configuration. Put a narrowly scoped release/symbol-upload
   token only in CI, then upload version-matched PDBs/dSYMs/source maps.
4. Configure PII scrubbing and at most 90-day crash retention, deliberately crash each release
   target, and inspect every captured field before shipping.

The organization token pasted into a chat or issue must be revoked and rotated immediately. It is
privileged server/CI material and is neither used nor stored by this repository implementation.

## Release verification

Run:

```text
npx tsc --noEmit
npm run test:telemetry
npm run test:telemetry-relay
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Then manually verify initial Continue/Do not share, later opt-in/out, identifier reset/copy, install
and launch cardinality, five-minute batching, graceful close, idle network silence, offline recovery,
and the Rust panic-marker recovery flow on every release platform.

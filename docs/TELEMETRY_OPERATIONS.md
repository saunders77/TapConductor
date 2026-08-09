# Telemetry operations

## Release configuration

TapConductor sends consented telemetry batches directly to PostHog Cloud. There is no TapConductor
telemetry server, proxy, relay, Cloudflare account, or custom telemetry domain to deploy.

TapConductor's public PostHog project ingestion token is compiled into the client. This is
intentional: every direct capture request must contain it, and it cannot read analytics or
administer the project. It is not equivalent to a PostHog personal API key. Project ID **544266** is
useful for PostHog administration/API tooling but is not needed by the capture client. Standard
local and GitHub release builds require no PostHog secret.

The default client posts to the US ingestion endpoint. For a staging project, EU project, or fork,
copy `.env.example` to `.env.local` and override only what is needed:

```dotenv
VITE_POSTHOG_PROJECT_KEY=phc_staging_or_fork_project_token
VITE_POSTHOG_HOST=https://eu.i.posthog.com
VITE_BUILD_NUMBER=your_build_number
VITE_RELEASE_CHANNEL=production
```

Confirm that project 544266 is hosted in PostHog's US region. If it is an EU project, set
`VITE_POSTHOG_HOST=https://eu.i.posthog.com` in release builds. When rotating or changing the
production project token, update `DEFAULT_POSTHOG_PROJECT_KEY` in `src/telemetry.ts`. A public
ingestion token may be present in source and shipped binaries; personal API keys must never be.

## PostHog project checklist

- Disable autocapture, person profiles, session replay, surveys, cookies, and feature flags.
- Keep GeoIP enrichment enabled if country/region reporting is required. The direct HTTPS request
  exposes its source IP to PostHog for network delivery and approximate geolocation; TapConductor
  does not add an IP property or request OS location permission. Configure PostHog's privacy and
  retention controls accordingly.
- Set product-event retention to no more than 12 months and document the exact configured value.
- Create dashboards for installs/launches by app version/OS/country/region, active time and taps,
  score format/source/length/load result, settings adoption, and `app_error` occurrence counts.
- Define alerts at 50%, 75%, and 90% of the monthly product-event allowance. A batch request still
  contains individually billable events.
- Verify `$insert_id` deduplication by replaying a canary event ID.
- Test opt-out with a network inspector: zero PostHog requests, including while idle.
- Verify a consented event reaches `https://us.i.posthog.com/batch/` (or the configured EU host),
  includes `app_version`, and contains none of the forbidden fields listed in `PRIVACY.md`.

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

## Release verification

Run:

```text
npx tsc --noEmit
npm run test:telemetry
cargo test --locked --workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
```

Then manually verify that fresh/undecided web, development, Windows, macOS, and iPadOS launches
default on; that an unchecked Windows installer choice and saved Info > Privacy opt-out remain off;
and that later opt-in/out, install and launch cardinality, direct PostHog delivery, five-minute
batching, graceful close, idle network silence, offline recovery, and the Rust panic-marker recovery
flow work on every release platform. For the Mac App Store build, also verify outbound delivery from
the sandboxed application.

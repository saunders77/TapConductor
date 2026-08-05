# TapConductor telemetry relay

This Cloudflare Worker accepts only TapConductor's versioned event schema, limits request and batch
size, derives a two-letter country code from Cloudflare edge metadata, disables PostHog IP
geolocation, and forwards the sanitized batch. It never logs or forwards the source IP.

Deployment requires a Cloudflare account/domain and Wrangler:

1. Review `ALLOWED_ORIGINS` and the custom-domain route in `wrangler.jsonc`.
2. Run `npx wrangler deploy` from this directory.
3. Set `VITE_TELEMETRY_ENDPOINT=https://telemetry.tapconductor.app/v1/events` in release builds.

The public production project token is embedded in both the client and Worker. A staging/fork
deployment can override it with `npx wrangler secret put POSTHOG_PROJECT_TOKEN`. Never put a PostHog
personal API key or Sentry organization token in the client or Worker source.

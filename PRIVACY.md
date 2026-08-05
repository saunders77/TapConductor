# TapConductor Privacy Policy

Effective date: August 5, 2026

TapConductor is a local music-performance application with no account system, advertising, sale of
personal data, or cloud synchronization. Scores and performances stay on the device. After a clear
first-run choice, a user may enable pseudonymous usage and diagnostic telemetry to help improve the
application. The app remains fully functional when telemetry is off.

## Data processed on the device

TapConductor processes score files selected by the user, tap and keyboard input, optional MIDI
messages and device names, audio-output information, and realtime performance diagnostics locally
so it can display and play a score. Score contents, filenames and paths, titles/composers, MIDI
messages and note values, audio/MIDI device names, and performance content are not uploaded.

On iPadOS and sandboxed macOS builds, the operating-system document picker may copy a selected score
into the app's private container. The app reads that copy and does not modify the original. The copy
can remain until the operating system clears temporary data or the user removes the app and its data.

The bundled demo scores and grand-piano samples are installed with the application. TapConductor
does not request microphone, camera, location, contacts, photos, local-network, or Bluetooth
permission. Class-compliant MIDI devices are accessed through operating-system MIDI services.

## Optional usage and diagnostic data

When **Share usage and crash data** is on, TapConductor may send:

- first consented use, application launches, updates, closes, and the application/build version;
- session wall time, active non-idle time, score-load count, error count, and total taps (never
  individual tap timing or notes);
- whether a score is a bundled demo or user file, its format, structural length, coarse part/event
  counts, load-time bucket, warning-count bucket, and success/failure category;
- coarse MIDI, audio, rhythm/beat, legato, and chord/tap-roll setting categories, never device names;
- stable sanitized error codes, affected component/operation, severity, coarse safe context, and a
  count of repeated occurrences, never raw exception text, paths, score fragments, or device names;
- random device-instance, installation, session, event, and error correlation identifiers; OS
  family/version, CPU architecture, application platform, locale, timestamp, and release channel;
- a two-letter country code derived by the relay from the network connection. TapConductor does not
  request location permission or retain city, coordinates, postal code, or the source IP.

The identifiers are random and are not derived from hardware serials, advertising identifiers,
email, login, username, or contact information. The data is pseudonymous, not anonymous, because
events from the same application instance can be correlated.

## Sending, processors, and retention

The app stores pending events in bounded origin-scoped application storage and normally sends at
most one batch every five minutes when events exist. First consented install/launch is sent
immediately, and a graceful close attempts one final batch. A healthy open-but-idle app sends no
heartbeat or polling request. Repeating handled errors are combined locally before upload.

The production ingestion relay is operated on Cloudflare and validates a strict schema, derives
country, and forwards accepted data to PostHog in the United States. The relay does not log or
forward the source IP. PostHog is used for product analytics and handled-error aggregates with
person profiles, autocapture, cookies, replay, and advertising features disabled. Release owners
must configure product data retention to no more than 12 months.

Sentry is not used for ordinary handled errors. A future release may enable it only for fatal native
crash dumps that cannot be diagnosed adequately in PostHog; that release must use a project DSN,
scrub personal data, update store disclosures, and apply a retention period no longer than 90 days.
Cloudflare, PostHog, Apple, Microsoft, and distribution stores may independently process ordinary
service, download, or crash information under their own policies.

## User choices and deletion

Until the first-run choice is made, no TapConductor telemetry is sent. Turning sharing off stops
capture immediately and removes local pending events and telemetry identifiers without sending an
opt-out event. Turning it on later creates fresh identifiers and does not upload activity from the
off period. **Reset telemetry identifier** removes the local queue and rotates the identifiers.

Because TapConductor has no account, a server-side deletion request needs the pseudonymous device
identifier. Before turning sharing off or resetting it, use **Copy telemetry identifier** in Help >
Privacy and submit it through the support channel below. Data that has reached its configured
retention limit is deleted automatically.

## Contact

Privacy questions and deletion requests can be filed through the project's public support channel:
<https://github.com/saunders77/TapConductor/issues>. Do not include score files, device names, or
other sensitive information in a public issue; ask for a private submission route when needed.

This repository copy is the source text for the public privacy-policy URL required by the Apple App
Store and Microsoft Store. Release owners must publish the same text at a stable HTTPS URL and keep
that URL available for the lifetime of the store listing.

# TapConductor Privacy Policy

Effective date: August 6, 2026

TapConductor is a local music-performance application with no account system, advertising, sale of
personal data, or cloud synchronization. Scores and performances stay on the device. During the
Windows installation flow, or on first launch after a fresh macOS or iPadOS installation, a user may
enable pseudonymous usage and diagnostic telemetry to help improve the application. No telemetry is
sent before that choice is applied. The choice can be changed later on the app's Info > Privacy
page, and the app remains fully functional when telemetry is off.

## Startup announcements

At startup, TapConductor requests the public `LATEST_ANNOUNCEMENT.md` file from GitHub. When the file
contains an announcement, it also requests the file's latest commit timestamp. This lets the
developer display an announcement and ensures that dismissing one announcement permanently does
not suppress a later announcement. TapConductor sends no score,
performance, MIDI, device, telemetry identifier, account, or contact data in these requests. GitHub
receives ordinary connection information such as the source IP address and request headers under
GitHub's own privacy terms.

If the user selects **Don't show this announcement again**, TapConductor stores only that
announcement's public update timestamp or content identifier in origin-scoped application storage.
It is not uploaded. Clearing the app's site/application data clears this preference. The app makes
no continuing announcement request after the startup check.

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

When **Send pseudonymous crash and usage data to the developer to help improve TapConductor** is on,
TapConductor may send:

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
- approximate country/region derived by PostHog from the direct network connection. TapConductor
  does not request location permission or add an IP address, city, coordinates, or postal code to
  the event payload.

The identifiers are random and are not derived from hardware serials, advertising identifiers,
email, login, username, or contact information. The data is pseudonymous, not anonymous, because
events from the same application instance can be correlated.

## Sending, processors, and retention

The app stores pending telemetry events in bounded origin-scoped application storage and normally sends at
most one batch every five minutes when events exist. First consented install/launch is sent
immediately, and a graceful close attempts one final batch. A healthy open-but-idle app sends no
telemetry heartbeat or continuing announcement request. Repeating handled errors are combined
locally before upload.

The app sends consented batches directly to PostHog in the United States; TapConductor operates no
telemetry intermediary. As with any direct HTTPS service, PostHog receives the connection's source
IP for network delivery and may process it for approximate geographic enrichment. TapConductor does
not include the IP as an event property. PostHog is used for product analytics and handled-error
aggregates with person profiles, autocapture, cookies, replay, and advertising features disabled.
Release owners must configure PostHog privacy controls and product-data retention of no more than
12 months.

Sentry is not used for ordinary handled errors. A future release may enable it only for fatal native
crash dumps that cannot be diagnosed adequately in PostHog; that release must use a project DSN,
scrub personal data, update store disclosures, and apply a retention period no longer than 90 days.
PostHog, Apple, Microsoft, and distribution stores may independently process ordinary
service, download, or crash information under their own policies.

## User choices and deletion

The Windows installer choice is applied on first launch. Because macOS and iPadOS distributions do
not provide the same customizable installer finish page, a fresh Apple installation presents the
equivalent choice inside the app before telemetry starts. Turning sharing off stops capture
immediately and removes local pending events and telemetry identifiers without sending an opt-out
event. Turning it on later creates fresh identifiers and does not upload activity from the off
period.

Because TapConductor has no account and does not expose its random telemetry identifier, it cannot
reliably match a user to telemetry that has already been sent. Turning sharing off deletes pending
events and identifiers stored by the app; data already sent is deleted automatically when it reaches
the configured retention limit.

## Contact

Privacy questions and deletion requests can be filed through the project's public support channel:
<https://github.com/saunders77/TapConductor/issues>. Do not include score files, device names, or
other sensitive information in a public issue; ask for a private submission route when needed.

This repository copy is the source text for the public privacy-policy URL required by the Apple App
Store and Microsoft Store. Release owners must publish the same text at a stable HTTPS URL and keep
that URL available for the lifetime of the store listing.

# TapConductor product and technical plan

> Cross-platform implementation update (July 24, 2026): the additive macOS, iPadOS, and Microsoft
> Store architecture, build commands, package matrix, policy gates, and execution status are in
> [CROSS_PLATFORM_IMPLEMENTATION.md](CROSS_PLATFORM_IMPLEMENTATION.md). Where this older plan calls
> implemented work "future," the cross-platform runbook and source are authoritative.

Status: proposed plan for a new codebase  
Initial platform: Windows 10/11  
Planned platforms: macOS and iPadOS/iOS

## 1. Product definition

TapConductor turns a written score into a performer-controlled sequence of sounding note attacks.
The score supplies pitch and ordering; the performer supplies timing and, when MIDI input is used,
velocity.

The primary workflow is:

1. Open a MusicXML (`.musicxml`, `.xml`) or compressed MusicXML (`.mxl`) score.
2. Inspect the engraved notation and choose which parts are active.
3. Set the live cursor, initially at the first sounding score position.
4. Press a configured keyboard key, mouse/pointer target, or (later) a MIDI key.
5. Hear every active note beginning at that exact musical position as one sample-aligned piano
   attack.
6. See that vertical slice highlighted while the live cursor advances to the next sounding position.
7. Play any single chord or individual note without moving the live cursor, or explicitly move the
   live cursor to any slice.

Rests never consume a tap. A chord on one staff and notes at the same rational score position on
other voices, staves, or parts form one tap event.

### MVP interaction design

- The score occupies most of the window and supports zoom, continuous vertical scrolling, and
  page view.
- A compact top bar contains Open, active parts, output device, volume, panic, and settings.
- A persistent performance strip shows Ready/Loading/Audio fault, the next measure and beat, and a
  large pointer tap target. Space and Enter are the default keyboard triggers.
- The score is engraved as one long horizontal system with a permanently available horizontal
  scrollbar. The current slice uses a strong translucent vertical highlight across all staves.
  The next slice has a lighter preview marker.
- Every slice permanently shows two compact controls: an ear icon (**Play single chord**) plays it
  without changing position, and a downward arrow (**Start here**) changes the live cursor. Selecting
  an individual notehead plays only that note without affecting the cursor or other sounding groups.
- When playback reaches within one measure of the viewport's right edge, the score scrolls so the
  slice has one measure of context to its left. Manual horizontal scrolling remains available.
- Performance input is handled on key/pointer down, not release. Key auto-repeat is ignored.
- Input release is also captured because it participates in the default piano note-off rule, but it
  never delays a new attack.
- Escape or the Panic control immediately releases all voices.
- Keyboard and pointer taps use a configurable fixed velocity (default 96). MIDI Note On velocity
  replaces that value once MIDI input is implemented.

### Explicit non-goals for the first release

- Score editing, recording, tempo-following accompaniment, networking, VST/AU plug-ins, PDF/OMR,
  and automatic correction of malformed scores.
- Perfect engraving of every MusicXML extension.
- Bluetooth audio as a low-latency target. It may work, but the app must warn that wireless output
  normally adds unacceptable performance latency.

## 2. Musical model and deterministic tap semantics

The audio engine must not infer simultaneity from rendered x-coordinates. Engraving can displace
notes for readability. Import creates a semantic timeline using exact rational musical positions.

```text
Score
  -> playback order (measures/repeats/endings)
  -> part / staff / voice note events
  -> exact rational onset and end positions
  -> filter rests and inactive parts
  -> group attacks with identical onset
  -> TapEvent[]
```

Core types should be independent Rust types, with serialization used only at the UI boundary:

```text
ScorePosition = { occurrence, measure_id, offset: Rational }
NoteAttack    = { source_id, staff, voice, midi_pitch, end, tie, velocity_hint }
TapEvent      = { id, position, attacks[], release_boundaries[], display_anchors[] }
Performance  = { cursor_index, active_parts, generation }
```

Rules for the MVP:

- Use the onset derived from MusicXML `divisions`, `duration`, `backup`, `forward`, `voice`, and
  `chord`, represented as reduced integer fractions. Never compare floating-point beats.
- Group attacks only when their absolute rational onsets are equal after playback order is
  constructed. Horizontal visual proximity is irrelevant.
- Exclude rests and hidden/non-playing notes. Cue-note behavior should be a per-score import option,
  defaulting to excluded.
- A tied continuation is not a new attack and does not consume a tap; the initial voice is extended
  to the end of the tie chain.
- Render written pitch but sound concert pitch for transposing instruments.
- Keep note spelling and source IDs even though the piano engine ultimately receives MIDI pitch.
- Grace notes are imported and displayed but skipped by the first MVP playback policy, with a clear
  warning. Their desired tap/roll semantics need rehearsal testing before release.
- Unpitched percussion is displayed but skipped by the piano mode, also with an import warning.
- Initially expand ordinary repeats and first/second endings into playback occurrences. D.C., D.S.,
  Coda, and ambiguous nested navigation should be detected and reported until a tested expansion
  implementation is available.
- When a visually repeated measure maps to multiple playback occurrences, **Start here** selects
  the first occurrence at or after the current cursor; the UI also offers an occurrence picker.

### Default piano note release policy

Every tapped slice has an independent voice group and remembers three times: its attack, the release
of the physical input that triggered it, and the first later tap. Its note-off time is:

```text
note_off = max(first_later_tap, triggering_input_release + 400 ms)
```

The complete behavior is:

- Start every attack immediately on input down.
- Never stop its voice group while its triggering key, pointer button, or MIDI key is still held.
- Ordinarily stop the group exactly when the next tap attacks.
- After the performer releases the triggering input, allow at least 400 ms before stopping that
  group. If the next tap arrives inside that window, start the new group immediately and leave the
  older group sounding until its own 400 ms deadline.
- If the input is still held when one or more later taps occur, keep the group sounding. Once that
  input is released, keep it for the full additional 400 ms and then stop it. Consequently, several
  independently aging groups can overlap during fast tapping.
- If there is no later tap, keep the group active after its 400 ms minimum until a later tap or
  explicit Panic/score unload. A piano will normally have become quiet through its natural decay.
- Apply the rule independently to each TapEvent, but send one sample-aligned all-notes-off command
  for all pitches in that event's group.
- Panic, score unload, reposition, audio-device loss, and application shutdown override the minimum
  and release all groups immediately.

The 400 ms rule is a minimum note-on lifetime, not an envelope stretcher. The sampler must run the
instrument's ordinary envelope while the note is held. It must not loop, amplify, freeze, or otherwise
artificially lengthen a naturally decaying piano strike. A sustaining instrument may maintain its
normal held-note level because that is part of that instrument; the gate policy only decides when to
deliver its note-off.

Keep this behavior behind a `GatePolicy` interface so future modes can use score durations or other
release semantics without changing the sampler or the attack-latency path.

## 3. Recommended stack

### Application shell and UI

- **Tauri 2** for Windows, macOS, and iOS/iPadOS packaging and native integration.
- **TypeScript + Vite** for the UI. Use lightweight DOM state rather than a large application
  framework initially; add one only if UI complexity justifies it.
- **OpenSheetMusicDisplay (OSMD)** for MusicXML engraving to SVG. Keep it solely in the view layer;
  it is not the authority for tap order or simultaneity.
- Platform webviews: WebView2 on Windows and WKWebView on Apple platforms, supplied through Tauri.

The web UI is suitable for score SVG, overlays, zoom, and layout. It must never synthesize the live
audio or own the authoritative performance cursor.

### Native core

- **Stable Rust** for score import, event grouping, performance state, audio scheduling, MIDI, and
  diagnostics.
- Workspace crates:
  - `tapconductor-score`: MusicXML/MIDI import and normalized semantic model.
  - `tapconductor-performance`: cursor and mode state machine; no UI or device dependencies.
  - `tapconductor-audio`: real-time command queue, sampler adapter, mixing, and platform output.
  - `tapconductor-midi`: device abstraction and MIDI input/output mapping.
  - `tapconductor-app`: Tauri commands/events and persistence.
- `quick-xml` plus a ZIP implementation for a deliberately scoped native MusicXML importer.
  Parsing the timing model natively avoids coupling playback correctness to OSMD internals.
- `midly` for Standard MIDI File parsing when `.mid` import is added.
- **RustySynth** as the initial MIT-licensed SoundFont 2 engine candidate. It must pass the phase-zero
  callback-allocation, polyphony, CPU, and onset tests before adoption. Keep it behind a `Sampler`
  trait so it can be replaced without touching score or performance code.
- A properly licensed piano SoundFont/sample set is a separate product asset and must be reviewed
  independently of the synth code. Do not copy a convenient General MIDI bank into the repository.

### Audio backends

- Windows production backend: direct event-driven WASAPI using `IAudioClient3`, initialized at the
  endpoint's native sample rate and smallest stable shared-mode engine period for devices without
  an installed ASIO driver.
- Windows performance backend: native vendor ASIO, prioritized for interfaces such as the Roland
  QUAD-CAPTURE where acoustic tests show that the ASIO path materially outperforms WASAPI. The
  Steinberg SDK has GPLv3 and proprietary licensing routes; TapConductor uses the GPLv3 route and
  the entire application is now distributed under GPLv3-only terms.
- Prototype/enumeration backend: `cpal`, which supports WASAPI, optional ASIO, macOS CoreAudio, and
  iOS CoreAudio. Use it to validate the engine quickly, not as an excuse to accept a larger buffer.
- Apple port: CoreAudio through `cpal` initially, with `AVAudioSession` configured for playback and
  a preferred approximately 5 ms I/O buffer on iPadOS/iOS. Replace only the backend if measurement
  shows a need; the engine and sampler remain shared Rust.

### MIDI

- A platform-neutral `MidiBackend` API in the first architecture even though the MVP UI may not
  expose MIDI.
- `midir` is a suitable first MIDI 1.0 implementation (WinRT/WinMM on Windows and CoreMIDI on Apple).
- Treat every selected MIDI Note On with velocity greater than zero as a tap; ignore its pitch in
  normal conductor mode and map its velocity through a configurable curve to the whole TapEvent.
- Emit Note On/Off to MIDI OUT from the same native event scheduler used by the sampler. Internal
  audio can be disabled while MIDI OUT remains active.
- Preserve device timestamps and define messages internally with room for 16-bit velocity and UMP,
  so later Windows MIDI Services/MIDI 2.0 support does not require changing the performance model.

### Dependency/license posture

The proposed foundational libraries have commercially usable permissive licenses: Tauri is
MIT/Apache-2.0, OSMD is BSD-3-Clause, `cpal` is Apache-2.0, `midir` and RustySynth are MIT, and
`midly` uses the Unlicense. Pin exact versions, commit lockfiles, generate an SBOM/third-party notice
file in CI, and review transitive dependencies and the piano asset before each release.

Avoid making JUCE, MuseScore, or Verovio foundational dependencies in a proprietary product without
a deliberate licensing decision. They are technically capable, but their GPL/LGPL or commercial
terms add obligations the proposed stack does not need for the MVP.

## 4. The real-time path

The live path must be short and must remain functional while the WebView is reflowing a large score:

```text
keyboard/pointer native capture ----+
MIDI callback + timestamp ----------+--> performance state --> SPSC command queue
audition request from score UI ------+                              |
                                                                 audio callback
                                                            sampler + MIDI OUT
                                                                 |
                                                             output device

performance result --> non-real-time UI event --> highlight/scroll
```

Implementation constraints:

- Open and keep the audio stream running before entering Ready state.
- Parse the score, construct all TapEvents, load/decode samples, initialize voices, allocate queues,
  and render a silent warm-up block before Ready.
- Capture `keydown`/`pointerdown` and their matching release events. Process input down immediately;
  release only updates that TapEvent group's gate state. The phase-zero spike measures WebView-to-Rust IPC;
  if its tail latency misses budget, add a Windows native input plug-in (Raw Input/window message
  path) for configured performance keys and the performance tap surface.
- On a tap, advance authoritative cursor state in Rust and enqueue exactly one fixed-capacity
  `PlaySlice` command containing all note attacks. The audio callback gives every attack in that
  command the same sample offset.
- Track overlapping voice groups by TapEvent performance instance rather than MIDI pitch. Repeated
  pitches from newer taps must not steal or prematurely release the same pitches in older groups.
- Schedule the 400 ms minimum on the audio sample clock. UI timers and wall-clock timers are not
  sufficiently precise and must not own note-off timing.
- The callback performs no heap allocation, file or network I/O, logging, mutex acquisition,
  blocking channel operation, UI call, or sample decoding.
- Use a bounded single-producer/single-consumer queue, preallocated voice pool, monotonic clock, and
  atomics for counters. Queue overflow triggers telemetry and a safe audible/UI fault, never a block.
- Keep highlight and auto-scroll asynchronous. Visual delay must not delay sound.
- Handle device changes by muting, rebuilding and warming the stream off the callback, then entering
  Ready. Do not silently fall back to a high-latency device configuration.
- Provide a one-click audio diagnostic showing backend, device, sample rate, buffer frames, estimated
  output latency, callback underruns, queue overflow, and MIDI device state.

### Measurable performance budgets

For a supported wired/built-in output device:

- Native input or MIDI callback to queued sampler command: p99 <= 1 ms.
- WebView pointer/keyboard input to queued command, if used: p99 <= 2 ms under normal UI load.
- Requested audio period: <= 256 frames, target 128 frames at 48 kHz where the driver supports it.
- Measured tap electrical/input event to first non-silent output sample: median <= 8 ms and p99 <=
  12 ms on reference hardware; p99 <= 15 ms during simultaneous score scrolling/reflow.
- Chord onset spread inside one TapEvent: exactly 0 samples.
- Default-gate note-off timing: no earlier than 400 ms after matching input release, with at most one
  audio buffer of scheduling error; new attacks remain subject only to the attack budget.
- No callback allocation or lock; zero underruns in a 30-minute stress performance on reference
  hardware.

These are engineering acceptance thresholds, not a claim that every Windows driver or speaker has
the same acoustic latency. The UI should label configurations that cannot meet the period target.

## 5. State, concurrency, and UI contract

The Rust core owns a monotonically increasing `generation` for each loaded score and a cursor index.
Every UI request includes the generation; stale audition or reposition requests are rejected after a
new score loads.

Only these compact messages cross the native/UI boundary:

- UI to core: open file, set active parts, set cursor by TapEvent ID/occurrence, audition TapEvent,
  tap, panic, and settings changes.
- Core to UI: load progress/warnings, score metadata and compact TapEvent-to-source-ID mapping,
  ready/fault status, cursor/highlight event, devices, and diagnostics.

The UI may optimistically highlight, but it must reconcile with the cursor event from Rust. Direct
audition does not advance the cursor. Reposition always performs all-notes-off first so voices from a
previous location cannot hang.

Persist only preferences and recent-file bookmarks. The source score remains the authority; do not
rewrite it. Product telemetry follows the opt-out, data-minimized design in the next section and
must never contain score content or identifying file/device names.

## 6. Telemetry and crash reporting

### Recommendation and boundaries

Use the small, typed `src/telemetry.ts` subsystem shared by the browser build and every Tauri WebView,
not vendor analytics calls spread through the product. It owns consent, correlation IDs, active-time
accounting, bounded persistence, error aggregation, and upload scheduling. Call sites pass only
allow-listed categories and counters. The audio, MIDI, sampler, and scheduling callbacks never call
it. Native crash adapters are separate because a terminated process cannot depend on WebView code.
The recommended initial services are:

- **PostHog Cloud US** for product events, handled-error aggregates, funnels, retention, and
  dashboards. Use only its batch capture API; disable autocapture, person profiles, session replay,
  surveys, cookies, and feature flags. Every event includes the application version. The host remains
  configurable for an EU project or a later self-hosted deployment. The client sends directly to
  PostHog with its embedded public project ingestion token; no application backend, proxy, custom
  domain, or Cloudflare account is required.
- **Sentry only for fatal native crashes that PostHog cannot symbolicate reliably on a target.** Do
  not send ordinary handled/recoverable errors to Sentry. Keep tracing, replay, logs, and automatic
  breadcrumbs disabled. A base build does not initialize Sentry; enable a platform adapter only after
  a deliberate Windows, macOS, or iPadOS crash test proves that PostHog cannot provide the required
  dump and symbolication. This makes Sentry's 5,000-error free allowance a fatal-crash budget rather
  than a product-error budget.
- The client talks directly to an enabled Sentry native-crash adapter because a fatal process cannot
  depend on application code; Sentry's PII scrubbing must be enabled.

At the August 2026 public free-tier limits, PostHog's one million product-analytics events per month
should be ample for an initial TapConductor release; batching reduces requests but does not reduce
the number of billable events. The application must alert on 50%, 75%, and 90% of that allowance and
review sampling before upgrading. `app_error` aggregates count as ordinary product events. If
PostHog's `$exception` Error Tracking is enabled later, its separate free exception allowance must
also be monitored. Sentry's 5,000 errors per month is too small for routine handled faults but should
be adequate when restricted to fatal native crashes. No paid plan is expected initially; a paid plan
becomes necessary only if measured usage, retention, team-seat, or crash volume exceeds the free
allowances.

Do not put OpenTelemetry Collector or a general logging agent on customer devices. Internally model
events so an OTLP exporter can be added behind the typed telemetry API; OTLP is a useful vendor-neutral
transport, but its full SDK/collector stack is unnecessary for these low-volume product events.

The public PostHog project token is not an administrative secret and is intentionally embedded in
released clients. The typed client is the schema and data-minimization boundary: it creates only
allow-listed properties, enforces queue and batch limits, and posts directly to PostHog's `/batch/`
endpoint. PostHog receives the connection's source IP as part of direct HTTPS delivery and may use
it for approximate geographic enrichment. TapConductor does not add an IP property or request OS
location permission. PostHog project privacy, access, and retention controls therefore form part of
the production configuration.

### Consent and privacy behavior

Telemetry is enabled by default only after a clear first-run disclosure has been shown. The screen
explains what is collected, links to the privacy policy, and offers **Continue** and **Do not share**
with equal prominence. Until it is dismissed, the sink is a no-op and no install/launch event is
sent. Settings must always expose **Share usage and crash data**, the current state, a
plain-language data inventory, and **Reset telemetry identifier**.

If the user chooses **Continue** on the initial run, enqueue exactly one `app_installed` (or
`browser_instance_created`) event and exactly one `session_started` event for that same initial
launch. Schedule that first consented launch batch immediately rather than waiting for the normal
five-minute upload window. Persist their `event_id` values before transmission so retrying after a
network failure cannot create duplicate logical events. If the user chooses **Do not share**, send
neither event. Every later consented process launch likewise emits exactly one `session_started`.

The consent state is read synchronously before either analytics or crash SDK initialization:

- On opt-out, stop enqueueing immediately, shut down network exporters, and delete the unsent spool.
  Do not send an `opted_out` event. Any optional Sentry native-crash adapter must also close/disable.
- On later opt-in, create fresh installation and device-instance IDs; do not upload events from the
  opted-out period.
- Resetting the identifier purges the spool and rotates both IDs. Because there is no account, the
  app cannot reliably perform server-side deletion without keeping another identifying secret;
  document a support-assisted deletion route and retention period in the privacy policy.
- Make the compile-time default configurable by distribution/territory so a release can be opt-in
  where local law or store policy requires prior consent. The product preference may be opt-out, but
  it does not override applicable consent law.

This feature changes the shipped privacy posture. Before enabling it, update `PRIVACY.md`, the in-app
privacy copy, Apple's privacy manifest/App Store privacy answers, and Microsoft Store disclosures.
Apple classifies product interaction, device identifiers, crash data, performance data, and derived
coarse location as collected data even when the identifiers are pseudonymous. Do not describe this
as anonymous in legal copy; **pseudonymous usage and diagnostic data** is accurate.

Never collect a hardware serial, advertising ID, IDFV, MAC address, username, email address, exact
location, score/file name or path, title/composer text, score contents, MIDI messages, MIDI/audio
device names, or a hash that could identify a user's file. Never add the connection IP address to
the event payload. Region means country code or broader approximate region derived by PostHog from
the network connection, never GPS or OS location permission.

### Identity, time, and common envelope

Each accepted event uses random UUID values generated locally:

- `device_instance_id`: random pseudonymous app/device identifier stored in the WebView's
  origin-scoped application storage (the same mechanism is used by the browser build). It is not
  derived from hardware, an OS advertising identifier, or a login. Storage may be cleared at any
  time and is not guaranteed to survive uninstall. It is therefore a device-instance correlation ID,
  not proof of a person or physical machine. Without accounts, there is intentionally no
  cross-device `user_id`.
- `installation_id`: random ID created when this installation's application-data record is first
  created. It survives application updates and rotates on reset/reinstall when the OS removes the
  data container.
- `session_id`: random ID created for every document/process launch. Foreground/resume remains part
  of that launch session; an operating-system process restart creates a new one.
- `event_id`: unique ID used by PostHog for deduplication. `error_id` additionally links an
  error event to its Sentry issue/event where available.

Every event has this allow-listed envelope:

```text
event_name, schema_version, event_id, occurred_at_utc,
device_instance_id, installation_id, session_id,
app_version, build_number, release_channel,
os_family, os_version, cpu_arch, app_platform,
locale, telemetry_sdk_version
```

The client sets `occurred_at_utc`; durations use a monotonic clock and are not derived by subtracting
wall-clock timestamps. Approximate country/region is PostHog ingestion enrichment rather than a
client-supplied envelope field. Product dashboards should use only country/region granularity, not
city, coordinates, postal code, or a precise location. `locale` is useful for localization
prioritization but must not be repurposed as region.

An "install" cannot be observed at installer/store download time by application code. For native
packages, define `app_installed` as **first consented launch of a newly created application-data
record** and use Microsoft/Apple store analytics separately for downloads and installations that
never launch or opt out. The static browser edition has no install lifecycle; its equivalent is
`browser_instance_created`, meaning first consented use for that origin/browser storage. An update
emits `app_updated` once when `app_version` changes under an existing `installation_id`.

### Versioned event catalog

All settings properties are enums, booleans, bounded integers, or coarse numeric buckets. Additive
optional properties do not change `schema_version`; removal, renaming, or semantic changes do.

| Event | Event-specific properties |
| --- | --- |
| `app_installed` | `initial_app_version`, `distribution` |
| `browser_instance_created` | `initial_app_version`, `distribution` (`web_static`) |
| `app_updated` | `from_version`, `to_version` |
| `session_started` | `launch_kind` (`normal`, `file_association`, `recovery`), `previous_session_unclean` |
| `score_loaded` | `source_kind` (`bundled_demo`, `user_file`), `file_format` (`musicxml`, `mxl`, `midi`, `other_supported`), `duration_seconds`, `structural_duration_quarter_notes`, `duration_bucket`, `part_count_bucket`, `tap_event_count_bucket`, `load_duration_ms_bucket`, `warning_count_bucket`, `result` |
| `midi_settings_changed` | `input_enabled`, `output_enabled`, `input_connection` (`none`, `physical`, `virtual`, `unknown`), `output_connection`, `channel_filter_mode`, `velocity_curve`, `sustain_enabled`; never device name, manufacturer, MIDI payload, or note value |
| `audio_settings_changed` | `backend`, `output_kind` (`built_in`, `wired`, `wireless`, `virtual`, `unknown`), `sample_rate_hz`, `buffer_frames`, `channel_count`, `internal_audio_enabled`, `estimated_latency_ms_bucket`; never device name |
| `rhythm_settings_changed` | `performance_mode`, `beat_mode`, `legato_enabled`, `meter_family`, `subdivision`, `tempo_source`; do not report score-derived note/rhythm sequences |
| `roll_settings_changed` | `roll_enabled`, `roll_order`, `tap_spread_ms_bucket`, `chord_spread_ms_bucket`, `gate_policy` |
| `app_error` | `error_id`, stable `error_code`, `component`, `severity`, `handled`, `operation`, `fingerprint`, `occurrence_count`; no raw user text, paths, device names, score fragments, or unsanitized exception message |
| `app_crashed` | `error_id`, `crash_kind`, `component`, `signal_or_exception_class`, `last_checkpoint_age_bucket`, `sentry_event_id`; sent on next launch as a minimal recovery event if the fatal handler could not upload |
| `session_recovered` | `active_duration_seconds`, `wall_duration_seconds`, `tap_count`, `score_load_count`, `error_count`, `last_checkpoint_age_bucket`, `end_reason` (`unclean`); an unclean session is not automatically labeled a crash |
| `session_ended` | `end_reason`, `active_duration_seconds`, `wall_duration_seconds`, `tap_count`, `score_load_count`, `error_count` |

`duration_seconds` for a score is musical/playback duration when known; if the active mode has no
meaningful tempo, send `null` plus the structural `tap_event_count_bucket`. Keep the exact duration
only if it is already available from import; telemetry must never do extra score analysis on the
critical path. A set of stable warning/error codes is acceptable, but detailed parser messages are
not.

Settings events fire only after a committed user-visible change and should be debounced into one
snapshot per settings panel close or 2-second idle window. Do not send an event for every tap. The
session's tap count is an atomic counter reported only in the end or recovered-session aggregate. If
future product questions genuinely need per-tap timing, they require a new privacy review and
sampled/bucketed event rather than silently expanding this schema.

### Lifecycle accuracy and crash recovery

`session_ended` is best effort: operating systems do not guarantee code execution on process kill,
power loss, crash, or mobile suspension. On session start, atomically write a small local session
record marked `open`. Every 60 seconds the background worker updates that local record with cumulative
counters. This checkpoint is not a telemetry event and never causes a network request. A clean close
marks it `closed` after enqueueing `session_ended`. If the next launch finds an open record, it emits
`session_recovered` with the last checkpoint totals and `end_reason = unclean`; it emits
`app_crashed` only when a crash marker/report provides positive evidence. It must not invent a
precise close time after the last checkpoint.

Track both `wall_duration_seconds` and `active_duration_seconds`. Wall duration is monotonic time for
which the process/session remained open. Active duration accrues only while the app is foregrounded
and has not been idle for five minutes. A score load, tap, score navigation, settings interaction,
audition, keyboard/pointer input, or MIDI input ends the idle state. Entering the background begins
idle immediately. This activity detection updates local counters only: leaving the app open idle
must produce no PostHog heartbeat, event, polling request, or other network traffic.

Handled and recoverable failures—including malformed XML/MXL/MIDI, missing or incompatible hardware,
device initialization, and non-blocking application faults—go through `recordError`. The client
fingerprints them by stable error code, component, operation, and allow-listed categorical context;
it never captures the raw exception message. Repeats become one `app_error` with
`occurrence_count`, and at most 32 distinct fingerprints are retained per upload window. These error
summaries share the next ordinary five-minute PostHog request, so a widespread repeating problem
does not consume one remote error event per occurrence.

PostHog supplies querying, dashboards, release/version segmentation, and an optional Error Tracking
product. The aggregate `app_error` schema intentionally favors prevalence and hardware/format
correlation over stack traces. Sentry adds mature native minidumps, Windows PDB/macOS and iPadOS dSYM
symbolication, richer stack grouping, and crash-context inspection. Those extras matter for a fatal
native crash but do not justify sending routine handled errors twice.

For fatal-crash diagnostics:

- JavaScript uncaught exceptions and unhandled rejections become scrubbed `app_error` aggregates;
  raw URL, DOM text, path, score metadata, message, and stack are not sent in the base configuration.
- Rust uses stable error codes for handled errors. Because the current release profile uses
  `panic = "abort"`, a panic hook alone is insufficient; a Sentry-enabled desktop build needs a
  native minidump adapter and Apple builds need an Apple-native crash adapter.
- Keep only allow-listed breadcrumbs such as `score_load_started`, `audio_backend_initialized`, and
  stable settings enums. Never breadcrumb taps, pitches, filenames, device names, or exception text.
- Release CI uploads version-matched JavaScript source maps, Windows PDBs, and Apple dSYMs only when
  the optional adapter is enabled, and verifies symbolication with a deliberate crash on each target.

An enabled Sentry crash carries `device_instance_id`, `installation_id`, `session_id`, and
`app_version` as tags/context. PostHog receives a sanitized `app_crashed` summary with the Sentry
event ID on recovery, not the stack trace or minidump. The Sentry organization token is a privileged
CI/server credential and must never ship in the app; the crash client uses a project DSN.

### Performance, delivery, and failure rules

Telemetry must be impossible to observe from the performance path:

```text
shared UI lifecycle + settings + errors --> bounded local queue --> PostHog batch API
audio/MIDI/performance callbacks -------> normal product events only; no telemetry work
accepted tap at UI/core boundary -------> in-memory counter --> 60-second local checkpoint
optional fatal native crash adapter -----------------------------------------------> Sentry
```

- No network, serialization, allocation, locks, logging, filesystem access, or telemetry queue write
  occurs in the audio callback, native input callback, MIDI callback, or sampler scheduler.
- Non-real-time producers append to an origin-scoped bounded queue (maximum 500 events/1 MiB). On
  overflow the oldest non-lifecycle event is removed; telemetry never blocks product work.
- The telemetry coordinator uploads at most one
  ordinary batch every five minutes. The interval is a maximum transmission frequency, not a poll:
  when no events are pending it performs no DNS lookup or request. Installation/launch is one
  immediate batch after consent, and graceful close is one final best-effort batch; these lifecycle
  batches are the only normal exceptions to the five-minute limit. Never retry a rejected schema.
- All score-load and settings events accumulated during a five-minute window share one PostHog
  request. Event volume does not trigger an early upload; the bounded queue/spool applies backpressure
  by dropping the oldest non-lifecycle product event rather than increasing request frequency.
- Use the bounded origin-scoped application-data spool described above, with oldest-first eviction.
  Do not spool while opted out. Because it contains only minimized pseudonymous events and never
  score/device data, strict minimization and bounds are the primary controls; an OS-protected native
  spool can replace it later without changing instrumentation.
- Flush for at most 150 ms on graceful desktop close; do not delay a mobile suspend or application
  shutdown beyond the platform deadline. Remaining events stay in the spool for the next launch.
- Treat all telemetry failures as silent, non-user-facing failures. PostHog project ingestion
  controls provide an operational kill switch; disabling capture in a subsequent release provides
  the client-side kill switch. Telemetry startup never delays the product's Ready state.

Acceptance tests must prove that opt-out performs zero telemetry DNS/HTTP calls, wipes the spool,
and disables any optional Sentry adapter; the client strips or rejects forbidden fields; duplicate `event_id`
values are idempotent; clocks changing do not corrupt durations; offline queues remain bounded; a
stalled endpoint does not affect launch, score load, MIDI, or audio latency. Before an optional
Sentry adapter is enabled, its crash reports from Windows, macOS, and iPadOS must be symbolicated and
contain the correlation IDs but no sample user data.
They must also prove that an opted-in initial run produces one `session_started` plus one install or
browser-instance event, an opted-out initial run produces neither, an idle open session makes no
PostHog request, and ordinary uploads never occur more than once in a five-minute window.
Benchmark the 30-minute real-time stress test with telemetry on and off; the existing latency and
underrun acceptance thresholds must be identical.

Relevant upstream references: [PostHog capture/batch API](https://posthog.com/docs/api/capture),
[PostHog data-collection controls](https://posthog.com/docs/privacy/data-collection),
[Sentry Rust SDK](https://docs.rs/sentry/latest/sentry/),
[OpenTelemetry event conventions](https://opentelemetry.io/docs/specs/semconv/general/events/), and
[Apple App Privacy data definitions](https://developer.apple.com/app-store/app-privacy-details/).

## 7. Format support

### MusicXML MVP

Support partwise MusicXML and compressed MXL first. The import conformance corpus should cover:

- multiple parts/staves/voices and cross-staff simultaneity;
- chords encoded with `<chord/>`, `backup`, and `forward`;
- changing divisions, meters, tuplets, pickups, and mid-measure attributes;
- accidentals, octave shifts, transposing instruments, ties, and basic articulations;
- ordinary repeats and endings;
- empty measures, full-measure rests, hidden notes, and malformed timing.

Import should produce structured warnings with measure/part context. A partially supported score can
open only when unsupported constructs cannot change event ordering; otherwise fail closed rather than
perform the wrong notes live.

### MIDI file import

Standard MIDI Files contain timing and pitch but weak notation semantics. Phase 2 imports type 0 and
type 1 files, groups equal absolute ticks across tracks, ignores tempo for tap advancement, derives
note ends from Note Off, and generates a basic piano-roll-like staff score for display. It should be
described as usable but less faithfully engraved than MusicXML.

### PDF later

PDF needs optical music recognition and a correction workflow, not just a decoder. Treat it as a
separate product project: PDF/image -> OMR -> editable validation -> MusicXML -> existing importer.
Never send unverified OMR output straight into performance mode.

## 8. Delivery plan and gates

Assuming one experienced full-time engineer, a credible Windows beta is roughly 8-11 weeks. The
latency and score-corpus gates matter more than the calendar.

### Phase 0 — risk spikes (week 1)

- Scaffold the Rust workspace and a minimal Tauri Windows app.
- Keep a warmed sampler stream running; trigger a 1 kHz transient and a 10-note piano chord from
  keyboard, pointer, and a synthetic MIDI callback.
- Build loopback measurement tooling and report median/p95/p99 input-to-first-sample latency.
- Compare `cpal` WASAPI with a minimal direct `IAudioClient3` backend at supported periods.
- Benchmark RustySynth for allocation behavior, 128-voice rendering, cold/warm note-on CPU, and exact
  chord alignment.
- Render and interact with one representative MusicXML score through OSMD.

Gate: do not build product UI until the wired reference machine meets the latency targets and the
chosen synth completes the audio callback comfortably within 50% of its deadline. Replace the audio
backend or sampler at this point if needed.

### Phase 1 — score and performance core (weeks 2-4)

- Implement normalized score types, exact rational arithmetic, MusicXML/MXL import, source IDs,
  cross-part grouping, tie handling, and active-part filtering.
- Implement playback-order expansion for basic repeats/endings and structured import warnings.
- Build the cursor state machine, paired input-down/input-up handling, per-tap overlapping voice
  groups, the default 400 ms piano gate, audition/reposition/panic commands, and fixed velocity.
- Add unit/property tests and golden TapEvent JSON snapshots for a curated MusicXML corpus.

Gate: for every corpus score, the event count, pitches, ordering, grouping, tie behavior, and source
anchors match independently prepared expected data.

### Phase 2 — usable Windows vertical slice (weeks 4-7)

- Implement the file picker, score viewport, zoom/reflow, current/next highlights, active-part UI,
  audition controls, start-here controls, keyboard mapping, and performance tap surface.
- Connect source IDs to OSMD graphical notes and compute vertical slice and notehead overlays on a
  single horizontal system.
- Add the production WASAPI backend, device selection, warm-up/Ready state, recovery, volume, and
  diagnostics.
- Bundle a legally approved piano asset and add installer/third-party notices.

Gate: a musician can open representative choral, piano, and ensemble scores and perform end-to-end
without stuck notes, cursor drift, missed taps, or audio glitches.

### Phase 3 — live-use hardening and beta (weeks 7-11)

- Add large-score virtualization/caching, keyboard focus hardening, display scaling, autosave of
  preferences, crash reporting choice, and actionable error states.
- Test 30-minute runs under CPU, GPU, resize, scrolling, and device-change stress.
- Run latency tests on at least three Windows systems: built-in audio, USB class-compliant interface,
  and a known poor/high-period device. Publish supported settings and warnings.
- Conduct rehearsals with a pianist/director and singers; tune highlight timing, controls, gate
  policy, velocity curve, and recovery behavior.
- Package and sign an x64 Windows installer; add ARM64 once dependencies and hardware testing permit.

Gate: all latency, stability, import-correctness, accessibility, and rehearsal acceptance criteria
below pass.

### Phase 4 — MIDI IN/OUT (roughly 2-3 additional weeks)

- Device selection/hot-plug, channel/note filtering, velocity curves, debounce policy, sustain/panic,
  and saved mappings.
- MIDI OUT destination, channel/part routing, Note On/Off scheduling, internal-audio toggle, and stuck
  note recovery.
- Timestamped integration tests with a virtual loopback plus at least two physical controllers.
- Investigate Windows MIDI Services/MIDI 2.0 behind the existing backend abstraction.

### Phase 5 — modes and Apple ports

- Rolled-chord modes: bottom-up, top-down, outside-in, and configurable spread. A single TapEvent is
  scheduled at sample offsets; the live cursor still advances once.
- Beat-tap mode: construct a beat grid and release all score attacks between two tapped beats using
  a predictive scheduler. Keep it a separate strategy from note-rhythm mode.
- macOS port, then iPadOS/iOS: CoreAudio session/route handling, touch-first controls, sandboxed file
  import, background/interruption policy, App Store signing, and hardware/MIDI validation.
- PDF/OMR discovery only after the digital-score workflow is stable.

## 9. Acceptance criteria for the Windows beta

### Functional

- Opening valid MusicXML/MXL displays readable notation and reports unsupported constructs.
- Each tap plays exactly the non-rest, non-tied-continuation pitches at the next rational onset across
  all active parts and advances once.
- Chord notes begin on the same output sample.
- Each group stops at the later of the first subsequent tap and 400 ms after its triggering input is
  released. A subsequent tap inside the minimum starts immediately while the earlier group continues.
- Holding an input never causes a note-off, and the synth's normal piano decay is never looped,
  amplified, frozen, or time-stretched to fill the hold or minimum period.
- Play single chord and single-note selection sound without moving the cursor; Start here moves it
  predictably.
- Changing active parts rebuilds the sequence safely and never leaves a voice sounding.
- Keyboard and pointer controls work without focus surprises; Panic always works.

### Latency and resilience

- The phase-zero latency budgets pass on reference hardware and remain within the stress budget.
- No callback allocations/locks, queue overflow, underrun, or stuck note in the 30-minute test.
- Loading/reflowing a large score cannot block or starve audio.
- Audio device loss produces a visible fault and safe recovery, never silent cursor advancement.

### Quality and safety

- Automated tests cover musical grouping, cursor transitions, repeats, ties, overlapping per-tap
  voice groups, paired press/release events, the 400 ms gate boundary, device-state transitions,
  queue bounds, and stale-generation rejection.
- Fuzz MusicXML/MXL and MIDI parsers; cap decompressed size, note count, repeat expansion, and XML
  depth to prevent resource-exhaustion files.
- CI runs format/lint/tests, dependency license policy, vulnerability audit, SBOM generation, and
  Windows release build.
- All bundled fonts, notation assets, piano samples, and libraries appear in third-party notices and
  are cleared for commercial distribution.

## 10. Decisions to validate with musicians

These do not block the architecture, but should be tested before the beta behavior is frozen:

1. Whether tied notes should visually remain highlighted between intervening tap events.
2. Whether grace notes should be skipped, consume a tap, or attach as a short roll to their anchor.
3. How repeated passages should choose an occurrence when the performer clicks Start here.
4. Whether a mouse click anywhere on the score should perform the next event, or only the dedicated
   performance surface should do so to avoid accidental taps while navigating.
5. Whether transposing-score users want concert-pitch sound by default or an explicit per-part choice.

## 11. First engineering backlog

1. Add a Rust/Tauri workspace and Windows CI.
2. Add a synthetic latency harness before any score features.
3. Implement the `Sampler` and `AudioBackend` traits with fixed-capacity command types.
4. Spike RustySynth + `cpal`, then direct `IAudioClient3`; record the decision and measurements.
5. Define normalized score types and a ten-file conformance corpus with hand-authored expectations.
6. Implement MusicXML/MXL timing import and TapEvent grouping.
7. Implement the pure performance state machine and exhaustive transition tests.
8. Integrate OSMD and establish source-note-to-overlay mapping.
9. Connect end-to-end keyboard/pointer taps, highlight, audition, and reposition.
10. Add device diagnostics, a licensed piano asset, packaging, and rehearsal builds.

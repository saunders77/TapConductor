# TapConductor product and technical plan

Status: living product and architecture plan
Initial platform: Windows 10/11  
Planned platforms: macOS and iPadOS/iOS

## 1. Product definition

TapConductor turns a written score into a performer-controlled sequence of sounding note attacks.
The score supplies pitch and ordering; the performer supplies timing and, when MIDI input is used,
velocity.

The primary workflow is:

1. Open a MusicXML (`.musicxml`, `.xml`), compressed MusicXML (`.mxl`), MIDI, or—on Windows—a
   scanned PDF that is recognized and reviewed through Audiveris.
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

- General score editing, recording, tempo-following accompaniment, networking, VST/AU plug-ins,
  automatic correction of malformed scores, and image-level OMR correction inside TapConductor.
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
rewrite it. Local telemetry is opt-in and should contain timings/device classes, never score data.

## 6. Format support

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

### PDF optical music recognition (Windows)

PDF import uses the separately packaged Audiveris desktop application. TapConductor invokes the
pinned Audiveris build in batch mode (`-batch -transcribe -save -export`) and requires one editable
`.omr` project plus one preliminary `.mxl`. The user chooses the persistent `.mxl` location; the
same-named `.omr` is retained beside it because MusicXML export is lossy.

**Review recognition** launches that `.omr` in the native Audiveris editor. A private Audiveris
profile installs a **Send to TapConductor** export plugin. The plugin asks Audiveris to create an
up-to-date MusicXML export and invokes TapConductor's one-shot callback mode, which places the file
in a project-specific inbox. The running application validates the callback export, replaces the
user-selected `.mxl`, reparses it, and refreshes OSMD and the performance sequence.

Audiveris owns image-level recognition and correction. OSMD remains only a MusicXML rendering
layer. A future TapConductor editor may offer limited semantic corrections for performance-relevant
MusicXML mistakes, but it must not be presented as an OMR image editor. This flow is not supported
on iOS/iPadOS. See `docs/OMR_ARCHITECTURE.md` for the process and licensing boundaries.

## 7. Delivery plan and gates

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
- Keep Audiveris OMR desktop-only; Apple mobile imports remain MusicXML/MXL/MIDI only.

## 8. Acceptance criteria for the Windows beta

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

## 9. Decisions to validate with musicians

These do not block the architecture, but should be tested before the beta behavior is frozen:

1. Whether tied notes should visually remain highlighted between intervening tap events.
2. Whether grace notes should be skipped, consume a tap, or attach as a short roll to their anchor.
3. How repeated passages should choose an occurrence when the performer clicks Start here.
4. Whether a mouse click anywhere on the score should perform the next event, or only the dedicated
   performance surface should do so to avoid accidental taps while navigating.
5. Whether transposing-score users want concert-pitch sound by default or an explicit per-part choice.

## 10. First engineering backlog

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

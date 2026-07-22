# TapConductor

TapConductor is a Windows desktop score-performance app. It supplies the pitches from a written
score while the performer supplies the timing: each keyboard, pointer, or MIDI-key press sounds the
next set of notes that begin at exactly the same score position, including notes across staves and
parts. Rests do not consume taps.

This repository contains an MVP foundation spanning phases 0–4: score import and display,
deterministic conducting, low-latency built-in audio, score navigation/audition, and MIDI
input/output. It is not yet a release-qualified completion of every phase gate; hardware loopback,
long-duration stress/rehearsal, signing, ASIO hardware qualification, and physical-controller coverage remain explicit
validation work.

## MVP capabilities

- Opens partwise MusicXML (`.musicxml`/`.xml`), compressed MusicXML (`.mxl`), and Standard MIDI
  Files (`.mid`/`.midi`). MusicXML is engraved with OpenSheetMusicDisplay; MIDI uses a compact event
  view because MIDI files do not contain complete engraving information.
- Groups simultaneous pitched note attacks by exact rational score time, even across enabled parts.
  Rests and tied continuations do not create extra conducting events.
- Conducts from Space, Enter, the large pointer target, or MIDI Note On. MIDI input
  velocity controls the whole sounded note/chord; the MIDI key's pitch is intentionally ignored.
- Highlights the most recently played slice separately from the next live cursor.
- Engraves MusicXML as one long horizontal system with a persistent horizontal scrollbar. During
  playback, the view follows the slice by whole-measure context rather than continually chasing it.
- Shows permanent ear and down-arrow controls at every slice: the ear plays that single chord
  without moving the live cursor, while the arrow moves the live cursor there. Selecting an
  individual notehead plays only that note, even while other sounds are active.
- Selects score parts, audio output, MIDI input, and MIDI output at runtime. **Panic** immediately
  clears sounding groups and MIDI output notes.
- Uses a bounded, allocation-free audio callback path, sample-clock scheduling, a fixed voice pool,
  and an SPSC command queue. Installed native ASIO drivers are exposed as low-latency outputs;
  ordinary Windows endpoints use the direct event-driven `IAudioClient3` path.

The default instrument is the 44.1 kHz, 16-bit Slender Salamander Grand Piano: Signal Experiments'
phase-aligned derivative of Alexander Holm's Salamander Grand Piano V3. Its three sampled velocity
layers are crossfaded into a continuous response. The 90 note samples are loaded once as compact
16-bit PCM and reused across audio-device changes; playback performs no allocation, decoding,
locking, or file I/O in the audio callback. The installed library occupies about 250 MB and uses
about 203 MiB for note PCM while the app is running. A small procedural piano remains available
automatically if the sampled asset is missing or invalid.

## Piano gate behavior

Every physical press creates an independent voice group and sounds immediately. For that group,
note-off occurs at:

```text
max(first later tap, matching input release + 100 ms)
```

Thus a quick next tap starts on time while the earlier sound may continue until its 100 ms
post-release minimum. Several groups can overlap during fast passages. If no later tap has happened,
the group remains eligible until one does. The synthesizer's piano envelope always decays naturally
while a key is held; the gate never freezes or lengthens that decay.

## Windows prerequisites

- 64-bit Windows 10 or Windows 11.
- [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with
  **Desktop development with C++**, an MSVC x64 toolset, and a Windows 10 or 11 SDK.
- [LLVM](https://releases.llvm.org/) with `libclang` available to the build. The ASIO binding
  generator normally finds a standard LLVM installation; otherwise set `LIBCLANG_PATH` to its
  `bin` directory.
- [Rust](https://rustup.rs/) stable with the `x86_64-pc-windows-msvc` target.
- Node.js 22.12 or newer and npm (the current Node LTS is recommended).
- Microsoft Edge WebView2 Runtime. It is normally already present on supported Windows systems.
- A wired audio device is strongly recommended for live use; Bluetooth adds latency outside the
  app's control.

Install/select the Rust toolchain from PowerShell if needed:

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc --profile minimal
rustup default stable-x86_64-pc-windows-msvc
```

## Run in development

From the repository root:

```powershell
npm ci
npm run tauri:dev
```

Use **Open score** and try [`examples/scores/cross-staff-demo.musicxml`](examples/scores/cross-staff-demo.musicxml).
Select a low-latency wired output in the top bar, then use Space or Enter to conduct. Key repeat is
ignored; release events are paired with their presses for correct gating.

ASIO entries are suffixed **(ASIO)** in the output selector. Choose the vendor driver for an audio
interface such as the Roland QUAD-CAPTURE; TapConductor opens its main output pair at the driver's
native sample rate and minimum reported buffer. Increase the buffer in the vendor control panel if
the minimum clicks or drops out. Built-in devices such as Realtek remain available through the
low-latency WASAPI path unless an installed ASIO driver exposes them. ASIO is a host API, not a
universal replacement driver, so TapConductor cannot create native ASIO support for hardware whose
manufacturer supplies none.

To exercise MIDI, connect the device before opening its selector, choose it under **MIDI in** or
**MIDI out**, and use **Panic** before disconnecting or changing a live routing setup.

## Build installers

```powershell
npm ci
npm run tauri:build
```

Release artifacts are written beneath `target\release\bundle\`, including NSIS and MSI packages.
Rust and JavaScript dependencies are pinned by `Cargo.lock` and `package-lock.json`; use `npm ci`
and Cargo's `--locked` flag in automated builds. The `asio-sys` build currently downloads
Steinberg's SDK from its official stable URL, so release engineering must archive the exact SDK and
license materials used for each build.
Local development bundles are unsigned; production distribution still requires code signing.

## Test and measure

```powershell
npm run build
cargo test --locked --workspace
cargo check --locked --workspace --all-targets
cargo run --locked --release -p tapconductor-latency-probe
cargo run --locked --release -p tapconductor-latency-probe -- --live
cargo run --locked --release -p tapconductor-latency-probe -- --live --device "Realtek"
cargo run --locked --release -p tapconductor-latency-probe -- --live --device "QUAD-CAPTURE (ASIO)"
```

The latency probe reports median/p95/p99 software command-to-first-render timings over 2,000
iterations, along with queue and late-command counters. It deliberately does **not** claim physical
input-to-speaker latency: validate that separately with loopback hardware on each performance
machine, audio interface, driver, and buffer configuration. The in-app diagnostics panel reports
the active backend/device, sample rate, callback buffer size, estimated output latency, queue
overflows, backend errors, active voices, and MIDI routing.

The latest checked-in measurement and its limits are recorded in
[`docs/LATENCY_REPORT.md`](docs/LATENCY_REPORT.md).
The verified Windows artifact paths, hashes, and test record are in
[`docs/BUILD_REPORT.md`](docs/BUILD_REPORT.md).

## Repository layout

| Path | Responsibility |
| --- | --- |
| `src/` | Tauri WebView UI and OSMD score rendering |
| `src-tauri/` | Windows application adapter, device lifecycle, and IPC |
| `crates/tapconductor-score/` | MusicXML/MXL/MIDI import and normalized score events |
| `crates/tapconductor-performance/` | Cursor, gesture, generation, and piano-gate state machine |
| `crates/tapconductor-audio/` | Real-time command queue, scheduler, diagnostics, and synth |
| `crates/tapconductor-midi/` | MIDI message mapping, ports, and overlapping-note tracking |
| `tools/latency-probe/` | Repeatable offline command-to-render benchmark |
| `docs/PRODUCT_AND_TECHNICAL_PLAN.md` | Product semantics, budgets, phases, and acceptance criteria |

The score, performance, audio, and MIDI crates are UI-independent Rust libraries. That separation
keeps a future macOS/iPadOS host possible without moving correctness-sensitive logic into a WebView.

## MVP boundaries

- The importer intentionally supports a practical, tested subset of MusicXML rather than every
  publisher extension. Inspect import warnings before relying on an unfamiliar score live.
- Native ASIO uses the driver's current sample rate, main one or two output channels, native sample
  representation, and minimum reported buffer. Other Windows devices use the direct event-driven
  shared `IAudioClient3` renderer. End-to-end latency remains dependent on the hardware and driver.
- Salamander's 44.1 kHz source samples are linearly resampled when an output device operates at a
  different native rate. The procedural synth is a recovery instrument rather than the default.
- MIDI output currently uses the MVP routing/channel behavior rather than per-part routing.
- PDF/optical recognition, rolled-chord modes, beat-tap mode, and Apple hosts are future work.
- The software latency harness is included, but release qualification still requires end-to-end
  loopback and sustained-load testing on representative Windows audio hardware.

See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for direct dependency licenses. TapConductor
source and distributed binaries are licensed under the [GNU GPL version 3 only](LICENSE).

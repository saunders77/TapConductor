# Latency validation

Last run: 2026-07-20, Windows x64, release build

## Native command-to-render probe

The checked-in `tapconductor-latency-probe` measures the allocation-free path from enqueueing a
ten-note chord through rendering the first audio block. It does not include keyboard/MIDI hardware,
WebView IPC, the Windows audio driver, DAC, speaker, microphone, or acoustic loopback delay.

Run it from a Visual Studio Developer PowerShell with:

```powershell
cargo run --offline --release -p tapconductor-latency-probe
```

Result from 2,000 warmed iterations at 48 kHz with a 128-frame stereo block:

| Measurement | Result |
| --- | ---: |
| First audible frame | 0 |
| p50 command-to-render | 0.0196 ms |
| p95 command-to-render | 0.0407 ms |
| p99 command-to-render | 0.0739 ms |
| Queue overflows | 0 |
| Late commands | 0 |

All notes in a `PlaySlice` command are started at the same sample offset, so measured chord onset
spread inside the engine is zero samples.

## Windows output status

The default Windows endpoint now uses a direct event-driven `IAudioClient3` renderer. It requests a
legal minimum shared-mode period and renders on an MMCSS **Pro Audio** thread. Explicitly selected
endpoints now use the same direct path. If a device cannot join the current `IAudioClient3` engine
period, TapConductor retries with a fresh event-driven compatibility client. Device switching first
releases the old stream so a single-client driver cannot be locked by TapConductor itself.

The former **Exclusive low latency** WASAPI control has been removed. TapConductor now enumerates
installed native ASIO drivers alongside ordinary Windows outputs. An ASIO selection opens the
driver's main output pair at its current sample rate and minimum reported buffer; a non-ASIO
selection continues to use the direct event-driven shared WASAPI path.

A read-only native query on 2026-07-21 successfully loaded the installed `QUAD-CAPTURE` ASIO
driver and negotiated 44.1 kHz, two channels, signed 32-bit samples, and a 256-frame buffer
(5.805 ms per callback). Playback, driver-to-DAC delay, and acoustic latency still require the
hardware validation below.

### Live profile on the current Windows system

Profile date: 2026-07-20. Default endpoint: `1-2 (QUAD-CAPTURE)`, 44.1 kHz stereo.

| Stage | Observed result |
| --- | ---: |
| Browser pointer handler | synchronous dispatch; no intentional delay |
| UI-to-native command | now measured in-app as a round-trip upper bound |
| Native command to first rendered sample | 1.988–5.142 ms in live trials; latest 2.977 ms |
| Windows-reported stream latency | 11.995 ms |
| Best observed native-to-endpoint estimate | 13.983 ms; latest 14.972 ms |
| Endpoint engine period | 441 frames / 10.000 ms |
| Late commands / queue overflows | 0 / 0 |
| Processing mode | normal low-latency shared mode |

The measured software and Windows-exposed path therefore accounts for roughly 14-17 ms, not the
approximately 180 ms acoustic delay reported by the user. About 163-166 ms remains after the point
that WASAPI reports as delivery to the endpoint. That remainder cannot be timed electrically from
software alone. Ableton's local preferences confirm that its low-latency comparison uses the native
`QUAD-CAPTURE` ASIO driver, whereas this historical measurement used the Windows WASAPI path.
ASIO's different driver path is therefore the leading explanation for the acoustic difference.

Native ASIO support is implemented through CPAL and Steinberg's open SDK. TapConductor elects the
SDK's GPLv3 route and the Windows release is licensed GPLv3-only. Non-Windows releases use MIT. The ASIO path still requires physical
loopback and sustained-load qualification on each driver and buffer setting before release claims
can be made.

TapConductor diagnostics now show the last UI-to-native round trip, a conservative UI-to-endpoint
bound, the direct-backend state, and a QUAD-CAPTURE-specific reminder. Roland's own driver help says
that its default Audio Buffer Size is the sixth slider position and that lowering it shortens latency.
For this interface, close programs using the device, open **QUAD-CAPTURE Control Panel → Driver →
Driver Settings**, move **Audio Buffer Size** toward the minimum, apply it, and then reopen
TapConductor. Increase it one step only if clicks or dropouts occur.

Run the reusable live probe (it plays one short note) with:

```powershell
cargo run --locked --release -p tapconductor-latency-probe -- --live
cargo run --locked --release -p tapconductor-latency-probe -- --live --device "QUAD-CAPTURE (ASIO)"
```

## Required hardware validation

Before calling a machine/audio configuration performance-qualified, run electrical or acoustic
loopback tests for keyboard, pointer, and MIDI input. Verify median input-to-first-sample latency of
at most 8 ms and p99 of at most 12 ms on wired/built-in output, plus a 30-minute underrun and stuck-
note stress run. These measurements are device- and driver-dependent and cannot be inferred from the
offline engine result above.

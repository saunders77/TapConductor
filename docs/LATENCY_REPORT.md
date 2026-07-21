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

The MVP queries the default endpoint's shared-mode engine periods through `IAudioClient3`, then uses
the smallest compatible period through CPAL's WASAPI backend. Diagnostics identify this honestly as
a CPAL/WASAPI fallback; the direct event-driven `IAudioClient3` renderer described in the product
plan has not yet replaced it.

## Required hardware validation

Before calling a machine/audio configuration performance-qualified, run electrical or acoustic
loopback tests for keyboard, pointer, and MIDI input. Verify median input-to-first-sample latency of
at most 8 ms and p99 of at most 12 ms on wired/built-in output, plus a 30-minute underrun and stuck-
note stress run. These measurements are device- and driver-dependent and cannot be inferred from the
offline engine result above.

# Windows MVP build report

Build date: 2026-07-21
Target: Windows x64 (`x86_64-pc-windows-msvc`)
Version: 0.1.0

## Verification

- `npm run build`: passed (TypeScript check and Vite production bundle).
- `cargo fmt --all -- --check`: passed.
- Native `cargo test --locked --workspace`: 75 tests passed with the Steinberg ASIO SDK compiled.
- Native `cargo check --locked --workspace` and
  `cargo clippy --locked --workspace --all-targets --all-features -D warnings`: passed. Windows CI
  now installs the required LLVM/libclang binding generator explicitly.
- Native read-only `QUAD-CAPTURE (ASIO)` query: 44.1 kHz, stereo, signed 32-bit output, 256-frame
  driver buffer.
- `cargo run --offline --locked --release -p tapconductor-latency-probe`: frame-0 onset,
  0.0739 ms p99, zero queue overflows, zero late commands.
- Live direct-WASAPI probe on `1-2 (QUAD-CAPTURE)`: 2.977 ms native command-to-render,
  11.995 ms Windows-reported stream latency, 14.972 ms native-to-endpoint estimate, zero queue
  overflows, and zero late commands.
- Endpoint-native 10 ms periods are accepted at both 44.1 kHz (441 frames) and 48 kHz
  (480 frames); buffer size remains visible in diagnostics rather than blocking playback.
- The exclusive WASAPI control and IPC path have been removed. Native ASIO device enumeration,
  format negotiation, minimum-buffer selection, real-time-safe float/integer conversion, and stream
  ownership are compiled into the Windows application.
- ASIO hardware playback and acoustic latency have not yet been re-measured. The historical WASAPI
  figures below remain useful as a comparison baseline only.
- Post-bundle GUI launch was not repeated because the user's installed TapConductor and Ableton
  audio sessions remained open; the same release code passed the direct hardware probe before
  packaging.

## Previous artifacts (superseded)

These hashes identify the pre-ASIO MIT build and must not be distributed as the current GPLv3 ASIO
version. Produce fresh signed artifacts after native ASIO CI and hardware qualification pass.

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `target/release/tapconductor-app.exe` | 5,404,160 bytes | `2A59538873CF2584F82E7F01651A146DB83FAE6C111E3853312C351ADC51C56F` |
| `target/release/bundle/nsis/TapConductor_0.1.0_x64-setup.exe` | 1,904,231 bytes | `9FA8D1189BAF9441F4E91E3B5AD310DC69D19E88857645E750592929E1EFDA8D` |
| `target/release/bundle/msi/TapConductor_0.1.0_x64_en-US.msi` | 2,678,784 bytes | `DBA870A2C441AE694261C2F36238903AA46738C67F36E32C788606B3C9294732` |

The installers are local development builds and are not code-signed.

## Qualification boundary

This confirms software compilation, deterministic core behavior, package generation, and basic
process startup on the build machine. It does not replace electrical/acoustic loopback latency,
physical MIDI-controller coverage, long-duration stress/rehearsal, installer upgrade/uninstall
matrix testing, or code-signing validation. See `LATENCY_REPORT.md` and the README for details.

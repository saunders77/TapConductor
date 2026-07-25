# Cross-platform build report

## 2026-07-24 cross-platform implementation

The Windows Store test lane now produces an offline, current-user NSIS installer containing the
same score import, performance, MIDI, and sampled-piano implementation as the desktop application.
It is intentionally unsigned and is only for local sideload testing:

| Artifact | Size | SHA-256 | Signing |
| --- | ---: | --- | --- |
| `target/store-artifacts/test-unsigned/TapConductor_0.1.0_x64_TEST-ONLY-UNSIGNED_store-setup.exe` | 324,194,121 bytes | `A007F169AD7124B592EAF80F3B14C9D280D6A07B4FA175EDBFE27E5EA54A544D` | Not signed; test only |

The staged directory also contains `LICENSE`, `PRIVACY.md`, `THIRD_PARTY_NOTICES.md`,
`SHA256SUMS.txt`, and `DO_NOT_SUBMIT_TO_STORE.txt`.

Verification completed from the current source:

- TypeScript production build passed.
- Both frontend suites passed: 5 tests total.
- Locked Rust workspace tests passed: 84 tests, with the large piano-bank test skipped in the
  normal pass.
- The separately invoked full Salamander bank test passed after loading and rendering the bundled
  assets.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` passed.
- The generated installer passed a silent current-user sideload and uninstall cycle in a temporary
  workspace directory.
- The installed payload contained 255 piano files totalling 250,243,209 bytes and the bundled demo
  score.
- The installed executable remained running through the five-second launch smoke interval.

The Apple source, privacy manifest, platform configuration, native iOS audio-session bridge,
packaging script, and macOS CI workflow are present. Apple `.app`, `.dmg`, `.pkg`, simulator `.app`,
and device `.ipa` outputs cannot be compiled on this Windows host. They must be produced on macOS
with Xcode; device/App Store outputs additionally require the project's Apple team, certificates,
and provisioning profiles. See `CROSS_PLATFORM_IMPLEMENTATION.md` for the exact commands and
qualification matrix.

## 2026-07-21 Windows MVP

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

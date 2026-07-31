# Cross-platform build report

## 2026-07-30 macOS and iPad completion

The checked-in Apple implementation was built and exercised on macOS 26.6 with Xcode 26.6 and the
iOS 26.5 Simulator SDK.

| Artifact | Size | SHA-256 | Signing |
| --- | ---: | --- | --- |
| `target/apple-artifacts/macos-ad-hoc/TapConductor_0.1.0_universal.dmg` | 164,687,400 bytes | `0D9BDA8109CF9173B674395A450104C920EF91981A314DC0DAED3E46A7A02987` | Ad hoc; local testing |
| `target/apple-artifacts/macos-ad-hoc/TapConductor.app` | 250 MiB on disk | See staged `SHA256SUMS.txt` | Ad hoc; local testing |
| `src-tauri/gen/apple/build/arm64-sim/TapConductor.app` | 345 MiB on disk | Generated simulator bundle | Ad hoc; simulator only |

Verification completed from the current source:

- The macOS release command produced a universal `x86_64`/`arm64` app and DMG. Strict deep
  `codesign` verification and the DMG container checksum passed, and all 265 files in the staged
  checksum manifest verified.
- The packaged macOS app remained running through a ten-second launch smoke interval.
- The iPad command produced an arm64 simulator app with a 16.0 deployment floor, iPad device-family
  support, the privacy manifest, demo score, and all 251 WAV samples plus four SFZ definitions.
- The iPad binary contains the native Swift `AVAudioSession` plugin and its activate/deactivate
  bridge. The fresh bundle installed on an iPad (A16) simulator, rendered the full interface, and
  remained running through a twelve-second launch smoke interval.
- The TypeScript production build and all three frontend suites passed: 11 tests total.
- Locked Rust workspace tests passed: 91 tests, with the 238 MB full-bank load test intentionally
  ignored in the normal pass.
- `cargo fmt --all -- --check` and
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` passed.
- Repeated Mac and iPad packaging passes now replace their prior generated outputs cleanly.

No Apple code-signing identity is installed on this machine. Developer ID notarization, Mac App
Store packaging, physical-iPad installation, TestFlight, and App Store export therefore remain
release-owner operations requiring the Apple team credentials, certificates, and provisioning
profiles described in `CROSS_PLATFORM_IMPLEMENTATION.md`.

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

At the time of this Windows-hosted pass, the Apple source, privacy manifest, platform configuration,
native iOS audio-session bridge, packaging script, and macOS CI workflow were present but could not
be compiled there. The macOS and simulator build gap has since been closed by the July 30
verification above. Device and App Store outputs still require the project's Apple team,
certificates, and provisioning profiles. See `CROSS_PLATFORM_IMPLEMENTATION.md` for the exact
commands and qualification matrix.

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

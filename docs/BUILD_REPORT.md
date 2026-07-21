# Windows MVP build report

Build date: 2026-07-20  
Target: Windows x64 (`x86_64-pc-windows-msvc`)  
Version: 0.1.0

## Verification

- `npm run build`: passed (TypeScript check and Vite production bundle).
- `cargo fmt --all -- --check`: passed.
- `cargo test --offline --locked --workspace`: 71 tests passed.
- `cargo clippy --offline --locked --workspace --all-targets --all-features -- -D warnings`:
  passed.
- `cargo run --offline --locked --release -p tapconductor-latency-probe`: frame-0 onset,
  0.0739 ms p99, zero queue overflows, zero late commands.
- Final post-bundle executable launch: remained healthy for the five-second smoke window and was
  then intentionally stopped.

## Artifacts

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `target/release/tapconductor-app.exe` | 5,411,840 bytes | `240FD9FBFD3640184DB3AA59AE63CF754C1C66CE3FFBC1A5E1DE1E3E49CBA4C3` |
| `target/release/bundle/nsis/TapConductor_0.1.0_x64-setup.exe` | 1,902,079 bytes | `4E7A2A877EF5265F7E8F72F505BDD531D96C15F1CDDDB8F4F2A34679424DFB7C` |
| `target/release/bundle/msi/TapConductor_0.1.0_x64_en-US.msi` | 2,682,880 bytes | `431001BB75BE2724BEB470EDEB653B6BE986C011AAE50B3EF09CD075C99D4001` |

The installers are local development builds and are not code-signed.

## Qualification boundary

This confirms software compilation, deterministic core behavior, package generation, and basic
process startup on the build machine. It does not replace electrical/acoustic loopback latency,
physical MIDI-controller coverage, long-duration stress/rehearsal, installer upgrade/uninstall
matrix testing, or code-signing validation. See `LATENCY_REPORT.md` and the README for details.

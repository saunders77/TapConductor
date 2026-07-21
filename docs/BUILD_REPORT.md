# Windows MVP build report

Build date: 2026-07-20  
Target: Windows x64 (`x86_64-pc-windows-msvc`)  
Version: 0.1.0

## Verification

- `npm run build`: passed (TypeScript check and Vite production bundle).
- `cargo fmt --all -- --check`: passed.
- `cargo test --locked --workspace`: 75 tests passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`:
  passed.
- `cargo run --offline --locked --release -p tapconductor-latency-probe`: frame-0 onset,
  0.0739 ms p99, zero queue overflows, zero late commands.
- Final post-bundle executable launch: remained healthy for the five-second smoke window and was
  then intentionally stopped.

## Artifacts

| Artifact | Size | SHA-256 |
| --- | ---: | --- |
| `target/release/tapconductor-app.exe` | 5,419,520 bytes | `EB5FD16DFE17AA77F157EC6D4489233915DEA3964FF102AD69CFDB4AAC01F444` |
| `target/release/bundle/nsis/TapConductor_0.1.0_x64-setup.exe` | 1,905,957 bytes | `DE965713465B3C017A9043D154513B2689FAEE0AF1E7EF2DC7AF70D7EAC56556` |
| `target/release/bundle/msi/TapConductor_0.1.0_x64_en-US.msi` | 2,686,976 bytes | `AD490087F2DA35DD4611F8CA61345F6751106C30A7B7F57A5CAC7FFDA6FD95B0` |

The installers are local development builds and are not code-signed.

## Qualification boundary

This confirms software compilation, deterministic core behavior, package generation, and basic
process startup on the build machine. It does not replace electrical/acoustic loopback latency,
physical MIDI-controller coverage, long-duration stress/rehearsal, installer upgrade/uninstall
matrix testing, or code-signing validation. See `LATENCY_REPORT.md` and the README for details.

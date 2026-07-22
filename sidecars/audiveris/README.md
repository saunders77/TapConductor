# Audiveris bundle staging area

Windows release builds stage the pinned Audiveris application image here. The generated `runtime/`,
`profile/`, `source/`, and `BUNDLE-MANIFEST.json` paths are intentionally ignored because they are
large release inputs, not TapConductor source.

Run `tools/stage-audiveris.ps1` with the official Audiveris 5.11.0 MSI, vetted Tesseract language
data, and the exact 5.11.0 corresponding-source archive. `npm run tauri:build` refuses to build an
installer until `tools/verify-audiveris-bundle.ps1` confirms that the private Java runtime, OCR
resources, and corresponding source are present.

# TapConductor cross-platform implementation and release plan

Status date: July 30, 2026

This document is both the implementation plan and the release runbook. The shared-source engineering
and build definitions described below are present in the repository. Production signatures,
notarization, device provisioning, Store uploads, and physical-hardware acceptance remain release
operations because they require accounts, credentials, hardware, and legal decisions that are not
present in source control.

## Outcome and constraints

TapConductor keeps the current Windows program and its architecture intact. macOS and iPadOS are
added beside it. One TypeScript UI and the existing Rust score, performance, audio-engine, sampler,
and MIDI crates are the product source for every platform.

| Platform/channel | Package | Build owner |
| --- | --- | --- |
| Existing Windows distribution | x64 NSIS EXE and MSI | Windows build |
| Microsoft Store testing | clearly marked unsigned offline x64 NSIS EXE | Windows build |
| Microsoft Store production | trusted Authenticode-signed offline x64 NSIS EXE at an immutable HTTPS URL | Windows release owner |
| macOS local testing | universal ad-hoc-signed `.app` and `.dmg` | macOS/Xcode build |
| macOS direct distribution | Developer ID-signed, hardened, notarized, stapled universal `.app` and `.dmg` | macOS release owner |
| Mac App Store | sandboxed universal `.app` and signed `.pkg` | macOS release owner |
| iPad simulator | simulator `.app` | macOS/Xcode build |
| Registered iPad testing | development/ad-hoc `.ipa`, `.xcarchive`, and dSYMs | Apple team |
| TestFlight/App Store | App Store Connect `.ipa`, archive, dSYMs, and thinning report | Apple team |

The universal macOS test packages and iPad simulator application have now been compiled and
smoke-tested on macOS 26 with Xcode 26. Apple requires Xcode signing for device and production
packages. The checked-in Apple workflow uses the same macOS 26/Xcode 26 combination.

## Additive architecture

```text
                         shared TypeScript/Vite/OSMD UI
                                      |
                                Tauri command DTOs
                                      |
                                  shared AppCore
                    +-----------------+-----------------+
                    |                 |                 |
             score import       performance state   MIDI mapping
         MusicXML/MXL/MIDI      tap/beat/audition    sustain/output
                    |                 |                 |
                    +----------- audio commands --------+
                                      |
                      lock-free scheduler + sampler
                                      |
                  +-------------------+-------------------+
                  |                                       |
        existing Windows host                       Apple hosts
      WASAPI/ASIO + WinMM MIDI          macOS: CPAL/CoreAudio + CoreMIDI
      NSIS/MSI/Store NSIS               iPad: RemoteIO + AVAudioSession
                                                   + CoreMIDI
```

The Windows host, direct `IAudioClient3` backend, ASIO owner thread, MIDI behavior, and existing
NSIS/MSI configuration are not replaced. Target-specific Cargo dependencies prevent Windows-only
audio features from entering Apple builds.

### Technology choices

- UI and engraving: existing TypeScript, DOM/CSS, Vite, and OpenSheetMusicDisplay in Tauri's
  WebView2/WKWebView.
- Shared product logic: existing Rust workspace crates.
- Windows audio: unchanged WASAPI/ASIO implementation.
- macOS audio: the existing `AudioBackend` trait with CPAL's CoreAudio host.
- iPad audio: the same CPAL callback/sampler path, with fixed buffer selection disabled on iOS and
  a small native Swift plugin configuring `AVAudioSession` for foreground playback, a preferred
  48 kHz rate and 256-frame duration, route changes, interruptions, and media-service resets.
- MIDI: existing `midir` integration, backed by CoreMIDI on Apple.
- Files: native Tauri open dialog. iPad uses document-picker copy mode, then Rust accepts either a
  native path or `file://` URL through `FilePath`.
- Windows Store: Microsoft's supported unpackaged Win32 EXE route. It preserves the application and
  uses an offline NSIS installer instead of imposing an MSIX rewrite.
- Apple packages: Tauri/Xcode `.app`, `.dmg`, `.pkg`, archives, and `.ipa` outputs.

## Implemented feature parity

The shared build contains:

- MusicXML, compressed MusicXML, and type 0/1 MIDI import.
- Exact rational event grouping across parts, staves, and voices.
- Rhythm Tap and Beat Tap/count-in modes.
- Pointer/touch, keyboard, and MIDI conducting with velocity.
- Rolled score chords and independently configured rolled auditions.
- Whole-chord, staff-chord, and single-note audition.
- Active-part selection, Start Here, previous/next navigation, replay, and Panic.
- Audio output, volume, MIDI input/output, direct MIDI free play, and diagnostics.
- The locally bundled Slender Salamander sampled grand piano and procedural fallback.
- A bundled demo score so reviewers can exercise the workflow without external files or hardware.
- In-app help, privacy, and piano/open-source acknowledgements.

The piano directory is about 250 MB and the sampler decodes about 203 MiB. It is bundled rather than
downloaded, preserving offline behavior and matching every platform. Release acceptance must measure
startup, memory pressure, and sustained playback on the oldest supported iPad. Loading and decoding
remain outside the realtime callback.

## Source and configuration map

- `src-tauri/tauri.conf.json`: existing Windows configuration plus shared resources.
- `src-tauri/tauri.microsoftstore.conf.json`: offline NSIS Store flavor.
- `src-tauri/tauri.macos.conf.json`: macOS app/DMG resources and Info.plist.
- `src-tauri/tauri.ios.conf.json`: iPad layout, resources, icons, OS floor, and Info.plist.
- `src-tauri/tauri.appstore.conf.json`: Mac App Store flavor.
- `src-tauri/apple/`: document types, privacy manifest, and sandbox-entitlement template.
- `src-tauri/gen/schemas/`: generated Windows, macOS, desktop, iOS, and mobile capability schemas.
- `crates/tauri-plugin-apple-audio-session/`: native Swift AVAudioSession integration.
- `tools/windows-store.ps1`: Store validation, unsigned test build, signing, and hashes.
- `tools/apple-release.sh`: Apple validation and package channels.
- `.github/workflows/apple.yml`: reproducible Mac and iPad simulator artifacts.
- `PRIVACY.md`: source for the required hosted privacy URL.

## Build and sideload runbook

### Rebuild both Apple test packages

After making source changes on a configured Mac, run:

```bash
cd /path/to/TapConductor
bash tools/rebuild-apple.sh
```

This runs the shared quality checks once, builds the universal macOS app and DMG, and then builds
the iPad Simulator app. Use `mac` or `ipad` to build only one platform:

```bash
bash tools/rebuild-apple.sh mac
bash tools/rebuild-apple.sh ipad
```

For a quick packaging iteration after the checks have already passed, append `--fast`. The normal
pre-commit or handoff build should omit `--fast`.

### Windows and Microsoft Store

```powershell
npm ci
npm run store:windows:validate
npm run store:windows:test
```

The test result is
`target/store-artifacts/test-unsigned/TapConductor_<version>_x64_TEST-ONLY-UNSIGNED_store-setup.exe`.
It can be tested with `/S`, but is deliberately marked as ineligible for submission.

Production mode requires `TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER` and an external
`TAPCONDUCTOR_WINDOWS_SIGN_CONFIG`, then verifies timestamped Authenticode:

```powershell
npm run store:windows:production
```

See `MICROSOFT_STORE_RELEASE.md` and `MICROSOFT_STORE_SUBMISSION.md`.

### macOS

On macOS 26 with Xcode 26:

```bash
npm run apple:validate
npm run apple:mac:test
```

The second command builds universal Intel/Apple-Silicon `.app` and `.dmg` bundles with an ad-hoc
signature under `target/apple-artifacts/macos-ad-hoc`.

The host needs Node.js 22, the stable Rust toolchain, the Xcode command-line tools, and XcodeGen.
The first iOS initialization also checks for CocoaPods and `libimobiledevice`; Homebrew is the
recommended way to install those three Apple build helpers.

For Developer ID distribution, install the identity and provide Tauri notarization credentials:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: ..."
export APPLE_API_KEY="..."
export APPLE_API_ISSUER="..."
export APPLE_API_KEY_PATH="/secure/AuthKey_....p8"
npm run apple:mac:release
```

Apple-ID notarization can instead use `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID`.

For the Mac App Store:

```bash
export APPLE_TEAM_ID="..."
export APPLE_MAS_APP_IDENTITY="Apple Distribution: ..."
export APPLE_MAS_INSTALLER_IDENTITY="Mac Installer Distribution: ..."
export APPLE_MAS_PROVISIONING_PROFILE="/secure/TapConductor.provisionprofile"
npm run apple:mac:store
```

This generates concrete sandbox entitlements without committing identity values, embeds the profile,
signs the app, and uses `productbuild` to create the `.pkg`.

### iPad

```bash
npm run apple:ipad:simulator
```

That command generates `src-tauri/gen/apple` when necessary. For a registered device or App Store:

```bash
export APPLE_DEVELOPMENT_TEAM="..."
npm run apple:ipad:development
npm run apple:ipad:store
```

Tauri writes device/App Store IPAs beneath `src-tauri/gen/apple/build/arm64/`. Open the generated
project in Xcode to archive, inspect dSYMs, export the app-thinning report, and distribute through
Xcode or Transporter.

## Store-policy release gates

### Microsoft

The controlling policy is Microsoft Store Policies 7.19:
<https://learn.microsoft.com/en-us/windows/apps/publish/store-policies>.

- 10.1: metadata must match formats, x64 architecture, optional MIDI/ASIO hardware, and the tested
  MusicXML subset.
- 10.2 and 10.2.9: use an offline silent installer at an immutable versioned HTTPS URL, with trusted
  timestamped signatures on every PE.
- 10.3: reviewer instructions use the installed demo score; no account or hardware is required.
- 10.4: clean-VM launch, responsiveness, install/upgrade/uninstall, DPI, accessibility, and stress
  tests must pass.
- 10.5.1: publish `PRIVACY.md` at a stable HTTPS URL even though no data is collected.
- 10.6: request only the file dialog and native functions the product uses.
- Complete listing art, screenshots, IARC, markets, support, source/license, and rights declarations.

The Store receives a signed EXE URL and invokes NSIS with `/S`. Existing unpackaged installations do
not receive Store-managed updates, so the owner must approve an updater/servicing decision and
disclose any network behavior before release.

### Apple

The controlling rules are the current App Review Guidelines:
<https://developer.apple.com/app-store/review/guidelines/>.

- 2.1: complete build, demo-score reviewer steps, no external MIDI dependency.
- 2.4.2: document CPU, memory, battery, wakeup, underrun, and piano-load measurements.
- 2.4.5: the Mac App Store flavor is sandboxed, self-contained, and has no external updater.
- 2.5.1/2.5.2: public APIs only; no downloaded code or samples.
- 2.5.4: iPad audio is foreground-only. Suspension panics voices and releases audio; no background
  audio entitlement is declared.
- 2.5.15: iPad imports through Files/iCloud document-picker copy mode.
- 4.2: explain realtime audio, MIDI, offline sampling, document import, and touch conducting.
- 5.1.1: provide the in-app disclosure and a stable public privacy URL.
- 5.2.1: retain rights evidence for code, icons, fonts, and the CC BY 3.0 piano.

The privacy manifest declares no tracking or collected data and required-reason API categories for
local files, timing, and preferences. Inspect Xcode's generated privacy report before every upload.

## Verification matrix

Automated:

- TypeScript compile and Vite production build.
- Beat scheduler and auto-follow JavaScript tests.
- Locked Rust tests, formatting, and clippy with warnings denied.
- Windows Store config/resource/signature-policy validation.
- Apple JSON/plist/privacy/entitlement validation.
- Universal Mac ad-hoc and iPad simulator package builds.
- Bundled demo-score import test.

Manual release qualification:

- Windows 10/11 x64 clean standard-user VMs, offline `/S`, upgrade, rollback, and uninstall.
- Intel and Apple-Silicon Macs, sandbox and Developer ID, sleep/wake, device loss, and Gatekeeper.
- Older and current iPads in portrait, landscape, Split View, Stage Manager, and memory pressure.
- Built-in/wired/USB audio, optional wireless latency, CoreMIDI, route removal, interruption,
  background/foreground, and media-service reset.
- All imports, malformed/oversized inputs, large scores, piano layers, synth fallback, rolls, Beat
  Tap, direct MIDI, Panic, and long rehearsal stress.
- VoiceOver/Narrator, keyboard control, contrast, text scaling, touch targets, and desktop DPI.
- Thinning and privacy reports, SBOM/license audit, malware scan, WACK where applicable, and hashes.

## External stop-ship items

Source engineering cannot manufacture Partner Center/App Store Connect enrollment, verified
identities, product reservations, certificates, profiles, devices, API keys, immutable hosting,
listing decisions, review approval, or physical audio/MIDI results.

There is also a licensing gate. The project is GPL-3.0-only, influenced by the Windows ASIO route.
Before an Apple App Store upload, copyright owners need qualified legal review and, if necessary, a
contributor-approved App Store exception or dual license. Developer ID macOS distribution can proceed
under GPL compliance while that Store-specific question is resolved. Every release must include the
GPL license, corresponding source/build inputs, notices, and piano CC BY attribution.

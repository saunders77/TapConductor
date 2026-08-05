# Microsoft Store submission template and policy checklist

This is a release template, not a claim that TapConductor is already certified. Replace every
angle-bracketed value with an owner-approved release input.

## Partner Center package fields

| Field | Value |
| --- | --- |
| Product type | EXE or MSI app |
| App type | EXE |
| Architecture | x64 |
| Language | en-US |
| Installer parameter | `/S` |
| Package URL | `<IMMUTABLE-DIRECT-HTTPS-URL>/<version>/TapConductor_<version>_x64_store-setup.exe` |
| Supported systems | x64 Windows 10 and Windows 11 |
| S mode | Not supported by the unpackaged Win32 Store route |
| Category | Music |
| Privacy policy | `<PRIVACY-POLICY-HTTPS-URL>` |
| Support | `<SUPPORT-URL-OR-EMAIL>` |
| Applicable license | GNU GPL version 3 only; full license installed with the app |
| Corresponding source | `https://github.com/saunders77/TapConductor` plus `<VERSIONED-SOURCE-ARCHIVE-URL>` |

Microsoft requires a description, at least one screenshot, 1:1 Store art, applicable license
terms, package architecture/language/type, availability, and IARC age-rating answers:
<https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/create-app-submission>.

## Proposed English listing

### Short description

Perform MusicXML and MIDI scores at your own timing with a built-in sampled grand piano.

### Description

TapConductor turns a written score into performer-controlled playback. Open a MusicXML, compressed
MusicXML, or MIDI file; choose the active parts and audio output; then use the keyboard, pointer,
touchscreen, or an optional MIDI controller to sound each written note or chord when you tap.

Notes that begin at the same musical position across enabled staves and parts sound together.
Rests do not consume taps. You can audition individual notes or chords, start from a selected score
position, route notes to MIDI output, and immediately stop all sound with Panic.

The app includes the Slender Salamander sampled grand piano and a lightweight synthesizer fallback.
Standard Windows audio works without additional software. Optional ASIO output requires a compatible
third-party driver already installed by the user; TapConductor does not install drivers. A wired
audio output is recommended because Bluetooth can add substantial latency.

Requires x64 Windows 10 or Windows 11. MusicXML support covers a practical tested subset; review
import warnings before relying on an unfamiliar score in performance.

### Feature lines

- Performer-controlled MusicXML, MXL, and MIDI playback
- Included sampled grand piano
- Exact simultaneous-note grouping across staves and parts
- Rhythm Tap and Beat Tap performance modes
- Keyboard, pointer, touch, and optional MIDI input
- Note, chord, and score-position audition controls
- Windows audio plus optional installed ASIO devices
- MIDI output and immediate Panic control
- Local score processing with no account required

Do not add claims about universal ASIO latency, native ARM64, Windows S mode, complete MusicXML
coverage, or completed hardware qualification.

## Privacy-policy facts to confirm

The privacy policy must describe the release actually shipped. At minimum, confirm:

- Score contents/paths and audio/MIDI device names are processed locally and are not uploaded.
- Whether any telemetry, crash reporting, or analytics exists.
- If an updater is added, what update host is contacted and what ordinary server data is retained,
  including IP address, app version, OS, and architecture.
- User controls, retention, disclosure, security, and a privacy contact.
- The first-run telemetry choice, direct PostHog processing, pseudonymous identifiers, derived
  country/region, five-minute batching, idle behavior, and the reset/deletion workflow.

Win32 products must always have a privacy policy under policy 10.5.1:
<https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#105-personal-information>.

## Certification notes template

Keep the final note within Partner Center's 2,000-character limit:

> Date: `<YYYY-MM-DD>`. No account, login, or application server is required. TapConductor is fully
> functional with the Windows System default audio output; ASIO and MIDI hardware are optional, and
> the app installs no driver or service. Launch TapConductor, choose Open demo score, and press A,
> Enter, or the TAP button. Grand piano is the default installed instrument. Use an ear control to
> audition a chord, a down-arrow control to reposition, and Panic to stop sound. The installer is
> offline, current-user, and silent with `/S`. Contact: `<CERTIFICATION-CONTACT>`.

The review score URL must be stable and require no login. The score must be owned or licensed for
that use. If the optional ASIO path is highlighted, state that it interoperates with a driver the
user already installed and that WASAPI is the no-driver baseline.

## Policy gate

| Requirement | Evidence before submission |
| --- | --- |
| 10.1 accurate function/value | Listing matches the shipped formats, inputs, piano, limitations, x64 target, and optional driver status. |
| 10.2 security | Offline silent installer; trusted, timestamped signatures on installer and every PE; malware and Smart App Control results; no secondary software or driver. |
| 10.2.7 uninstall | Clean current-user uninstall and upgrade/rollback test record. |
| 10.2.9 Win32 package | Immutable direct HTTPS URL, `.exe`, `/S`, x64, signed PEs, no downloader. |
| 10.3 testable | Stable reviewer score, steps above, no account, WASAPI baseline, optional hardware clearly identified. |
| 10.4 usability | Clean-VM launch/shutdown, device-loss, exception, high-DPI, responsiveness, stress, and download/install-success evidence. |
| 10.5 privacy | Live privacy URL accurately covering local processing and any release-time network behavior. |
| 10.6 capabilities | No Store package capability claims; Tauri ACL remains limited to core functions and the open-file dialog. |
| 10.7 localization | Declare only en-US until the app and listing are fully localized elsewhere. |
| 11.2 rights | GPL/source offer, exact ASIO SDK/source archive, complete notices/SBOM, piano CC BY attribution and provenance, owned listing art. |
| 11.11 age rating | Owner completes the IARC questionnaire accurately in Partner Center. |

Policy source, version 7.19:
<https://learn.microsoft.com/en-us/windows/apps/publish/store-policies>.

## Stop-ship items

Do not upload a package while any of these remains unresolved:

- Artifact filename contains `TEST-ONLY-UNSIGNED`, or any signature is missing/invalid/untimestamped.
- Publisher identity or Partner Center product name has not been verified.
- Package is not offline, opens installer UI under `/S`, or downloads a prerequisite.
- Privacy/support/reviewer/source URLs are placeholders, mutable, unavailable, or require login.
- Updater responsibility for existing unpackaged installations has no approved release decision.
- Store art, screenshots, IARC answers, markets, price, or license terms are incomplete.
- Grand-piano resources/notices are absent or corresponding-source/license obligations are incomplete.
- Installer upgrade/uninstall, hardware, accessibility, or sustained-use release gates have not passed.

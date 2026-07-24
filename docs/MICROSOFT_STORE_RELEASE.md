# Microsoft Store Windows release lane

TapConductor uses Microsoft's supported **unpackaged Win32** Store path. The Store submission
references a signed NSIS installer at an immutable HTTPS URL. This keeps the existing Windows
Tauri, WASAPI, ASIO, MIDI, and grand-piano architecture unchanged.

Microsoft supports listing an existing MSI/EXE installer, and Tauri 2 currently documents that
route for its Windows bundles:

- <https://learn.microsoft.com/en-us/windows/apps/distribute-through-store/how-to-distribute-your-win32-app-through-microsoft-store>
- <https://v2.tauri.app/distribute/microsoft-store/>

The Store flavor is an additive overlay:
`src-tauri/tauri.microsoftstore.conf.json`. It selects only NSIS, retains current-user
installation, embeds the offline WebView2 installer, and repeats the piano, GPL license, and
third-party notices so config merging cannot omit them.

## Build modes

Install pinned dependencies first:

```powershell
npm ci
npm run store:windows:validate
```

Create an explicitly unsigned local test package:

```powershell
npm run store:windows:test
```

For a faster packaging-only iteration after the normal checks have already passed:

```powershell
npm run store:windows:test -- -SkipQualityChecks
```

The output is deliberately named:

```text
target/store-artifacts/test-unsigned/
  TapConductor_<version>_x64_TEST-ONLY-UNSIGNED_store-setup.exe
  DO_NOT_SUBMIT_TO_STORE.txt
  LICENSE
  THIRD_PARTY_NOTICES.md
  SHA256SUMS.txt
```

It can be installed locally with the NSIS `/S` silent switch, but it is not eligible for Store
submission or public distribution.

## Production release inputs

Production mode refuses to run without both inputs:

1. `TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER`: the verified legal publisher name. It must not equal
   `TapConductor`.
2. `TAPCONDUCTOR_WINDOWS_SIGN_CONFIG`: path to an external Tauri JSON overlay that selects a
   trusted Authenticode signing method. Do not put credentials or private keys in the repository.

Certificate-store example:

```json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "<TRUSTED-CODE-SIGNING-CERTIFICATE-THUMBPRINT>",
      "digestAlgorithm": "sha256",
      "timestampUrl": "<CERTIFICATE-AUTHORITY-TIMESTAMP-URL>"
    }
  }
}
```

Artifact Signing example:

```json
{
  "bundle": {
    "windows": {
      "signCommand": "<SIGNING-CLI-AND-NON-SECRET-PROFILE-ARGUMENTS> %1"
    }
  }
}
```

Authentication for a signing service belongs in the CI identity or secret store, not this file.
Then build:

```powershell
$env:TAPCONDUCTOR_WINDOWS_STORE_PUBLISHER = "<VERIFIED-LEGAL-PUBLISHER>"
$env:TAPCONDUCTOR_WINDOWS_SIGN_CONFIG = "C:\secure-config\tapconductor-signing.json"
npm run store:windows:production
```

The script requires valid, timestamped Authenticode signatures on both the installer and application
EXE and stages the result beneath `target/store-artifacts/production/`.

Verify an existing pair without rebuilding:

```powershell
npm run store:windows:verify -- `
  -Mode Production `
  -ArtifactPath "target\store-artifacts\production\TapConductor_<version>_x64_store-setup.exe" `
  -AppExecutablePath "target\release\tapconductor-app.exe"
```

## Release gates outside this repository

The following are mandatory release inputs, not values engineering can invent:

- Partner Center enrollment, reserved product name, and verified publisher identity.
- A production certificate/service chaining to the Microsoft Trusted Root Program.
- An immutable, direct HTTPS/CDN URL for every installer version.
- A privacy-policy URL, support contact, markets/pricing, listing art, screenshots, and IARC answers.
- A supported update policy and HTTPS update endpoint. This repository intentionally contains no
  invented updater endpoint.
- Completed installer, hardware audio/MIDI, accessibility, stress, malware, Smart App Control, and
  upgrade/uninstall qualification.
- A release rights dossier and complete corresponding-source offer for GPLv3/ASIO, plus piano
  attribution evidence.

Do not submit until all gates in `MICROSOFT_STORE_SUBMISSION.md` are complete.

## Sideload and certification checks

Test on clean standard-user Windows 10 and Windows 11 x64 VMs:

1. Disconnect the VM network and install with `/S`; no UI, downloader, forced restart, or elevation
   should be required.
2. Launch from the Start menu and verify System default audio and the bundled grand piano before
   connecting optional MIDI or ASIO hardware.
3. Exercise MusicXML, MXL, MIDI, malformed/oversized input, audio-device loss, Panic, long rehearsal,
   and application shutdown.
4. Test same-version reinstall, version upgrade, failed-install rollback, and clean uninstall from
   Installed Apps.
5. Verify the installer, app EXE, installed PE files, and generated uninstaller with
   `signtool verify /pa /all /v` and Smart App Control audit.
6. Run the applicable Windows App Certification Kit workflow and Microsoft Defender scan.
7. Retain the exact installer, SHA-256, SBOM, notices, source archive, test logs, and build
   provenance. Never replace the bytes at a submitted URL.

Official package requirements:
<https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements>.

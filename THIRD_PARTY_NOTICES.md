# Third-party notices

TapConductor uses the open-source packages below. This list covers direct application, build, and
development dependencies at the versions selected by `Cargo.lock` and `package-lock.json`; their
transitive dependencies remain subject to their own licenses. The lockfiles are the authoritative
inventory for a particular source build. Release engineering should generate and retain a complete
SBOM/license report for every distributed binary.

## Slender Salamander Grand Piano

TapConductor bundles the 44.1 kHz, 16-bit edition of **Slender Salamander Grand Piano**, Signal
Experiments' phase-aligned, three-velocity-layer derivative of Salamander Grand Piano V3. The
original Yamaha C5 recordings are by Alexander Holm; phase alignment and Slender SFZ mappings are
by Signal Experiments. The instrument is licensed under the
[Creative Commons Attribution 3.0 Unported license](https://creativecommons.org/licenses/by/3.0/).
TapConductor loads the supplied note samples and crossfade mapping data with its own native
real-time player; it does not claim authorship of the recordings or derivative sample work.

Source and instrument information:

- <https://sig-ex.com/2017/11/11/slender-salmander-grand-piano/>
- <https://musical-artifacts.com/artifacts/534>

The small procedural piano remains part of TapConductor as a recovery instrument if the sampled
asset is missing or cannot be loaded.

## Audiveris optical music recognition sidecar

Windows installers bundle **Audiveris 5.11.0** as a separate desktop application image, including
the private Java runtime and native Tesseract/Leptonica components produced by Audiveris's official
Windows distribution. Audiveris is licensed under the GNU Affero General Public License version 3.
It is launched as an independent process; it is not linked into TapConductor. The two programs
exchange file paths and `.mxl`/`.omr` files through Audiveris's documented command-line and plugin
interfaces.

The installer payload includes the exact Audiveris corresponding-source archive under
`audiveris/source/`, the Audiveris license materials carried by the official application image, and
a generated `BUNDLE-MANIFEST.json` recording SHA-256 hashes of the MSI, source archive, and bundled
OCR language files. Audiveris source and license information are available from:

- <https://github.com/Audiveris/audiveris/tree/5.11.0>
- <https://github.com/Audiveris/audiveris/blob/5.11.0/LICENSE>
- <https://audiveris.github.io/audiveris/>

Tesseract OCR and its language data have their own Apache-2.0 license and notices in the Audiveris
distribution/source materials. Release engineering must review every staged language file and the
Audiveris transitive/native notices before distribution; the staging manifest is an inventory, not
a substitute for those licenses.

## JavaScript/TypeScript direct dependencies

| Package | Locked version | License |
| --- | ---: | --- |
| `@tauri-apps/api` | 2.11.1 | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-dialog` | 2.7.2 | MIT OR Apache-2.0 |
| `opensheetmusicdisplay` | 1.9.9 | BSD-3-Clause |
| `@tauri-apps/cli` (development/build) | 2.11.4 | Apache-2.0 OR MIT |
| `typescript` (development/build) | 5.9.3 | Apache-2.0 |
| `vite` (development/build) | 7.3.6 | MIT |

## Rust direct dependencies

| Crate | Locked version | License |
| --- | ---: | --- |
| `cpal` | 0.15.3 | Apache-2.0 |
| `asio-sys` | 0.2.6 | Apache-2.0 |
| `midir` | 0.10.4 | MIT |
| `midly` | 0.5.3 | Unlicense |
| `quick-xml` | 0.37.5 | MIT |
| `serde` | 1.0.229 | MIT OR Apache-2.0 |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 |
| `tauri` | 2.11.5 | Apache-2.0 OR MIT |
| `tauri-build` (build) | 2.6.3 | Apache-2.0 OR MIT |
| `tauri-plugin-dialog` | 2.7.2 | Apache-2.0 OR MIT |
| `thiserror` | 2.0.19 | MIT OR Apache-2.0 |
| `tracing` | 0.1.44 | MIT |
| `windows` | 0.62.2 | MIT OR Apache-2.0 |
| `zip` | 2.4.2 | MIT |

## Steinberg ASIO SDK

Windows ASIO builds download and compile the Steinberg ASIO SDK 2.3 during the build through
`asio-sys`. TapConductor elects the SDK's GNU GPL version 3 license route. The SDK is not checked
into this repository; its build-time download includes Steinberg's license and usage guidelines.
ASIO is a trademark of Steinberg Media Technologies GmbH. TapConductor uses the name only to
identify compatible drivers and does not use the ASIO logo or include ASIO in its product name.

The texts and attribution requirements for these licenses are available in the corresponding
package distributions and upstream repositories. The SPDX identifiers above are copied from those
packages' metadata; this notice does not replace their license texts.

# TapConductor browser build

TapConductor's browser edition is a static application. Scores are selected and processed locally;
they are not uploaded to a server. The existing Rust score importer is compiled to WebAssembly, and
the browser host supplies local files, Web Audio, and optional Web MIDI.

## Build prerequisites

- The same Node.js/npm and Rust toolchain used by the native application.
- Rust's WebAssembly target:

  ```text
  rustup target add wasm32-unknown-unknown
  ```

- The matching WebAssembly binding CLI:

  ```text
  cargo install wasm-bindgen-cli --version 0.2.126 --locked
  ```

After installing JavaScript dependencies with `npm ci`, create the uploadable site:

```text
npm run build:web
```

The complete site is written to `dist/`. Upload the **contents** of that directory to a web-server
directory. Its URLs are relative, so it can be hosted at a domain root or under a path such as
`https://example.com/tapconductor/`.

The generated `dist/index.html` is self-contained and can also be opened directly by double-clicking
it. The source-level `index.html` in the repository is a Vite development entry point and is not the
standalone application.

Serve the site over HTTPS. Web MIDI, service workers, and audio-output selection require a secure
context in browsers. Configure the server to return `application/wasm` for `.wasm` files. No
server-side application, database, account, or score-upload endpoint is required.

For a local production preview:

```text
npm run preview:web
```

## Browser capabilities

- MusicXML, compressed MusicXML, and Standard MIDI files use the same normalization code as native.
- Rhythm Tap, Beat Tap, navigation, audition, part filtering, pointer, keyboard, and touch work in
  the browser.
- Playback uses a lightweight procedural Web Audio instrument, avoiding the native application's
  roughly 250 MB piano download.
- Web MIDI input/output is enabled when the browser exposes the Web MIDI API and the user grants
  permission. Keyboard, pointer, and touch remain available otherwise.
- Audio-output selection is enabled only where `AudioContext.setSinkId()` exists. Other browsers
  play through the system-default output.
- Native ASIO/WASAPI/CoreAudio device control and native latency diagnostics remain features of the
  installed application.

## Hosting headers

The defaults on most static hosts are sufficient. A restrictive Permissions Policy must allow the
features if the server sets one:

```text
Permissions-Policy: midi=(self), speaker-selection=(self)
```

To publish a new version, rebuild and replace the previous files together so `index.html`, hashed
assets, and the WebAssembly loader stay in sync.

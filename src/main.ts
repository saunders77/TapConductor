import "./styles.css";
import { OpenSheetMusicDisplay } from "opensheetmusicdisplay";
import { autoFollowTarget } from "./auto-follow";
import { planBeatInterval, rationalValue } from "./beat-scheduler";
import {
  appInvoke as invoke,
  appListen as listen,
  isWebBuild,
  openScoreDialog,
  type UnlistenFn,
} from "./platform";
import type {
  BeatMidiInput,
  CoreEvent,
  DeviceDto,
  DiagnosticsDto,
  LoadedScore,
  MidiPortsDto,
  TapEventDto,
} from "./types";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Missing #app");
const standaloneFingerUrl = (
  globalThis as typeof globalThis & { __TAPCONDUCTOR_FINGER_URL__?: string }
).__TAPCONDUCTOR_FINGER_URL__;
const fingerIconUrl = standaloneFingerUrl
  ?? new URL("../assets/finger transparent-background.png", import.meta.url).href;

app.innerHTML = `
  <div class="shell">
    <header class="topbar">
      <div class="brand" aria-label="TapConductor">
        <img class="brand-mark" src="${fingerIconUrl}" alt="" aria-hidden="true" />
        <span><strong>Tap</strong>Conductor</span>
      </div>
      <button id="open-score" class="primary-button" type="button">
        <span aria-hidden="true">＋</span> Open score
      </button>
      <button id="help-button" class="help-button" type="button" aria-haspopup="dialog" aria-controls="help-overlay" aria-expanded="false">
        Help
      </button>
      <div class="status-pill loading" id="status-pill"><span></span><b>Starting audio…</b></div>
      <div class="web-edition-badge hidden" id="web-edition-badge">Browser edition</div>
    </header>

    <section class="control-deck" aria-label="Performance controls">
      <label class="field" title="Choose the audio device that TapConductor plays through.">
        <span>Audio out</span>
        <select id="audio-output" aria-label="Audio output"><option>System default</option></select>
      </label>
      <label class="field instrument-field" title="Choose the sound used for score playback.">
        <span>Instrument</span>
        <select id="instrument" aria-label="Instrument">
          <option value="piano">Grand piano</option>
          <option value="synth">Synthesizer</option>
        </select>
      </label>
      <label class="field" title="Choose a MIDI controller for conducting or direct play.">
        <span>MIDI in</span>
        <select id="midi-input" aria-label="MIDI input"><option value="">Off</option></select>
      </label>
      <label class="field midi-output-field" title="Choose a MIDI device to receive TapConductor's notes.">
        <span>MIDI out</span>
        <select id="midi-output" aria-label="MIDI output"><option value="">Off</option></select>
      </label>
      <label class="field tap-mode-field" title="Choose whether each tap plays a written event or conducts whole beats.">
        <span>Tap mode</span>
        <select id="tap-mode" aria-label="Tap mode">
          <option value="rhythm">Rhythm Tap</option>
          <option value="beat">Beat Tap</option>
        </select>
      </label>
      <label class="range-field delay-field" title="Set the delay between notes when regular score chords are rolled.">
        <span>Tap Roll <output id="regular-roll-value">0 ms</output></span>
        <input id="regular-roll" type="range" min="0" max="250" value="0" />
      </label>
      <label class="range-field" title="Set the playback volume.">
        <span>Volume <output id="volume-value">100%</output></span>
        <input id="volume" type="range" min="0" max="100" value="100" />
      </label>
      <label class="range-field delay-field" title="Set the delay between notes when auditioned chords are rolled.">
        <span>Chord Roll <output id="audition-roll-value">120 ms</output></span>
        <input id="audition-roll" type="range" min="0" max="250" value="120" />
      </label>
      <button id="parts-button" class="field deck-menu-button" type="button" title="Choose which score parts TapConductor plays.">
        <span>Parts</span><strong id="parts-value">—</strong>
      </button>
        <button id="diagnostics-button" class="field deck-menu-button diagnostics-button" type="button" title="View live audio and MIDI diagnostics." aria-label="Audio diagnostics">
          <span>Diagnostics</span><strong id="diagnostics-value">Starting</strong>
        </button>
        <button id="panic-button" class="panic-button" type="button" title="Play MIDI input directly" aria-label="Play MIDI input directly">■</button>
    </section>

    <aside id="parts-popover" class="popover hidden" aria-label="Parts">
      <h3>Parts</h3><p>Choose which staves play when you tap.</p>
      <div id="parts-list"></div>
    </aside>

    <aside id="diagnostics-popover" class="popover diagnostics hidden" aria-label="Audio diagnostics"></aside>

    <div id="help-overlay" class="help-overlay hidden" role="dialog" aria-modal="true" aria-labelledby="help-title" aria-describedby="help-summary">
      <section class="help-card">
        <div class="help-card-header">
          <div>
            <span class="help-kicker">TapConductor guide</span>
            <h2 id="help-title">Help, privacy, and acknowledgements</h2>
            <p id="help-summary" class="help-summary">Instructions and product information for TapConductor.</p>
          </div>
          <button id="help-close" class="help-close" type="button" aria-label="Close help">×</button>
        </div>
        <nav class="help-jump-links" aria-label="Help topics">
          <a href="#help-instructions">Instructions</a>
          <a href="#privacy">Privacy</a>
          <a href="#acknowledgements">Acknowledgements</a>
        </nav>
        <div class="help-content">
          <section id="help-instructions" tabindex="-1"><h3>1. Open a score</h3><p>Select a MusicXML, compressed MusicXML, or MIDI file (file extensions .musicxml, .xml, .mxl, .mid, or .midi). If you use notation software (like MuseScore, Sibelius, or Dorico) or a DAW (like Ableton Live, Logic Pro, or Cubase), you can use the Export function to create a MusicXML or MIDI file that TapConductor can read. If you only have a PDF, you can use a converter program to create a file TapConductor can read (such as Audiveris or MuseScore).</p></section>
          <section><h3>2. Configure audio settings</h3>
            <p>Use the Audio Out control to select the speakers or sound card to use. On Windows, an option marked (ASIO) is an installed ASIO driver and may provide lower latency on supported hardware. A driver such as ASIO4ALL can route to built-in Realtek speakers or headphones after that endpoint is enabled in the driver's control panel. ASIO is not automatically the best choice for every device or configuration; choose the output that is stable and responsive with your hardware.</p>
            <p id="instrument-help">Choose an instrument, either the grand piano or a synthesizer.</p>
            <p>If you want to control TapConductor with a piano or another MIDI instrument, then plug in the instrument and select it from the MIDI In menu. You'll still be able to tap using normal mouse and keyboard controls too. When you use a piano, TapConductor will use the dynamics you play for each note. If you connect or reconnect a device while TapConductor is open, choose <b>Reload audio &amp; MIDI devices</b> from Audio Out.</p>
            <p>The MIDI OUT setting is only needed if you want to route your performance to another program for recording or further manipulation. For normal playing, it's not necessary.</p>
            <p>By default, all staves (parts) will play during tapping, but you can select specific staves in the PARTS menu.</p>
          </section>
          <section><h3>3. Conduct the score</h3>
            <p>Press the large <b>TAP</b> button, a supported keyboard key (A-Z, numbers, Shift, or punctuation), or your MIDI instrument/piano to play the next written note or chord, starting from the beginning. The location marker will automatically progress to the next note or chord. If you do nothing further, playing does not continue; every note waits for your tap. Hold down the control for longer notes. To play legato, use multiple fingers to hold keys down as you alternate between them. This mode is useful for rehearsals with a choir, performance, or recording. If you want each tap to roll each chord, you can use the ROLL slider at the bottom of the window. </p>
            <p>If you don't want to play a note/chord on every tap, but you instead want to use the program for normal conducting, keeping a steady beat while the notes play, then switch from the Rhythm mode to the Beat mode in the TAP MODE menu. Then you'll need to start by counting in with taps, and each tap will be interpreted as one beat in the music.</p>
            <p>The Stop button on the top right switches to a mode where TapConductor ignores your taps, except for MIDI IN, which it plays directly. Use this mode if you want to play on your piano as you would normally.</p>
          </section>
          <section><h3>4. Hear specific notes and chords</h3>
            <p>Click a note on the score to hear it played at any time - the position indicator doesn't need to be on that note, and the click won't move the position indicator.</p>
            <p>Use the speaker buttons above the score system to hear any chord at any time. It will play a rolled chord from bottom to top if there are multiple notes. You can configure how long time time between rolled notes is with the CHORD slider at the bottom.</p>
          </section>   
          <section><h3>5. Navigate</h3>
            <p>Use the downward-pointing arrows above each score location to control the green location selector and choose where to start playing when you resume tapping. You can also use the left and right arrow keys to move the selector left and right.</p>
            <p>The Spacebar replays the last chord, which can be useful in a rehearsal situation.</p>
          </section>
          <section id="privacy" class="legal-disclosure" tabindex="-1">
            <h3>Privacy</h3>
            <p>TapConductor processes your selected score, taps, connected MIDI device information, audio output information, and performance diagnostics locally on your device. It has no account system, advertising, analytics, tracking, or cloud sync, and it does not upload your scores or performances.</p>
            <p>On iPadOS and macOS, a score chosen through the document picker is copied into TapConductor's private app storage so the sandboxed app can read it. Your original document is not changed. The imported copy may remain in app storage until the operating system clears it or you clear or remove the app's data.</p>
            <p>TapConductor does not request access to your microphone, camera, location, contacts, or photos. The full policy is available in <b>PRIVACY.md</b> and at <span class="legal-url">github.com/saunders77/TapConductor</span>.</p>
          </section>
          <section id="acknowledgements" class="legal-disclosure" tabindex="-1">
            <h3>Acknowledgements</h3>
            <p>The bundled grand piano is <b>Slender Salamander Grand Piano</b>, Signal Experiments' phase-aligned derivative of Salamander Grand Piano V3. The original Yamaha C5 recordings are by Alexander Holm, with phase alignment and Slender SFZ mappings by Signal Experiments. It is used under the Creative Commons Attribution 3.0 Unported license.</p>
            <p>TapConductor also uses open-source Tauri, OpenSheetMusicDisplay, Rust, TypeScript, and supporting libraries. TapConductor is distributed under the GNU General Public License version 3 only. Complete dependency and instrument notices are in the bundled <b>THIRD_PARTY_NOTICES.md</b>.</p>
          </section>
        </div>
        <button id="help-done" class="primary-button" type="button">Got it</button>
      </section>
    </div>

    <main class="workspace">
      <section class="score-panel" aria-label="Musical score">
        <div class="score-toolbar">
          <div class="score-help">Tap to advance · <span aria-hidden="true">◖)</span> play single chord · <span aria-hidden="true">▼</span> start here · select a note to play it</div>
          <div class="zoom-controls" title="Change the displayed score size.">
            <button id="zoom-out" type="button" aria-label="Zoom out">−</button>
            <span class="zoom-label">Zoom</span><output id="zoom-value">90%</output>
            <button id="zoom-in" type="button" aria-label="Zoom in">＋</button>
            <input id="zoom-range" type="range" min="50" max="175" value="90" step="1" aria-label="Zoom" />
          </div>
        </div>
        <div id="score-scroll" class="score-scroll">
          <div id="empty-state" class="empty-state">
            <h1>Play sheet music by tapping.</h1>
            <p>Open your sheet music score in TapConductor using any of these file formats: <b>.musicxml</b>, <b>.xml</b>, <b>.mxl</b>, <b>.mid</b>, <b>.midi</b>.</p>
            <p>If you use notation software (like MuseScore, Sibelius, or Dorico) or a DAW (like Ableton Live, Logic Pro, or Cubase), you can use the Export function to create a MusicXML or MIDI file that TapConductor can read.
            <p>If you only have a PDF, you can use a converter program to create a file TapConductor can read (such as Audiveris or MuseScore).</p>
            <div class="empty-actions" aria-label="Choose a score">
              <button id="empty-open" class="primary-button large" type="button">Open a score</button>
              <button id="demo-open" class="secondary-button large" type="button">Open demo score</button>
            </div>
            <small>Every tap plays the next written note or chord. You can also connect a piano or other MIDI instruments and control dynamics.</small>
            <small>Use any keyboard keys, mouse, tapping the touchscreen, or MIDI</small>
          </div>
          <div id="score-stage" class="score-stage hidden">
            <div id="score-targets" class="score-targets"></div>
            <div id="osmd"></div>
          </div>
        </div>
      </section>

      <footer class="performance-strip">
        <div class="position-readout">
          <span>Live position</span>
          <strong id="position-title">Waiting for a score</strong>
          <small id="position-detail">—</small>
        </div>
        <button id="back-button" class="transport" type="button" disabled aria-label="Previous event">‹</button>
        <button id="tap-button" class="tap-button" type="button" disabled>
          <span>TAP</span>
          <small>Tap <strong>any key A-Z</strong> to play.</small>
          <small>Hold for longer notes.</small>
          <small><strong>Spacebar</strong> to replay a chord.</small>
        </button>
        <button id="forward-button" class="transport" type="button" disabled aria-label="Next event">›</button>
        <div class="next-readout">
          <span>Next</span>
          <strong id="next-title">—</strong>
          <small id="next-detail">—</small>
        </div>
      </footer>
    </main>

    <div id="toast-region" class="toast-region" aria-live="polite"></div>
  </div>
`;

// Keep the header actions in one layout container so the control deck cannot
// overlap the score button or status indicator as the window is resized.
const topbar = document.querySelector<HTMLElement>(".topbar");
const controlDeck = document.querySelector<HTMLElement>(".control-deck");
if (topbar && controlDeck) topbar.append(controlDeck);

const byId = <T extends HTMLElement>(id: string): T => {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing #${id}`);
  return element as T;
};

const elements = {
  open: byId<HTMLButtonElement>("open-score"),
  helpButton: byId<HTMLButtonElement>("help-button"),
  helpOverlay: byId("help-overlay"),
  helpClose: byId<HTMLButtonElement>("help-close"),
  helpDone: byId<HTMLButtonElement>("help-done"),
  emptyOpen: byId<HTMLButtonElement>("empty-open"),
  demoOpen: byId<HTMLButtonElement>("demo-open"),
  status: byId("status-pill"),
  audioOutput: byId<HTMLSelectElement>("audio-output"),
  instrument: byId<HTMLSelectElement>("instrument"),
  midiInput: byId<HTMLSelectElement>("midi-input"),
  midiOutput: byId<HTMLSelectElement>("midi-output"),
  tapMode: byId<HTMLSelectElement>("tap-mode"),
  volume: byId<HTMLInputElement>("volume"),
  volumeValue: byId<HTMLOutputElement>("volume-value"),
  regularRoll: byId<HTMLInputElement>("regular-roll"),
  regularRollValue: byId<HTMLOutputElement>("regular-roll-value"),
  auditionRoll: byId<HTMLInputElement>("audition-roll"),
  auditionRollValue: byId<HTMLOutputElement>("audition-roll-value"),
  partsButton: byId<HTMLButtonElement>("parts-button"),
  partsValue: byId<HTMLElement>("parts-value"),
  partsList: byId("parts-list"),
  partsPopover: byId("parts-popover"),
  diagnosticsButton: byId<HTMLButtonElement>("diagnostics-button"),
  diagnosticsValue: byId<HTMLElement>("diagnostics-value"),
  diagnostics: byId("diagnostics-popover"),
  panic: byId<HTMLButtonElement>("panic-button"),
  empty: byId("empty-state"),
  scoreStage: byId("score-stage"),
  scoreScroll: byId("score-scroll"),
  scoreTargets: byId("score-targets"),
  osmd: byId("osmd"),
  zoomOut: byId<HTMLButtonElement>("zoom-out"),
  zoomIn: byId<HTMLButtonElement>("zoom-in"),
  zoomValue: byId<HTMLOutputElement>("zoom-value"),
  zoomRange: byId<HTMLInputElement>("zoom-range"),
  positionTitle: byId("position-title"),
  positionDetail: byId("position-detail"),
  nextTitle: byId("next-title"),
  nextDetail: byId("next-detail"),
  back: byId<HTMLButtonElement>("back-button"),
  tap: byId<HTMLButtonElement>("tap-button"),
  forward: byId<HTMLButtonElement>("forward-button"),
  toasts: byId("toast-region"),
};

const performanceStrip = document.querySelector<HTMLElement>(".performance-strip");
const bottomControls = document.createElement("div");
bottomControls.className = "bottom-controls";
const footerBrand = document.querySelector<HTMLElement>(".brand");
const zoomControls = document.querySelector<HTMLElement>(".zoom-controls");
if (performanceStrip) {
  if (footerBrand) {
    footerBrand.classList.add("footer-brand");
    performanceStrip.prepend(footerBrand);
  }
  performanceStrip.append(bottomControls);
  [elements.regularRoll.parentElement, elements.volume.parentElement, elements.auditionRoll.parentElement, zoomControls]
    .filter((element): element is HTMLElement => element instanceof HTMLElement)
    .forEach((element) => bottomControls.append(element));
}

function ensureBottomControls(): void {
  if (!performanceStrip) return;
  if (bottomControls.parentElement !== performanceStrip) performanceStrip.append(bottomControls);
  bottomControls.classList.remove("hidden");
  bottomControls.style.display = "flex";
}

let score: LoadedScore | null = null;
let cursorIndex = 0;
let highlightIndex = 0;
let mostRecentChordIndex: number | null = null;
let zoom = 0.9;
const ZOOM_STEPS = [50, 75, 90, 100, 110, 125, 150, 175];
let osmd: OpenSheetMusicDisplay | null = null;
let osmdEventSteps: number[] = [];
let osmdBeatSteps: number[] = [];
let osmdCurrentStep = 0;
let eventHorizontalPositions: number[] = [];
let beatHorizontalPositions: number[] = [];
let measureHorizontalPositions = new Map<number, number>();
let orderedMeasureHorizontalPositions: number[] = [];
let eventHighlightNodes: HTMLElement[][] = [];
let activeHighlightNodes: HTMLElement[] = [];
let beatHighlightNode: HTMLDivElement | null = null;
let beatHighlightVisuals: Array<{ top: number; height: number } | undefined> = [];
type ScorePerformance = {
  coreLoadMs?: number;
  osmdLoadMs?: number;
  osmdRenderMs?: number;
  targetBuildMs?: number;
  displayTotalMs?: number;
  visualSteps?: number;
  targetNodes?: number;
};
let scorePerformance: ScorePerformance | null = null;
let lastDiagnostics: DiagnosticsDto | null = null;
let lastUiNativeRoundTripMs: number | null = null;
let unlisteners: UnlistenFn[] = [];
const heldTokens = new Set<string>();
let midiFreePlay = false;
let selectedAudioDeviceId = "";
const RELOAD_AUDIO_SYSTEMS_VALUE = "__reload_audio_systems__";

function updateMidiFreePlayButton(): void {
  elements.panic.classList.toggle("midi-free-play", midiFreePlay);
  elements.panic.textContent = midiFreePlay ? "☟" : "■";
  elements.panic.title = midiFreePlay
    ? "Taps start following the score again"
    : "Stop conducting the score";
  elements.panic.setAttribute("aria-label", elements.panic.title);
}
const pendingDowns = new Map<string, Promise<void>>();
const DEFAULT_VELOCITY = 96;
const MINIMUM_POINTER_NOTE_HOLD_MS = 250;
const SCORE_MEASURE_PREFIX = "tapconductor:";
type PointerHold = {
  token: string;
  releaseAt: Promise<number>;
  released: boolean;
};

function recordScorePhase(
  phase: keyof Pick<ScorePerformance, "coreLoadMs" | "osmdLoadMs" | "osmdRenderMs" | "targetBuildMs" | "displayTotalMs">,
  startedAt: number,
): number {
  const duration = performance.now() - startedAt;
  scorePerformance ??= {};
  scorePerformance[phase] = duration;
  const measureName = `${SCORE_MEASURE_PREFIX}${phase}`;
  try {
    performance.clearMeasures(measureName);
    performance.measure(measureName, { start: startedAt, duration });
  } catch {
    // The diagnostics values remain available on older WebViews that do not
    // implement PerformanceMeasureOptions.
  }
  return duration;
}

function registerEventHighlight(node: HTMLElement, eventIndices: number[]): void {
  for (const eventIndex of eventIndices) {
    const nodes = eventHighlightNodes[eventIndex] ?? [];
    nodes.push(node);
    eventHighlightNodes[eventIndex] = nodes;
  }
}

function clearActiveHighlights(): void {
  for (const node of activeHighlightNodes) node.classList.remove("current");
  activeHighlightNodes = [];
  if (beatHighlightNode) beatHighlightNode.classList.remove("current");
}

function rolledFinalOnsetDelay(noteCount: number, rollMs: number): number {
  return Math.max(0, noteCount - 1) * Math.max(0, rollMs);
}

function createPointerHold(
  token: string,
  down: Promise<void>,
  finalOnsetDelayMs: number,
  minimumHoldMs = MINIMUM_POINTER_NOTE_HOLD_MS,
): PointerHold {
  return {
    token,
    releaseAt: down.then(() => performance.now() + finalOnsetDelayMs + minimumHoldMs),
    released: false,
  };
}

async function releasePointerHold(hold: PointerHold): Promise<void> {
  if (hold.released) return;
  hold.released = true;
  try {
    const releaseAt = await hold.releaseAt;
    const remaining = releaseAt - performance.now();
    if (remaining > 0) {
      await new Promise<void>((resolve) => window.setTimeout(resolve, remaining));
    }
  } catch {
    // The down path already reported its error and removed the held token.
  }
  await performUp(hold.token);
}

type TapMode = "rhythm" | "beat";
let tapMode: TapMode = "rhythm";
let beatCountRequired = 0;
let beatCounted = 0;
let beatPlaying = false;
let beatIndex = 0;
let beatNextEventIndex = 0;
let beatTimes: number[] = [];
let beatTimers = new Map<number, number>();
let beatQueue: Promise<void> = Promise.resolve();
let beatRunId = 0;
let lastPressedBeatIndex: number | null = null;
let activeBeatVisualIndex: number | null = null;

function clearBeatTimers(): void {
  beatTimers.forEach((_index, timer) => window.clearTimeout(timer));
  beatTimers.clear();
}

function flushBeatTimers(): void {
  const pending = [...beatTimers.values()].sort((left, right) => left - right);
  clearBeatTimers();
  pending.forEach(dispatchBeatEvent);
}

function updateTapButtonLabel(): void {
  const label = elements.tap.querySelector("span");
  if (!label) return;
  if (tapMode === "rhythm") {
    label.textContent = "TAP";
  } else if (!beatPlaying) {
    label.textContent = beatCounted === 0 ? "COUNT IN" : `COUNT ${beatCounted}/${beatCountRequired}`;
  } else if (lastPressedBeatIndex !== null && score?.beats[lastPressedBeatIndex]) {
    const beat = score.beats[lastPressedBeatIndex]!;
    label.textContent = `BEAT ${beat.beatIndex + 1}/${beat.beatsInMeasure}`;
  } else {
    label.textContent = "READY";
  }
}

function resetBeatTap(): void {
  beatRunId += 1;
  clearBeatTimers();
  beatTimes = [];
  beatPlaying = false;
  beatCounted = 0;
  lastPressedBeatIndex = null;
  activeBeatVisualIndex = null;
  beatNextEventIndex = cursorIndex;
  if (!score || score.beats.length === 0 || cursorIndex >= score.events.length) {
    beatIndex = 0;
    beatCountRequired = 4;
  } else {
    const eventPosition = rationalValue(score.events[cursorIndex]!.absolute);
    beatIndex = score.beats.reduce(
      (best, beat, index) => rationalValue(beat.absolute) <= eventPosition + 1e-9 ? index : best,
      0,
    );
    const target = score.beats[beatIndex]!;
    beatCountRequired = Math.max(2, target.beatsInMeasure + target.beatIndex);
  }
  updateTapButtonLabel();
  if (score) updatePosition();
}

function pauseBeatTap(): void {
  if (!beatPlaying) return;
  clearBeatTimers();
  beatPlaying = false;
  beatCounted = 0;
  beatTimes = [];
  beatNextEventIndex = cursorIndex;
  void invoke("panic").catch(() => undefined);
  toast("Beat Tap paused: release the tap, then count in again.", "warning");
  resetBeatTap();
}

function dispatchBeatEvent(index: number): void {
  if (!score || index >= score.events.length) return;
  const runId = beatRunId;
  beatQueue = beatQueue.then(async () => {
    if (tapMode !== "beat" || !beatPlaying || runId !== beatRunId) return;
    activeBeatVisualIndex = null;
    highlightIndex = index;
    updatePosition();
    const token = `beat-auto:${crypto.randomUUID()}`;
    await invokeSafe<void>("performance_input_down", { token, velocity: DEFAULT_VELOCITY });
    await invokeSafe<void>("release_input", { token });
  }).catch(() => undefined);
}

function scheduleBeatInterval(): void {
  if (!score || beatIndex >= score.beats.length) return;
  const currentBeatIndex = beatIndex;
  const currentBeat = score.beats[currentBeatIndex]!;
  const nextBeat = score.beats[beatIndex + 1];
  const interval = beatTimes.length >= 2 ? beatTimes.at(-1)! - beatTimes.at(-2)! : 500;
  const plan = planBeatInterval(score.events, beatNextEventIndex, currentBeat, nextBeat, interval);

  lastPressedBeatIndex = currentBeatIndex;
  activeBeatVisualIndex = currentBeatIndex;
  beatNextEventIndex = plan.nextEventIndex;
  beatIndex += 1;
  updateTapButtonLabel();
  updatePosition();

  for (const planned of plan.events) {
    if (planned.delayMs <= 0) {
      dispatchBeatEvent(planned.eventIndex);
      continue;
    }
    const timer = window.setTimeout(() => {
      beatTimers.delete(timer);
      if ([...heldTokens].some((token) => !token.startsWith("audition"))) {
        pauseBeatTap();
        return;
      }
      dispatchBeatEvent(planned.eventIndex);
    }, planned.delayMs);
    beatTimers.set(timer, planned.eventIndex);
  }
}

function beatTapDown(token: string): void {
  if (!score || heldTokens.has(token)) return;
  if (beatPlaying) flushBeatTimers();
  heldTokens.add(token);
  const now = performance.now();
  beatTimes.push(now);
  if (beatTimes.length > 2) beatTimes.shift();
  if (!beatPlaying) {
    beatCounted += 1;
    if (beatCounted >= beatCountRequired) {
      beatPlaying = true;
      updateTapButtonLabel();
    } else {
      updateTapButtonLabel();
    }
    return;
  }
  scheduleBeatInterval();
}

function setStatus(kind: "ready" | "loading" | "fault", label: string): void {
  elements.status.className = `status-pill ${kind}`;
  elements.status.querySelector("b")!.textContent = label;
}

function toast(message: string, kind: "info" | "warning" | "error" = "info"): void {
  const item = document.createElement("div");
  item.className = `toast ${kind}`;
  item.textContent = message;
  elements.toasts.append(item);
  window.setTimeout(() => item.remove(), 5500);
}

function noteName(midi: number): string {
  const names = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
  return `${names[midi % 12] ?? "?"}${Math.floor(midi / 12) - 1}`;
}

function describeEvent(event: TapEventDto | undefined): { title: string; detail: string } {
  if (!event) return { title: "End of score", detail: "—" };
  const pitches = event.notes.map((note) => noteName(note.midiPitch));
  const title = pitches.length === 1 ? pitches[0]! : `${pitches.length}-note chord`;
  return { title, detail: `Measure ${event.measureNumber} · ${pitches.join("  ")}` };
}

function updatePosition(): void {
  if (!score) return;
  const current = describeEvent(score.events[cursorIndex]);
  const next = describeEvent(score.events[cursorIndex + 1]);
  elements.positionTitle.textContent = current.title;
  elements.positionDetail.textContent = current.detail;
  elements.nextTitle.textContent = next.title;
  elements.nextDetail.textContent = next.detail;
  elements.back.disabled = cursorIndex <= 0;
  elements.forward.disabled = cursorIndex >= score.events.length - 1;
  const displayedBeatIndex = activeBeatVisualIndex !== null
    && osmdBeatSteps[activeBeatVisualIndex] !== undefined
    ? activeBeatVisualIndex
    : null;
  if (displayedBeatIndex !== null) {
    moveOsmdCursorToStep(osmdBeatSteps[displayedBeatIndex]!);
    autoFollowPosition(beatHorizontalPositions[displayedBeatIndex]);
  } else {
    moveOsmdCursor(highlightIndex);
    autoFollowPosition(eventHorizontalPositions[highlightIndex]);
  }
  clearActiveHighlights();
  if (displayedBeatIndex !== null && beatHighlightNode) {
    const left = beatHorizontalPositions[displayedBeatIndex];
    const visual = beatHighlightVisuals[displayedBeatIndex];
    if (left !== undefined && visual) {
      beatHighlightNode.style.left = `${left - 10}px`;
      beatHighlightNode.style.top = `${visual.top}px`;
      beatHighlightNode.style.height = `${visual.height}px`;
      beatHighlightNode.classList.add("current");
    }
  } else {
    activeHighlightNodes = eventHighlightNodes[highlightIndex] ?? [];
    for (const node of activeHighlightNodes) node.classList.add("current");
  }
}

function autoFollowPosition(sliceLeft: number | undefined): void {
  if (sliceLeft === undefined) return;
  const orderedBars = orderedMeasureHorizontalPositions;
  let barWidth = 180 * zoom;
  let low = 0;
  let high = orderedBars.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (orderedBars[middle]! <= sliceLeft + 1) low = middle + 1;
    else high = middle;
  }
  const barIndex = low - 1;
  if (barIndex >= 0 && barIndex + 1 < orderedBars.length) {
    barWidth = orderedBars[barIndex + 1]! - orderedBars[barIndex]!;
  } else if (barIndex > 0) {
    barWidth = orderedBars[barIndex]! - orderedBars[barIndex - 1]!;
  }
  barWidth = Math.max(48, barWidth);

  const sliceInScrollContent = elements.scoreStage.offsetLeft + sliceLeft;
  const target = autoFollowTarget(
    sliceInScrollContent,
    elements.scoreScroll.scrollLeft,
    elements.scoreScroll.clientWidth,
    barWidth,
  );
  if (target !== undefined) {
    elements.scoreScroll.scrollTo({
      left: target,
      behavior: "auto",
    });
  }
}

function moveOsmdCursor(index: number): void {
  if (!osmd || score?.format !== "music_xml") return;
  moveOsmdCursorToStep(osmdEventSteps[index] ?? index);
}

function moveOsmdCursorToStep(visualStep: number): void {
  if (!osmd || score?.format !== "music_xml") return;
  try {
    if (visualStep < osmdCurrentStep) {
      osmd.cursor.reset();
      osmdCurrentStep = 0;
    }
    while (osmdCurrentStep < visualStep) {
      osmd.cursor.next();
      osmdCurrentStep += 1;
    }
    osmd.cursor.hide();
  } catch {
    // Some malformed scores have fewer graphical cursor positions than semantic events.
  }
}

async function invokeSafe<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    toast(message, "error");
    throw error;
  }
}

async function chooseScore(): Promise<void> {
  const path = await openScoreDialog();
  if (!path) return;
  await loadScore(
    () => invokeSafe<LoadedScore>("load_score", { path }),
    "Loading score…",
  );
}

async function loadDemoScore(): Promise<void> {
  await loadScore(
    () => invokeSafe<LoadedScore>("load_demo_score"),
    "Loading demo score…",
  );
}

async function loadScore(
  loader: () => Promise<LoadedScore>,
  loadingMessage: string,
): Promise<void> {
  setStatus("loading", loadingMessage);
  const loadButtons = [elements.open, elements.emptyOpen, elements.demoOpen];
  loadButtons.forEach((button) => {
    button.disabled = true;
  });
  let loaded: LoadedScore | null = null;
  try {
    scorePerformance = {};
    const coreLoadStarted = performance.now();
    loaded = await loader();
    recordScorePhase("coreLoadMs", coreLoadStarted);
    await displayScore(loaded);
  } catch (error) {
    // Native load failures are already surfaced by invokeSafe. Rendering is a
    // separate step, so make OSMD failures visible instead of looking like an
    // unresponsive file picker.
    if (loaded) {
      const message = error instanceof Error ? error.message : String(error);
      toast(`The score loaded, but its notation could not be displayed: ${message}`, "error");
    }
    setStatus("fault", "Score load failed");
  } finally {
    loadButtons.forEach((button) => {
      button.disabled = false;
    });
  }
}

type ScoreViewState = {
  event?: TapEventDto;
  scrollLeft: number;
};

function indexForPreservedEvent(events: TapEventDto[], previous: TapEventDto | undefined): number {
  if (!previous) return 0;
  const exact = events.findIndex((event) =>
    event.measureIndex === previous.measureIndex
    && event.offset.numerator === previous.offset.numerator
    && event.offset.denominator === previous.offset.denominator,
  );
  if (exact >= 0) return exact;
  const following = events.findIndex((event) => event.measureIndex > previous.measureIndex);
  return following >= 0 ? following : Math.max(0, events.length - 1);
}

async function displayScore(loaded: LoadedScore, preserved?: ScoreViewState): Promise<void> {
  const displayStarted = performance.now();
  if (zoomRenderTimer !== null) {
    window.clearTimeout(zoomRenderTimer);
    zoomRenderTimer = null;
  }
  renderedZoom = null;
  ensureBottomControls();
  window.requestAnimationFrame(ensureBottomControls);
  const preserveView = preserved !== undefined;
  const preservedCursor = indexForPreservedEvent(loaded.events, preserved?.event);
  const preservedScrollLeft = preserved?.scrollLeft ?? 0;
  score = loaded;
  osmdEventSteps = [];
  osmdBeatSteps = [];
  osmdCurrentStep = 0;
  eventHorizontalPositions = [];
  beatHorizontalPositions = [];
  measureHorizontalPositions = new Map();
  orderedMeasureHorizontalPositions = [];
  eventHighlightNodes = [];
  activeHighlightNodes = [];
  beatHighlightNode = null;
  beatHighlightVisuals = [];
  cursorIndex = preserveView ? Math.min(preservedCursor, Math.max(0, loaded.events.length - 1)) : 0;
  highlightIndex = cursorIndex;
  mostRecentChordIndex = null;
  elements.empty.classList.add("hidden");
  elements.scoreStage.classList.remove("hidden");
  elements.tap.disabled = false;
  renderParts();

  elements.osmd.replaceChildren();
  elements.scoreTargets.replaceChildren();
  elements.scoreStage.style.removeProperty("width");
  if (!preserveView) elements.scoreScroll.scrollLeft = 0;
  if (loaded.format === "music_xml" && loaded.musicXml) {
    osmd = new OpenSheetMusicDisplay(elements.osmd, {
      autoResize: false,
      backend: "svg",
      drawTitle: false,
      drawingParameters: "compacttight",
      followCursor: false,
      cursorsOptions: [{ type: 1, color: "#75ffb3", alpha: 0, follow: false }],
      pageFormat: "Endless",
      renderSingleHorizontalStaffline: true,
      newSystemFromXML: false,
      newSystemFromNewPageInXML: false,
      newPageFromXML: false,
    });
    // OSMD caps a single horizontal system at 32,767 px by default (a
    // defensive limit for its canvas backend). We render SVG, where that cap
    // is unnecessary and causes very long scores to wrap onto another line.
    osmd.EngravingRules.SheetMaximumWidth = 1_000_000;
    const osmdLoadStarted = performance.now();
    await osmd.load(loaded.musicXml);
    recordScorePhase("osmdLoadMs", osmdLoadStarted);
    await scheduleOsmdRender("score");
  } else {
    osmd = null;
    renderMidiRoll(loaded.events);
  }

  updatePosition();
  if (tapMode === "beat") resetBeatTap();
  if (preserveView) {
    window.setTimeout(() => {
      elements.scoreScroll.scrollLeft = preservedScrollLeft;
      updatePosition();
    }, 0);
  }
  recordScorePhase("displayTotalMs", displayStarted);
  if (scorePerformance) {
    console.info("TapConductor score display performance", { ...scorePerformance });
  }
  setStatus("ready", "Ready");
  loaded.warnings.forEach((warning) => toast(warning, "warning"));
}

type RenderWaiter = {
  resolve: () => void;
  reject: (error: unknown) => void;
};
let renderRequested = false;
let renderRunning = false;
let renderFrame: number | null = null;
let renderRequestVersion = 0;
let renderWaiters: RenderWaiter[] = [];
let renderReason = "score";
let renderedZoom: number | null = null;

function scheduleOsmdRender(reason: "score" | "zoom"): Promise<void> {
  if (!osmd) return Promise.resolve();
  renderRequested = true;
  renderReason = reason;
  renderRequestVersion += 1;
  const promise = new Promise<void>((resolve, reject) => {
    renderWaiters.push({ resolve, reject });
  });
  if (!renderRunning && renderFrame === null) {
    renderFrame = window.requestAnimationFrame(() => {
      renderFrame = null;
      void drainOsmdRenders();
    });
  }
  return promise;
}

async function drainOsmdRenders(): Promise<void> {
  if (renderRunning) return;
  renderRunning = true;
  try {
    while (renderRequested) {
      renderRequested = false;
      const version = renderRequestVersion;
      const reason = renderReason;
      await renderOsmdNow(version);
      if (version === renderRequestVersion && reason === "zoom") {
        updatePosition();
      }
    }
    const completed = renderWaiters;
    renderWaiters = [];
    completed.forEach((waiter) => waiter.resolve());
  } catch (error) {
    const failed = renderWaiters;
    renderWaiters = [];
    failed.forEach((waiter) => waiter.reject(error));
  } finally {
    renderRunning = false;
    if (renderRequested && renderFrame === null) {
      renderFrame = window.requestAnimationFrame(() => {
        renderFrame = null;
        void drainOsmdRenders();
      });
    }
  }
}

async function renderOsmdNow(version: number): Promise<void> {
  const activeOsmd = osmd;
  if (!activeOsmd) return;
  ensureBottomControls();
  const renderZoom = zoom;
  activeOsmd.Zoom = renderZoom;
  const renderStarted = performance.now();
  activeOsmd.render();
  renderedZoom = renderZoom;
  recordScorePhase("osmdRenderMs", renderStarted);
  activeOsmd.cursor.show();
  await new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
  if (version !== renderRequestVersion || activeOsmd !== osmd) return;

  const contentWidth = Math.max(elements.osmd.scrollWidth, elements.osmd.getBoundingClientRect().width);
  if (contentWidth > 0) {
    elements.scoreStage.style.width = `${Math.ceil(contentWidth + 68)}px`;
  }
  const targetsStarted = performance.now();
  const targetStats = buildScoreTargets();
  recordScorePhase("targetBuildMs", targetsStarted);
  scorePerformance ??= {};
  scorePerformance.visualSteps = targetStats.visualSteps;
  scorePerformance.targetNodes = targetStats.targetNodes;
}

function renderMidiRoll(events: TapEventDto[]): void {
  const wrapper = document.createElement("div");
  wrapper.className = "midi-roll";
  for (const event of events) {
    const eventCard = document.createElement("div");
    eventCard.className = "midi-event";
    eventCard.dataset.eventIndex = String(event.index);
    eventCard.dataset.eventIndices = String(event.index);
    registerEventHighlight(eventCard, [event.index]);
    eventCard.style.setProperty("--event-height", String(Math.max(1, event.notes.length)));
    const measure = document.createElement("b");
    measure.textContent = event.measureNumber;
    const notes = document.createElement("div");
    notes.className = "midi-notes";
    for (const note of event.notes) {
      const noteButton = document.createElement("button");
      noteButton.type = "button";
      noteButton.className = "midi-note";
      noteButton.textContent = noteName(note.midiPitch);
      installAuditionHandlers(noteButton, () => event.index, [note.midiPitch]);
      notes.append(noteButton);
    }
    eventCard.append(createSliceControls(() => event.index, event.measureNumber), measure, notes);
    wrapper.append(eventCard);
  }
  elements.osmd.append(wrapper);
  window.requestAnimationFrame(() => {
    wrapper.querySelectorAll<HTMLElement>(".midi-event").forEach((eventCard, index) => {
      eventHorizontalPositions[index] = eventCard.offsetLeft;
      const measureIndex = events[index]?.measureIndex;
      if (measureIndex !== undefined && !measureHorizontalPositions.has(measureIndex)) {
        measureHorizontalPositions.set(measureIndex, eventCard.offsetLeft);
      }
    });
    orderedMeasureHorizontalPositions = [...new Set(measureHorizontalPositions.values())]
      .sort((left, right) => left - right);
  });
}

function buildScoreTargets(): { visualSteps: number; targetNodes: number } {
  if (!osmd || !score) return { visualSteps: 0, targetNodes: 0 };
  const activeScore = score;
  const targetFragment = document.createDocumentFragment();
  eventHorizontalPositions = [];
  beatHorizontalPositions = [];
  measureHorizontalPositions = new Map();
  orderedMeasureHorizontalPositions = [];
  eventHighlightNodes = [];
  activeHighlightNodes = [];
  beatHighlightNode = null;
  beatHighlightVisuals = [];
  const hostRect = elements.scoreStage.getBoundingClientRect();
  const cursor = osmd.cursor;
  cursor.reset();
  osmdCurrentStep = 0;
  cursor.show();

  type NoteVisual = {
    left: number;
    top: number;
    width: number;
    height: number;
    candidates: number[];
    partIndex?: number;
    staffId?: number;
  };
  type VisualStep = {
    step: number;
    measureIndex: number;
    numerator: number;
    denominator: number;
    left: number;
    anchorLeft: number;
    top: number;
    height: number;
    notes: NoteVisual[];
  };
  const visualSteps: VisualStep[] = [];
  const maximumSteps = Math.max(10_000, activeScore.events.length * 8 + 100);
  for (let step = 0; step < maximumSteps && !cursor.Iterator.EndReached; step += 1) {
    const timestamp = cursor.Iterator.CurrentRelativeInMeasureTimestamp;
    const rect = cursor.cursorElement?.getBoundingClientRect();
    if (timestamp && rect) {
      const noteVisuals: NoteVisual[] = [];
      for (const graphicalNote of cursor.GNotesUnderCursor()) {
        try {
          const rendered = graphicalNote as unknown as {
            sourceNote?: OsmdSourceNote;
            getNoteheadSVGs?: () => HTMLElement[];
          };
          if (rendered.sourceNote?.isRest?.()) continue;
          const rectangles = (rendered.getNoteheadSVGs?.() ?? [])
            .map((head) => head.getBoundingClientRect())
            .filter((headRect) => headRect.width > 0 && headRect.height > 0);
          if (rectangles.length === 0) continue;
          const left = Math.min(...rectangles.map((headRect) => headRect.left));
          const right = Math.max(...rectangles.map((headRect) => headRect.right));
          const top = Math.min(...rectangles.map((headRect) => headRect.top));
          const bottom = Math.max(...rectangles.map((headRect) => headRect.bottom));
          noteVisuals.push({
            left: left - hostRect.left,
            top: top - hostRect.top,
            width: right - left,
            height: bottom - top,
            candidates: osmdMidiCandidates(rendered.sourceNote),
            partIndex: rendered.sourceNote?.ParentStaff?.ParentInstrument?.Id,
            staffId: rendered.sourceNote?.ParentStaff?.Id,
          });
        } catch {
          // A note without SVG geometry remains visible and playable as part
          // of its slice; only its optional direct-click overlay is omitted.
        }
      }
      const stepLeft = rect.left - hostRect.left - 5;
      const measureLeft = measureHorizontalPositions.get(cursor.Iterator.CurrentMeasureIndex);
      if (measureLeft === undefined || stepLeft < measureLeft) {
        measureHorizontalPositions.set(cursor.Iterator.CurrentMeasureIndex, stepLeft);
      }
      const noteCenters = noteVisuals
        .map((note) => note.left + note.width / 2)
        .sort((left, right) => left - right);
      const anchorLeft = noteCenters.length > 0
        ? noteCenters[Math.floor(noteCenters.length / 2)]!
        : stepLeft;
      visualSteps.push({
        step,
        measureIndex: cursor.Iterator.CurrentMeasureIndex,
        numerator: timestamp.GetExpandedNumerator(),
        denominator: timestamp.Denominator,
        left: stepLeft,
        anchorLeft,
        top: rect.top - hostRect.top - 25,
        height: rect.height + 50,
        notes: noteVisuals,
      });
    }
    cursor.next();
    osmdCurrentStep += 1;
  }

  const rationalKey = (measureIndex: number, numerator: number, denominator: number): string => {
    if (denominator === 0) return "";
    let left = BigInt(numerator);
    let right = BigInt(denominator);
    if (right < 0n) {
      left = -left;
      right = -right;
    }
    let a = left < 0n ? -left : left;
    let b = right;
    while (b !== 0n) {
      const remainder = a % b;
      a = b;
      b = remainder;
    }
    const divisor = a === 0n ? 1n : a;
    return `${measureIndex}:${left / divisor}/${right / divisor}`;
  };
  const stepsByPosition = new Map<string, VisualStep[]>();
  for (const visual of visualSteps) {
    const key = rationalKey(visual.measureIndex, visual.numerator, visual.denominator);
    const matches = stepsByPosition.get(key) ?? [];
    matches.push(visual);
    stepsByPosition.set(key, matches);
  }
  const matchingVisualSteps = (
    measureIndex: number,
    numerator: number,
    denominator: number,
  ): VisualStep[] => {
    // The normalized core uses quarter-note units while OSMD commonly exposes
    // whole-note fractions. Index both accepted representations.
    const directKey = rationalKey(measureIndex, numerator, denominator);
    const quarterKey = rationalKey(measureIndex, numerator, denominator * 4);
    const direct = stepsByPosition.get(directKey) ?? [];
    if (quarterKey === directKey) return direct;
    const scaled = stepsByPosition.get(quarterKey) ?? [];
    if (direct.length === 0) return scaled;
    if (scaled.length === 0) return direct;
    return [...new Map([...direct, ...scaled].map((step) => [step.step, step])).values()]
      .sort((left, right) => left.step - right.step);
  };

  osmdBeatSteps = [];
  beatHorizontalPositions = [];
  beatHighlightVisuals = [];
  activeScore.beats.forEach((beat, index) => {
    const beatOffsetNumerator = beat.beatIndex * 4;
    const visual = matchingVisualSteps(beat.measureIndex, beatOffsetNumerator, beat.beatType)[0];
    if (!visual) return;
    osmdBeatSteps[index] = visual.step;
    beatHorizontalPositions[index] = visual.anchorLeft;
    beatHighlightVisuals[index] = { top: visual.top, height: visual.height };
  });
  if (beatHighlightVisuals.some(Boolean)) {
    beatHighlightNode = document.createElement("div");
    beatHighlightNode.className = "slice-ghost beat-ghost";
    beatHighlightNode.style.width = "20px";
    targetFragment.append(beatHighlightNode);
  }

  const groupedTargets = new Map<string, { eventIndices: number[]; visual: VisualStep; measureNumber: string }>();
  const eventIndicesByStep = new Map<number, number[]>();
  osmdEventSteps = [];
  for (const event of activeScore.events) {
    const candidates = matchingVisualSteps(
      event.measureIndex,
      event.offset.numerator,
      event.offset.denominator,
    );
    const visual = candidates[Math.max(0, event.occurrence - 1)] ?? candidates[0] ?? visualSteps[event.index];
    if (!visual) continue;
    osmdEventSteps[event.index] = visual.step;
    const indicesAtStep = eventIndicesByStep.get(visual.step) ?? [];
    indicesAtStep.push(event.index);
    eventIndicesByStep.set(visual.step, indicesAtStep);
    eventHorizontalPositions[event.index] = visual.anchorLeft;
    const measureLeft = measureHorizontalPositions.get(event.measureIndex);
    if (measureLeft === undefined || visual.left < measureLeft) {
      measureHorizontalPositions.set(event.measureIndex, visual.left);
    }
    const targetKey = `${event.measureIndex}:${event.offset.numerator}/${event.offset.denominator}`;
    const existing = groupedTargets.get(targetKey);
    if (existing) {
      existing.eventIndices.push(event.index);
    } else {
      groupedTargets.set(targetKey, {
        eventIndices: [event.index],
        visual,
        measureNumber: event.measureNumber,
      });
    }
  }

  for (const target of groupedTargets.values()) {
    const resolveIndex = (): number =>
      target.eventIndices.find((index) => index >= cursorIndex) ?? target.eventIndices[0]!;
    const controls = createSliceControls(resolveIndex, target.measureNumber);
    controls.dataset.eventIndices = target.eventIndices.join(",");
    controls.style.left = `${target.visual.anchorLeft}px`;
    const controlTop = Math.max(4, target.visual.top - 70);
    controls.style.top = `${controlTop}px`;
    if (target.visual.notes.length > 0) {
      const noteLeft = Math.min(...target.visual.notes.map((note) => note.left));
      const noteRight = Math.max(...target.visual.notes.map((note) => note.left + note.width));
      const noteTop = Math.min(...target.visual.notes.map((note) => note.top));
      const noteBottom = Math.max(...target.visual.notes.map((note) => note.top + note.height));
      controls.classList.add("has-highlight");
      controls.style.setProperty("--highlight-left", `${noteLeft - target.visual.anchorLeft + 2}px`);
      controls.style.setProperty("--highlight-top", `${target.visual.top - controlTop}px`);
      controls.style.setProperty("--highlight-width", `${Math.max(28, noteRight - noteLeft + 20)}px`);
      controls.style.setProperty("--highlight-height", `${Math.max(noteBottom - noteTop + 28, target.visual.height)}px`);
    }
    registerEventHighlight(controls, target.eventIndices);
    targetFragment.append(controls);
  }

  type StaffNoteGroup = {
    left: number;
    right: number;
    top: number;
    bottom: number;
    midiPitches: Set<number>;
    resolveIndex: () => number;
  };
  const staffNoteGroups = new Map<string, StaffNoteGroup>();
  const positionedEvents = activeScore.events
    .map((event) => ({ index: event.index, left: eventHorizontalPositions[event.index] }))
    .filter((position): position is { index: number; left: number } => position.left !== undefined)
    .sort((left, right) => left.left - right.left);
  const nearestEventIndex = (left: number): number => {
    if (positionedEvents.length === 0) return 0;
    let low = 0;
    let high = positionedEvents.length;
    while (low < high) {
      const middle = Math.floor((low + high) / 2);
      if (positionedEvents[middle]!.left < left) low = middle + 1;
      else high = middle;
    }
    const following = positionedEvents[Math.min(low, positionedEvents.length - 1)]!;
    const previous = positionedEvents[Math.max(0, low - 1)]!;
    return Math.abs(previous.left - left) <= Math.abs(following.left - left)
      ? previous.index
      : following.index;
  };
  for (const visual of visualSteps) {
    const exactIndices = eventIndicesByStep.get(visual.step) ?? [];
    const nearestIndex = nearestEventIndex(visual.left);
    const resolveIndex = (): number =>
      exactIndices.find((index) => index >= cursorIndex) ?? exactIndices[0] ?? nearestIndex;
    const expectedNotes = exactIndices.flatMap((index) => activeScore.events[index]?.notes ?? []);
    for (const note of visual.notes) {
      const samePart = note.partIndex === undefined
        ? expectedNotes
        : expectedNotes.filter((expected) => expected.partIndex === note.partIndex);
      const staffId = note.staffId;
      const sameStaff = staffId === undefined
        ? samePart
        : samePart.filter((expected) => expected.staff === staffId || expected.staff === staffId + 1);
      const choosePitch = (notes: TapEventDto["notes"]): number | undefined =>
        note.candidates.find((candidate) => notes.some((expected) => expected.midiPitch === candidate))
        ?? notes.find((expected) => note.candidates.some((candidate) => candidate % 12 === expected.midiPitch % 12))?.midiPitch;
      // OSMD's transposed pitch can differ by an octave from MusicXML's MIDI
      // pitch. Match the rendered note to its own part/staff first, then use
      // pitch class only as a last-resort bridge between the two conventions.
      const midiPitch = choosePitch(sameStaff) ?? choosePitch(samePart) ?? choosePitch(expectedNotes);
      if (midiPitch === undefined) continue;
      const staffChord = [...new Set((sameStaff.length > 0 ? sameStaff : [ { midiPitch } ]).map((expected) => expected.midiPitch))]
        .sort((left, right) => left - right);
      const groupKey = `${visual.step}:${note.partIndex ?? "part"}:${staffId ?? "staff"}`;
      const existing = staffNoteGroups.get(groupKey);
      if (existing) {
        existing.left = Math.min(existing.left, note.left);
        existing.right = Math.max(existing.right, note.left + note.width);
        existing.top = Math.min(existing.top, note.top);
        existing.bottom = Math.max(existing.bottom, note.top + note.height);
        staffChord.forEach((pitch) => existing.midiPitches.add(pitch));
      } else {
        staffNoteGroups.set(groupKey, {
          left: note.left,
          right: note.left + note.width,
          top: note.top,
          bottom: note.top + note.height,
          midiPitches: new Set(staffChord),
          resolveIndex,
        });
      }
    }
  }
  for (const group of staffNoteGroups.values()) {
    const midiPitches = [...group.midiPitches].sort((left, right) => left - right);
    const noteButton = document.createElement("button");
    noteButton.type = "button";
    noteButton.className = "note-target";
    noteButton.style.left = `${group.left - 6}px`;
    noteButton.style.top = `${group.top - 6}px`;
    noteButton.style.width = `${Math.max(18, group.right - group.left + 12)}px`;
    noteButton.style.height = `${Math.max(18, group.bottom - group.top + 12)}px`;
    noteButton.title = midiPitches.length > 1 ? "Play this staff chord" : `Play single note ${noteName(midiPitches[0]!)}`;
    noteButton.setAttribute("aria-label", noteButton.title);
    installAuditionHandlers(noteButton, group.resolveIndex, midiPitches);
    targetFragment.append(noteButton);
  }
  orderedMeasureHorizontalPositions = [...new Set(measureHorizontalPositions.values())]
    .sort((left, right) => left - right);
  const targetNodes = targetFragment.childElementCount;
  elements.scoreTargets.replaceChildren(targetFragment);
  moveOsmdCursor(highlightIndex);
  osmd.cursor.hide();
  return {
    visualSteps: visualSteps.length,
    targetNodes,
  };
}

type OsmdPitch = {
  Octave?: number;
  FundamentalNote?: number;
  AccidentalHalfTones?: number;
};

type OsmdSourceNote = {
  halfTone?: number;
  Pitch?: OsmdPitch;
  TransposedPitch?: OsmdPitch;
  ParentStaff?: {
    Id?: number;
    ParentInstrument?: { Id?: number };
  };
  isRest?: () => boolean;
};

function osmdMidiCandidates(sourceNote: OsmdSourceNote | undefined): number[] {
  const candidates: number[] = [];
  const addPitch = (pitch: OsmdPitch | undefined): void => {
    if (pitch?.Octave === undefined || pitch.FundamentalNote === undefined) return;
    candidates.push(Math.round((pitch.Octave + 1) * 12 + pitch.FundamentalNote + (pitch.AccidentalHalfTones ?? 0)));
  };
  addPitch(sourceNote?.TransposedPitch);
  addPitch(sourceNote?.Pitch);
  if (sourceNote?.halfTone !== undefined) {
    candidates.push(Math.round(sourceNote.halfTone), Math.round(sourceNote.halfTone + 12));
  }
  return [...new Set(candidates.filter((candidate) => candidate >= 0 && candidate <= 127))];
}

function createSoundIcon(): SVGSVGElement {
  const namespace = "http://www.w3.org/2000/svg";
  const icon = document.createElementNS(namespace, "svg");
  icon.setAttribute("viewBox", "0 0 24 24");
  icon.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(namespace, "path");
  path.setAttribute("d", "M4 10h4l5-4v12l-5-4H4zm12.5-3.2a5 5 0 0 1 0 10m2.5-13a9 9 0 0 1 0 16");
  icon.append(path);
  return icon;
}

function createStartIcon(): SVGSVGElement {
  const namespace = "http://www.w3.org/2000/svg";
  const icon = document.createElementNS(namespace, "svg");
  icon.setAttribute("viewBox", "0 0 24 24");
  icon.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(namespace, "path");
  path.setAttribute("d", "M12 3v13m0 0 6-6m-6 6-6-6");
  icon.append(path);
  return icon;
}

function createSliceControls(resolveIndex: () => number, measureNumber: string): HTMLDivElement {
  const controls = document.createElement("div");
  controls.className = "slice-controls";
  const play = document.createElement("button");
  play.type = "button";
  play.className = "slice-action play-chord";
  play.title = `Measure ${measureNumber}: Play single chord`;
  play.setAttribute("aria-label", play.title);
  play.append(createSoundIcon());
  installAuditionHandlers(play, resolveIndex);

  const start = document.createElement("button");
  start.type = "button";
  start.className = "slice-action start-here";
  start.title = `Measure ${measureNumber}: Start here`;
  start.setAttribute("aria-label", start.title);
  start.append(createStartIcon());
  const reposition = (event: Event): void => {
    event.preventDefault();
    event.stopPropagation();
    if (!score) return;
    const index = resolveIndex();
    void invokeSafe("set_cursor", { generation: score.generation, index }).then(() => {
      cursorIndex = index;
      highlightIndex = index;
      updatePosition();
      if (tapMode === "beat") resetBeatTap();
    });
  };
  start.addEventListener("pointerdown", reposition);
  start.addEventListener("click", (event) => {
    if (event.detail === 0) reposition(event);
  });
  controls.append(play, start);
  return controls;
}

function installAuditionHandlers(
  button: HTMLButtonElement,
  resolveIndex: () => number,
  midiPitches?: number[],
): void {
  auditionTargets.set(button, { resolveIndex, midiPitches });
}

type AuditionTarget = {
  resolveIndex: () => number;
  midiPitches?: number[];
};
const auditionTargets = new WeakMap<HTMLButtonElement, AuditionTarget>();
const auditionPointerHolds = new Map<number, PointerHold>();

function auditionTargetFromEvent(event: Event): { button: HTMLButtonElement; target: AuditionTarget } | null {
  const origin = event.target;
  if (!(origin instanceof Element)) return null;
  const button = origin.closest("button");
  if (!(button instanceof HTMLButtonElement)) return null;
  const target = auditionTargets.get(button);
  return target ? { button, target } : null;
}

app.addEventListener("pointerdown", (event) => {
  const match = auditionTargetFromEvent(event);
  if (!match) return;
  event.preventDefault();
  event.stopPropagation();
  const { button, target } = match;
  const token = `audition:${event.pointerId}:${crypto.randomUUID()}`;
  const index = target.resolveIndex();
  const noteCount = target.midiPitches?.length ?? score?.events[index]?.notes.length ?? 1;
  const finalOnsetDelayMs = rolledFinalOnsetDelay(noteCount, Number(elements.auditionRoll.value));
  button.setPointerCapture(event.pointerId);
  const down = auditionDown(token, index, target.midiPitches);
  auditionPointerHolds.set(event.pointerId, createPointerHold(token, down, finalOnsetDelayMs));
});

const releaseAuditionPointer = (event: PointerEvent): void => {
  const hold = auditionPointerHolds.get(event.pointerId);
  if (!hold) return;
  auditionPointerHolds.delete(event.pointerId);
  void releasePointerHold(hold);
};
app.addEventListener("pointerup", releaseAuditionPointer);
app.addEventListener("pointercancel", releaseAuditionPointer);
app.addEventListener("lostpointercapture", releaseAuditionPointer);
app.addEventListener("click", (event) => {
  if (event.detail !== 0) return;
  const match = auditionTargetFromEvent(event);
  if (!match) return;
  const keyboardToken = `audition-keyboard:${crypto.randomUUID()}`;
  void auditionDown(
    keyboardToken,
    match.target.resolveIndex(),
    match.target.midiPitches,
  ).then(() => performUp(keyboardToken));
});

async function auditionDown(token: string, index: number, midiPitches?: number[]): Promise<void> {
  if (!score || heldTokens.has(token)) return;
  heldTokens.add(token);
  const command = midiPitches === undefined ? "audition_event" : midiPitches.length === 1 ? "audition_note" : "audition_chord";
  const pending = invokeSafe<void>(command, {
    generation: score.generation,
    index,
    ...(midiPitches === undefined ? {} : midiPitches.length === 1 ? { midiPitch: midiPitches[0] } : { midiPitches }),
    token,
    velocity: DEFAULT_VELOCITY,
  });
  pendingDowns.set(token, pending);
  try {
    await pending;
  } catch {
    heldTokens.delete(token);
  } finally {
    if (pendingDowns.get(token) === pending) pendingDowns.delete(token);
  }
}

async function performDown(token: string, velocity = DEFAULT_VELOCITY): Promise<void> {
  if (tapMode === "beat" && !token.startsWith("audition:")) {
    beatTapDown(token);
    return;
  }
  if (!score || heldTokens.has(token)) return;
  heldTokens.add(token);
  const started = performance.now();
  const pending = invokeSafe<void>("performance_input_down", { token, velocity });
  pendingDowns.set(token, pending);
  try {
    await pending;
    lastUiNativeRoundTripMs = performance.now() - started;
  } catch {
    heldTokens.delete(token);
    return;
  } finally {
    if (pendingDowns.get(token) === pending) pendingDowns.delete(token);
  }
}

async function performUp(token: string): Promise<void> {
  if (tapMode === "beat" && !token.startsWith("audition:")) {
    heldTokens.delete(token);
    return;
  }
  if (!heldTokens.delete(token)) return;
  // Preserve physical ordering even for a click shorter than one IPC round
  // trip. This wait affects note-off bookkeeping only; note-on was dispatched
  // immediately by performDown.
  const pending = pendingDowns.get(token);
  if (pending) {
    try {
      await pending;
    } catch {
      return;
    }
  }
  try {
    await invokeSafe("release_input", { token });
  } catch {
    // invokeSafe has already surfaced the fault; do not create an unhandled
    // rejection from DOM release handlers.
  }
}

function renderParts(): void {
  if (!score) return;
  const enabledParts = score.parts.filter((part) => part.enabled).length;
  elements.partsValue.textContent = enabledParts === score.parts.length
    ? "All parts"
    : `${enabledParts} of ${score.parts.length}`;
  elements.partsList.replaceChildren();
  score.parts.forEach((part) => {
    const label = document.createElement("label");
    label.className = "check-row";
    label.title = part.name;
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = part.enabled;
    input.setAttribute("aria-label", `Staff ${part.name}`);
    input.addEventListener("change", async () => {
      if (!score) return;
      const previousEvent = score.events[cursorIndex];
      const previousScrollLeft = elements.scoreScroll.scrollLeft;
      try {
        const updated = await invokeSafe<LoadedScore>("set_part_enabled", {
          generation: score.generation,
          partId: part.id,
          enabled: input.checked,
        });
        const restoredIndex = indexForPreservedEvent(updated.events, previousEvent);
        await displayScore(updated, { event: previousEvent, scrollLeft: previousScrollLeft });
        await invokeSafe("set_cursor", { generation: updated.generation, index: restoredIndex });
      } catch {
        input.checked = part.enabled;
      }
    });
    label.append(input, document.createTextNode(part.name));
    elements.partsList.append(label);
  });
}

function populateSelect(select: HTMLSelectElement, devices: DeviceDto[], offLabel?: string): void {
  select.replaceChildren();
  if (offLabel) select.add(new Option(offLabel, ""));
  for (const device of devices) select.add(new Option(`${device.name}${device.isDefault ? " (default)" : ""}`, device.id));
  fitSelect(select);
}

function populateAudioSelect(devices: DeviceDto[]): void {
  elements.audioOutput.replaceChildren();
  elements.audioOutput.add(new Option("System default", ""));
  for (const device of devices) {
    elements.audioOutput.add(
      new Option(`${device.name}${device.isDefault ? " (default)" : ""}`, device.id),
    );
  }
  const separator = new Option("────────────", "");
  separator.disabled = true;
  elements.audioOutput.add(separator);
  elements.audioOutput.add(new Option("↻ Reload audio & MIDI devices", RELOAD_AUDIO_SYSTEMS_VALUE));
  const selectionStillExists = [...elements.audioOutput.options]
    .some((option) => option.value === selectedAudioDeviceId && !option.disabled);
  elements.audioOutput.value = selectionStillExists ? selectedAudioDeviceId : "";
  if (!selectionStillExists) selectedAudioDeviceId = "";
  fitSelect(elements.audioOutput);
}

function fitSelect(select: HTMLSelectElement): void {
  const text = select.selectedOptions[0]?.textContent ?? "";
  select.style.width = `${Math.max(38, Math.min(190, text.length * 7 + 22))}px`;
}

async function refreshDevices(): Promise<void> {
  const [audioResult, midiResult, diagnosticsResult] = await Promise.allSettled([
    invoke<DeviceDto[]>("audio_devices"),
    invoke<MidiPortsDto>("midi_ports"),
    invoke<DiagnosticsDto>("diagnostics"),
  ]);
  const errors: string[] = [];
  if (audioResult.status === "fulfilled") {
    populateAudioSelect(audioResult.value);
  } else {
    errors.push(`Audio device discovery failed: ${String(audioResult.reason)}`);
  }
  if (midiResult.status === "fulfilled") {
    const midiPorts = midiResult.value;
    populateSelect(elements.midiInput, midiPorts.inputs, "Off");
    populateSelect(elements.midiOutput, midiPorts.outputs, "Off");
    if (midiPorts.selectedInput) elements.midiInput.value = midiPorts.selectedInput;
    if (midiPorts.selectedOutput) elements.midiOutput.value = midiPorts.selectedOutput;
  } else {
    errors.push(`MIDI device discovery failed: ${String(midiResult.reason)}`);
  }
  if (diagnosticsResult.status === "fulfilled") {
    showDiagnostics(diagnosticsResult.value);
    elements.diagnosticsButton.classList.toggle("not-ready", !diagnosticsResult.value.ready);
    if (isWebBuild()) {
      setStatus(
        diagnosticsResult.value.ready ? "ready" : "fault",
        diagnosticsResult.value.ready ? "Browser audio ready" : "Audio needs attention",
      );
    }
  } else {
    errors.push(`Diagnostics failed: ${String(diagnosticsResult.reason)}`);
  }
  if (errors.length > 0) {
    elements.diagnosticsButton.classList.add("not-ready");
    toast(errors.join(" "), "error");
  }
}

async function reloadAudioSystems(): Promise<void> {
  elements.audioOutput.disabled = true;
  setStatus("loading", "Reloading devices…");
  try {
    await invokeSafe("reload_audio_systems");
    toast("Audio and MIDI devices reloaded.", "info");
  } finally {
    await refreshDevices();
    elements.audioOutput.disabled = false;
    if (lastDiagnostics?.ready) setStatus("ready", "Audio ready");
    else setStatus("fault", "Audio needs attention");
  }
}

function showDiagnostics(diagnostics: DiagnosticsDto): void {
  lastDiagnostics = diagnostics;
  elements.diagnosticsValue.textContent = diagnostics.ready ? "Ready" : "Needs attention";
  const rows: Array<[string, string]> = [
    ["State", diagnostics.ready ? "Ready" : diagnostics.message ?? "Unavailable"],
    ["Backend", diagnostics.audioBackend],
    ["Mode", isWebBuild() ? "Browser Web Audio" : diagnostics.asioStream ? "ASIO low latency" : "Shared low latency"],
    ["Output", diagnostics.outputDevice],
    ["Format", `${diagnostics.sampleRate.toLocaleString()} Hz · ${diagnostics.bufferFrames} frames`],
    ["Est. buffer", `${diagnostics.estimatedLatencyMs.toFixed(1)} ms`],
    ["Active voices", String(diagnostics.activeVoices)],
    ["Underruns", String(diagnostics.callbackUnderruns)],
    ["Backend errors", String(diagnostics.backendErrors)],
    ["Late commands", String(diagnostics.lateCommands)],
    ["Invalid buffers", String(diagnostics.invalidAudioBuffers)],
    ["Voice steals", String(diagnostics.voiceSteals)],
    ["Queue overflow", String(diagnostics.queueOverflows)],
    ["Direct WASAPI", isWebBuild() ? "Not available in browser" : diagnostics.directWasapiStream ? "Yes" : "No (CPAL fallback)"],
    ["Native ASIO", isWebBuild() ? "Not available in browser" : diagnostics.asioStream ? "Yes" : "No"],
    ["MIDI in", diagnostics.midiInput ?? "Off"],
    ["MIDI out", diagnostics.midiOutput ?? "Off"],
  ];
  if (lastUiNativeRoundTripMs !== null) {
    const callbackPeriodMs = diagnostics.sampleRate > 0
      ? diagnostics.bufferFrames * 1_000 / diagnostics.sampleRate
      : 0;
    rows.splice(5, 0,
      ["Last UI → native reply", `${lastUiNativeRoundTripMs.toFixed(2)} ms (enqueue is earlier)`],
      ["UI → endpoint bound", `< ${(lastUiNativeRoundTripMs + callbackPeriodMs + diagnostics.estimatedLatencyMs).toFixed(1)} ms`],
    );
  }
  if (scorePerformance) {
    const formatMs = (value: number | undefined): string =>
      value === undefined ? "â€”" : `${value.toFixed(1)} ms`;
    rows.push(
      ["Score core load", formatMs(scorePerformance.coreLoadMs)],
      ["OSMD parse", formatMs(scorePerformance.osmdLoadMs)],
      ["OSMD engraving", formatMs(scorePerformance.osmdRenderMs)],
      ["Interaction targets", formatMs(scorePerformance.targetBuildMs)],
      ["Score ready total", formatMs(scorePerformance.displayTotalMs)],
      [
        "Score UI size",
        `${scorePerformance.visualSteps?.toLocaleString() ?? "â€”"} steps Â· ${scorePerformance.targetNodes?.toLocaleString() ?? "â€”"} targets`,
      ],
    );
  }
  if (diagnostics.outputDevice.toUpperCase().includes("QUAD-CAPTURE")) {
    rows.push([
      "Driver buffer",
      "QUAD-CAPTURE Control Panel → Driver → Driver Settings: lower Audio Buffer Size",
    ]);
  }
  if (diagnostics.wasapiPeriods) {
    const periods = diagnostics.wasapiPeriods;
    rows.splice(4, 0, [
      "WASAPI periods",
      `${periods.minimumFrames} min · ${periods.defaultFrames} default · ${periods.maximumFrames} max`,
    ]);
  }
  if (diagnostics.midiOutputError) {
    rows.push(["MIDI OUT error", diagnostics.midiOutputError]);
  }
  elements.diagnostics.replaceChildren();
  const heading = document.createElement("h3");
  heading.textContent = "Live diagnostics";
  elements.diagnostics.append(heading);
  for (const [key, value] of rows) {
    const row = document.createElement("div");
    const label = document.createElement("span");
    label.textContent = key;
    const result = document.createElement("b");
    result.textContent = value;
    row.append(label, result);
    elements.diagnostics.append(row);
  }
}

async function refreshDiagnostics(): Promise<void> {
  try {
    const diagnostics = await invoke<DiagnosticsDto>("diagnostics");
    const previousDiagnostics = lastDiagnostics;
    const previousMidiError = previousDiagnostics?.midiOutputError;
    showDiagnostics(diagnostics);
    if (diagnostics.midiOutputError && diagnostics.midiOutputError !== previousMidiError) {
      toast(diagnostics.midiOutputError, "warning");
    }
    if (
      !diagnostics.ready
      && (previousDiagnostics?.ready !== false || diagnostics.message !== previousDiagnostics.message)
    ) {
      toast(
        `${diagnostics.message ?? "The audio output is unavailable."} Reload devices from Audio Out.`,
        "error",
      );
    }
    elements.diagnosticsButton.classList.toggle("not-ready", !diagnostics.ready);
  } catch {
    elements.diagnosticsButton.classList.add("not-ready");
    elements.diagnosticsValue.textContent = "Unavailable";
  }
}

async function installListeners(): Promise<void> {
  unlisteners.push(
    await listen<CoreEvent>("performance-event", ({ payload }) => {
      if (payload.type === "fault") {
        elements.diagnosticsButton.classList.add("not-ready");
        toast(payload.message, "error");
        return;
      }
      if (!score || payload.generation !== score.generation) return;
      if (payload.type === "cursor") {
        cursorIndex = payload.index;
        highlightIndex = payload.playedIndex ?? payload.index;
        if (payload.playedIndex !== undefined) mostRecentChordIndex = payload.playedIndex;
      }
      if (payload.type === "ended") toast("End of score", "info");
      updatePosition();
    }),
    await listen<DiagnosticsDto>("audio-diagnostics", ({ payload }) => showDiagnostics(payload)),
    await listen<string>("audio-lifecycle-error", ({ payload }) => {
      elements.diagnosticsButton.classList.add("not-ready");
      toast(`${payload} Reload devices from Audio Out.`, "error");
    }),
    await listen<BeatMidiInput>("beat-midi-input", ({ payload }) => {
      if (payload.type === "down") void performDown(payload.token, payload.velocity);
      else void performUp(payload.token);
    }),
  );
}

elements.open.addEventListener("click", () => void chooseScore());
elements.emptyOpen.addEventListener("click", () => void chooseScore());
elements.demoOpen.addEventListener("click", () => void loadDemoScore());
let helpPreviousFocus: HTMLElement | null = null;
function closeHelp(): void {
  elements.helpOverlay.classList.add("hidden");
  elements.helpButton.setAttribute("aria-expanded", "false");
  helpPreviousFocus?.focus();
  helpPreviousFocus = null;
}
elements.helpButton.addEventListener("click", () => {
  helpPreviousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  elements.helpOverlay.classList.remove("hidden");
  elements.helpButton.setAttribute("aria-expanded", "true");
  elements.helpClose.focus();
});
elements.helpClose.addEventListener("click", closeHelp);
elements.helpDone.addEventListener("click", closeHelp);
elements.helpOverlay.addEventListener("pointerdown", (event) => {
  if (event.target === elements.helpOverlay) closeHelp();
});
elements.helpOverlay.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeHelp();
    return;
  }

  if (event.key === "Tab") {
    const focusable = [...elements.helpOverlay.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
    )].filter((element) => !element.hasAttribute("hidden"));
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (first && last) {
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  }

  // Help owns its keyboard interaction while modal so score navigation,
  // replay, and conducting shortcuts cannot fire behind it.
  event.stopPropagation();
});
elements.panic.addEventListener("click", async () => {
  const nextMode = !midiFreePlay;
  await invokeSafe("set_midi_free_play", { enabled: nextMode });
  midiFreePlay = nextMode;
  updateMidiFreePlayButton();
});
const tapPointerHolds = new Map<number, PointerHold>();
elements.tap.addEventListener("pointerdown", (event) => {
  elements.tap.setPointerCapture(event.pointerId);
  const token = `pointer:${event.pointerId}:${crypto.randomUUID()}`;
  const noteCount = score?.events[cursorIndex]?.notes.length ?? 1;
  const finalOnsetDelayMs = tapMode === "rhythm"
    ? rolledFinalOnsetDelay(noteCount, Number(elements.regularRoll.value))
    : 0;
  const down = performDown(token);
  tapPointerHolds.set(
    event.pointerId,
    createPointerHold(token, down, finalOnsetDelayMs, tapMode === "rhythm" ? MINIMUM_POINTER_NOTE_HOLD_MS : 0),
  );
});
const releaseTapPointer = (event: PointerEvent): void => {
  const hold = tapPointerHolds.get(event.pointerId);
  if (!hold) return;
  tapPointerHolds.delete(event.pointerId);
  void releasePointerHold(hold);
};
elements.tap.addEventListener("pointerup", releaseTapPointer);
elements.tap.addEventListener("pointercancel", releaseTapPointer);
elements.tap.addEventListener("lostpointercapture", releaseTapPointer);
elements.tap.addEventListener("click", (event) => {
  if (event.detail !== 0) return;
  const token = `tap-keyboard:${crypto.randomUUID()}`;
  void performDown(token).then(() => performUp(token));
});
elements.back.addEventListener("click", async () => {
  if (!score) return;
  const index = Math.max(0, cursorIndex - 1);
  await invokeSafe("set_cursor", { generation: score.generation, index });
  cursorIndex = index;
  highlightIndex = index;
  updatePosition();
  if (tapMode === "beat") resetBeatTap();
});
elements.forward.addEventListener("click", async () => {
  if (!score) return;
  const index = Math.min(score.events.length - 1, cursorIndex + 1);
  await invokeSafe("set_cursor", { generation: score.generation, index });
  cursorIndex = index;
  highlightIndex = index;
  updatePosition();
  if (tapMode === "beat") resetBeatTap();
});

const tapKeyCodes = new Set([
  "Enter",
  "ShiftLeft",
  "ShiftRight",
  ...Array.from({ length: 26 }, (_, index) => `Key${String.fromCharCode(65 + index)}`),
  ...Array.from({ length: 10 }, (_, index) => `Digit${index}`),
  "Comma",
  "Period",
  "Semicolon",
  "BracketLeft",
  "BracketRight",
  "Quote",
  "Equal",
  "Minus",
  "Backquote",
]);

document.addEventListener("keydown", (event) => {
  if (event.code === "ArrowLeft") {
    event.preventDefault();
    elements.back.click();
    return;
  }
  if (event.code === "ArrowRight") {
    event.preventDefault();
    elements.forward.click();
    return;
  }
  if (event.code === "Escape") {
    event.preventDefault();
    if (!elements.helpOverlay.classList.contains("hidden")) {
      closeHelp();
      return;
    }
    void invokeSafe("panic");
    return;
  }
  if (event.code === "Space" && !event.repeat) {
    event.preventDefault();
    if (mostRecentChordIndex !== null) {
      void auditionDown("audition:key:Space", mostRecentChordIndex);
    }
    return;
  }
  if (tapKeyCodes.has(event.code) && !event.repeat) {
    event.preventDefault();
    void performDown(`key:${event.code}`);
  }
});
document.addEventListener("keyup", (event) => {
  if (event.code === "Space") {
    event.preventDefault();
    void performUp("audition:key:Space");
    return;
  }
  if (tapKeyCodes.has(event.code)) {
    event.preventDefault();
    void performUp(`key:${event.code}`);
  }
});
window.addEventListener("blur", () => {
  for (const token of [...heldTokens]) void performUp(token);
});
window.addEventListener("beforeunload", () => {
  unlisteners.forEach((unlisten) => unlisten());
  void invoke("panic").catch(() => undefined);
});

elements.volume.addEventListener("input", () => {
  elements.volumeValue.value = `${elements.volume.value}%`;
  void invoke("set_volume", { value: Number(elements.volume.value) / 100 }).catch(() => undefined);
});
const updateRollDelays = (): void => {
  const regularMs = Number(elements.regularRoll.value);
  const auditionMs = Number(elements.auditionRoll.value);
  elements.regularRollValue.value = `${regularMs} ms`;
  elements.auditionRollValue.value = `${auditionMs} ms`;
  void invokeSafe("set_roll_delays", { regularMs, auditionMs });
};
elements.regularRoll.addEventListener("input", updateRollDelays);
elements.auditionRoll.addEventListener("input", updateRollDelays);
function togglePopover(button: HTMLElement, popover: HTMLElement): void {
  const wasHidden = popover.classList.contains("hidden");
  popover.classList.toggle("hidden", !wasHidden);
  if (!wasHidden) return;

  const buttonBounds = button.getBoundingClientRect();
  const popoverWidth = popover.getBoundingClientRect().width;
  const left = Math.max(8, Math.min(buttonBounds.left, window.innerWidth - popoverWidth - 8));
  popover.style.left = `${left}px`;
  popover.style.right = "auto";
  popover.style.top = `${buttonBounds.bottom + 8}px`;
}

document.addEventListener("pointerdown", (event) => {
  const target = event.target;
  if (!(target instanceof Node)) return;
  for (const [button, popover] of [
    [elements.partsButton, elements.partsPopover],
    [elements.diagnosticsButton, elements.diagnostics],
  ] as const) {
    if (!popover.classList.contains("hidden") && !popover.contains(target) && !button.contains(target)) {
      popover.classList.add("hidden");
    }
  }
});

elements.partsButton.addEventListener("click", () => togglePopover(elements.partsButton, elements.partsPopover));
elements.audioOutput.addEventListener("change", async () => {
  const requested = elements.audioOutput.value;
  if (requested === RELOAD_AUDIO_SYSTEMS_VALUE) {
    elements.audioOutput.value = selectedAudioDeviceId;
    fitSelect(elements.audioOutput);
    try {
      await reloadAudioSystems();
    } catch {
      // invokeSafe and refreshDevices already surfaced the relevant errors.
    }
    return;
  }
  const previous = selectedAudioDeviceId;
  try {
    await invokeSafe("set_audio_device", { id: requested });
    selectedAudioDeviceId = requested;
    fitSelect(elements.audioOutput);
  } catch {
    elements.audioOutput.value = previous;
    fitSelect(elements.audioOutput);
  }
});
elements.instrument.addEventListener("change", () => {
  fitSelect(elements.instrument);
  void invokeSafe("set_instrument", { instrument: elements.instrument.value });
});
elements.midiInput.addEventListener("change", () => {
  fitSelect(elements.midiInput);
  void invokeSafe("set_midi_input", { id: elements.midiInput.value || null })
    .then(() => {
      if (isWebBuild()) return refreshDevices();
    });
});
elements.midiOutput.addEventListener("change", () => {
  fitSelect(elements.midiOutput);
  void invokeSafe("set_midi_output", { id: elements.midiOutput.value || null })
    .then(() => {
      if (isWebBuild()) return refreshDevices();
    });
});
elements.tapMode.addEventListener("change", () => {
  tapMode = elements.tapMode.value === "beat" ? "beat" : "rhythm";
  clearBeatTimers();
  void invokeSafe("set_tap_mode", { beat: tapMode === "beat" });
  if (tapMode === "beat") resetBeatTap();
  else {
    lastPressedBeatIndex = null;
    activeBeatVisualIndex = null;
    updateTapButtonLabel();
    updatePosition();
  }
});
elements.diagnosticsButton.addEventListener("click", () => {
  if (lastDiagnostics) showDiagnostics(lastDiagnostics);
  togglePopover(elements.diagnosticsButton, elements.diagnostics);
});
let zoomRenderTimer: number | null = null;

function requestZoomRender(immediate: boolean): void {
  if (!osmd) return;
  if (zoomRenderTimer !== null) window.clearTimeout(zoomRenderTimer);
  if (immediate && zoom === renderedZoom) {
    zoomRenderTimer = null;
    return;
  }
  const render = (): void => {
    zoomRenderTimer = null;
    void scheduleOsmdRender("zoom").catch((error: unknown) => {
      toast(`The score could not be resized: ${String(error)}`, "error");
    });
  };
  if (immediate) render();
  else zoomRenderTimer = window.setTimeout(render, 140);
}

function setZoomPercent(percent: number, immediate = false): void {
  const snapped = ZOOM_STEPS.reduce((nearest, candidate) =>
    Math.abs(candidate - percent) < Math.abs(nearest - percent) ? candidate : nearest,
  );
  const nextZoom = snapped / 100;
  elements.zoomRange.value = String(snapped);
  elements.zoomValue.value = `${snapped}%`;
  if (nextZoom === zoom) return;
  zoom = nextZoom;
  requestZoomRender(immediate);
}

function moveZoom(direction: -1 | 1): void {
  const current = Math.round(zoom * 100);
  const index = ZOOM_STEPS.findIndex((step) => step >= current);
  const currentIndex = index >= 0 && ZOOM_STEPS[index] === current ? index : Math.max(0, index - 1);
  setZoomPercent(
    ZOOM_STEPS[Math.max(0, Math.min(ZOOM_STEPS.length - 1, currentIndex + direction))]!,
    true,
  );
}

elements.zoomOut.addEventListener("click", () => moveZoom(-1));
elements.zoomIn.addEventListener("click", () => moveZoom(1));
elements.zoomRange.addEventListener("input", () => setZoomPercent(Number(elements.zoomRange.value)));
elements.zoomRange.addEventListener("change", () => requestZoomRender(true));

void installListeners().then(refreshDevices).catch((error: unknown) => {
  setStatus("fault", "Core unavailable");
  toast(String(error), "error");
});
window.setInterval(() => void refreshDiagnostics(), 1_000);

if (isWebBuild()) {
  document.documentElement.classList.add("web-build");
  document.getElementById("web-edition-badge")?.classList.remove("hidden");
  elements.instrument.replaceChildren(
    new Option("Web piano", "piano", true, true),
    new Option("Synthesizer", "synth"),
  );
  fitSelect(elements.instrument);
  void invokeSafe("set_instrument", { instrument: "piano" });
  document.getElementById("instrument-help")!.textContent =
    "The browser edition includes a lightweight Web piano and synthesizer.";
  document.querySelector(".brand")?.setAttribute(
    "title",
    "Browser edition: scores and performances remain on this device",
  );
  if (import.meta.env.PROD && location.protocol !== "file:" && "serviceWorker" in navigator) {
    window.addEventListener("load", () => {
      void navigator.serviceWorker.register(`${import.meta.env.BASE_URL}sw.js`);
    });
  }
}

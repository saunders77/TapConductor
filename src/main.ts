import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { OpenSheetMusicDisplay } from "opensheetmusicdisplay";
import { autoFollowTarget } from "./auto-follow";
import { planBeatInterval, rationalValue } from "./beat-scheduler";
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
const fingerIconUrl = new URL("../assets/finger transparent-background.png", import.meta.url).href;

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
      <div class="status-pill loading" id="status-pill"><span></span><b>Starting audio…</b></div>
    </header>

    <section class="control-deck" aria-label="Performance controls">
      <label class="field">
        <span>Audio out</span>
        <select id="audio-output" aria-label="Audio output"><option>System default</option></select>
      </label>
      <label class="field">
        <span>MIDI in</span>
        <select id="midi-input" aria-label="MIDI input"><option value="">Off</option></select>
      </label>
      <label class="field">
        <span>MIDI out</span>
        <select id="midi-output" aria-label="MIDI output"><option value="">Off</option></select>
      </label>
      <label class="field tap-mode-field">
        <span>Tap mode</span>
        <select id="tap-mode" aria-label="Tap mode">
          <option value="rhythm">Rhythm Tap</option>
          <option value="beat">Beat Tap</option>
        </select>
      </label>
        <label class="range-field">
        <span>Vol <output id="volume-value">100%</output></span>
        <input id="volume" type="range" min="0" max="100" value="100" />
      </label>
      <label class="range-field delay-field">
        <span>Roll <output id="regular-roll-value">0 ms</output></span>
        <input id="regular-roll" type="range" min="0" max="250" value="0" />
      </label>
      <label class="range-field delay-field">
        <span>Chord <output id="audition-roll-value">120 ms</output></span>
        <input id="audition-roll" type="range" min="0" max="250" value="120" />
      </label>
      <button id="parts-button" class="deck-button" type="button">Staves</button>
        <button id="diagnostics-button" class="deck-button diagnostics-button" type="button" title="Audio diagnostics" aria-label="Audio diagnostics">⚙</button>
        <button id="panic-button" class="panic-button" type="button" title="Play MIDI input directly" aria-label="Play MIDI input directly">■</button>
    </section>

    <aside id="parts-popover" class="popover hidden" aria-label="Staves">
      <h3>Staves</h3><p>Choose which staves are included when you tap.</p>
      <div id="parts-list"></div>
    </aside>

    <aside id="diagnostics-popover" class="popover diagnostics hidden" aria-label="Audio diagnostics"></aside>

    <main class="workspace">
      <section class="score-panel" aria-label="Musical score">
        <div class="score-toolbar">
          <div class="score-help">Tap to advance · <span aria-hidden="true">◖)</span> play single chord · <span aria-hidden="true">▼</span> start here · select a note to play it</div>
          <div class="zoom-controls">
            <button id="zoom-out" type="button" aria-label="Zoom out">−</button>
            <span class="zoom-label">Zoom</span><output id="zoom-value">90%</output>
            <button id="zoom-in" type="button" aria-label="Zoom in">＋</button>
            <input id="zoom-range" type="range" min="50" max="175" value="90" step="1" aria-label="Zoom" />
          </div>
        </div>
        <div id="score-scroll" class="score-scroll">
          <div id="empty-state" class="empty-state">
            <div class="empty-score" aria-hidden="true">
              <span>𝄞</span><i></i><i></i><i></i><i></i><i></i>
            </div>
            <h1>Put the score under your fingers.</h1>
            <p>Open MusicXML, compressed MXL, or MIDI. Every tap plays the next written note or chord.</p>
            <button id="empty-open" class="primary-button large" type="button">Open a score</button>
            <small>Space, Enter, mouse, or a MIDI keyboard can conduct.</small>
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
          <small>Space / Enter</small>
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
  emptyOpen: byId<HTMLButtonElement>("empty-open"),
  status: byId("status-pill"),
  audioOutput: byId<HTMLSelectElement>("audio-output"),
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
  partsList: byId("parts-list"),
  partsPopover: byId("parts-popover"),
  diagnosticsButton: byId<HTMLButtonElement>("diagnostics-button"),
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
  [elements.volume.parentElement, elements.regularRoll.parentElement, elements.auditionRoll.parentElement, zoomControls]
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
let zoom = 0.9;
const ZOOM_STEPS = [50, 75, 90, 100, 110, 125, 150, 175];
let osmd: OpenSheetMusicDisplay | null = null;
let osmdEventSteps: number[] = [];
let osmdBeatSteps: number[] = [];
let osmdCurrentStep = 0;
let eventHorizontalPositions: number[] = [];
let beatHorizontalPositions: number[] = [];
let measureHorizontalPositions = new Map<number, number>();
let lastDiagnostics: DiagnosticsDto | null = null;
let lastUiNativeRoundTripMs: number | null = null;
let unlisteners: UnlistenFn[] = [];
const heldTokens = new Set<string>();
let midiFreePlay = false;

function updateMidiFreePlayButton(): void {
  elements.panic.classList.toggle("midi-free-play", midiFreePlay);
  elements.panic.textContent = midiFreePlay ? "👇" : "■";
  elements.panic.title = midiFreePlay
    ? "Return to score-following MIDI input"
    : "Play MIDI input directly";
  elements.panic.setAttribute("aria-label", elements.panic.title);
}
const pendingDowns = new Map<string, Promise<void>>();
const DEFAULT_VELOCITY = 96;
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
  document.querySelectorAll<HTMLElement>("[data-event-indices], [data-event-index], [data-beat-index]").forEach((node) => {
    const eventIndices = (node.dataset.eventIndices ?? node.dataset.eventIndex ?? "")
      .split(",")
      .filter(Boolean)
      .map(Number);
    const nodeBeatIndex = node.dataset.beatIndex === undefined ? null : Number(node.dataset.beatIndex);
    const isCurrent = displayedBeatIndex === null
      ? eventIndices.includes(highlightIndex)
      : nodeBeatIndex === displayedBeatIndex;
    node.classList.toggle("current", isCurrent);
  });
}

function autoFollowPosition(sliceLeft: number | undefined): void {
  if (sliceLeft === undefined) return;
  const orderedBars = [...new Set(measureHorizontalPositions.values())].sort((left, right) => left - right);
  let barWidth = 180 * zoom;
  let barIndex = -1;
  for (let index = 0; index < orderedBars.length; index += 1) {
    if (orderedBars[index]! <= sliceLeft + 1) barIndex = index;
  }
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
    osmd.cursor.show();
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
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Musical scores", extensions: ["musicxml", "xml", "mxl", "mid", "midi"] }],
  });
  if (!path) return;
  setStatus("loading", "Loading score…");
  elements.open.disabled = true;
  let loaded: LoadedScore | null = null;
  try {
    loaded = await invokeSafe<LoadedScore>("load_score", { path });
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
    elements.open.disabled = false;
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
  cursorIndex = preserveView ? Math.min(preservedCursor, Math.max(0, loaded.events.length - 1)) : 0;
  highlightIndex = cursorIndex;
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
      cursorsOptions: [{ type: 1, color: "#75ffb3", alpha: 0.8, follow: false }],
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
    await osmd.load(loaded.musicXml);
    renderOsmd();
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
  setStatus("ready", "Ready");
  loaded.warnings.forEach((warning) => toast(warning, "warning"));
}

function renderOsmd(): void {
  if (!osmd) return;
  ensureBottomControls();
  osmd.Zoom = zoom;
  osmd.render();
  osmd.cursor.show();
  window.setTimeout(() => {
    const contentWidth = Math.max(elements.osmd.scrollWidth, elements.osmd.getBoundingClientRect().width);
    if (contentWidth > 0) {
      elements.scoreStage.style.width = `${Math.ceil(contentWidth + 68)}px`;
    }
    buildScoreTargets();
  }, 0);
}

function renderMidiRoll(events: TapEventDto[]): void {
  const wrapper = document.createElement("div");
  wrapper.className = "midi-roll";
  for (const event of events) {
    const eventCard = document.createElement("div");
    eventCard.className = "midi-event";
    eventCard.dataset.eventIndex = String(event.index);
    eventCard.dataset.eventIndices = String(event.index);
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
  });
}

function buildScoreTargets(): void {
  if (!osmd || !score) return;
  const activeScore = score;
  elements.scoreTargets.replaceChildren();
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

  const rationalMatches = (
    measureIndex: number,
    numerator: number,
    denominator: number,
    step: VisualStep,
  ): boolean => {
    if (measureIndex !== step.measureIndex || denominator === 0 || step.denominator === 0) return false;
    const eventNumerator = BigInt(numerator);
    const eventDenominator = BigInt(denominator);
    const visualNumerator = BigInt(step.numerator);
    const visualDenominator = BigInt(step.denominator);
    const left = eventNumerator * visualDenominator;
    const right = visualNumerator * eventDenominator;
    // The normalized core uses MusicXML division units (quarter note = 1),
    // while OSMD versions commonly expose whole-note fractions. Accept the
    // direct representation too so this remains robust across OSMD changes.
    return left === right || left === right * 4n;
  };

  osmdBeatSteps = [];
  beatHorizontalPositions = [];
  activeScore.beats.forEach((beat, index) => {
    const beatOffsetNumerator = beat.beatIndex * 4;
    const visual = visualSteps.find((step) =>
      rationalMatches(beat.measureIndex, beatOffsetNumerator, beat.beatType, step),
    );
    if (!visual) return;
    osmdBeatSteps[index] = visual.step;
    beatHorizontalPositions[index] = visual.anchorLeft;
    const ghost = document.createElement("div");
    ghost.className = "slice-ghost beat-ghost";
    ghost.dataset.beatIndex = String(index);
    ghost.style.left = `${visual.anchorLeft - 10}px`;
    ghost.style.top = `${visual.top}px`;
    ghost.style.width = "20px";
    ghost.style.height = `${visual.height}px`;
    elements.scoreTargets.append(ghost);
  });

  const groupedTargets = new Map<string, { eventIndices: number[]; visual: VisualStep; measureNumber: string }>();
  const eventIndicesByStep = new Map<number, number[]>();
  osmdEventSteps = [];
  for (const event of activeScore.events) {
    const candidates = visualSteps.filter((step) =>
      rationalMatches(event.measureIndex, event.offset.numerator, event.offset.denominator, step),
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
    if (target.visual.notes.length > 0) {
      const noteLeft = Math.min(...target.visual.notes.map((note) => note.left));
      const noteRight = Math.max(...target.visual.notes.map((note) => note.left + note.width));
      const noteTop = Math.min(...target.visual.notes.map((note) => note.top));
      const noteBottom = Math.max(...target.visual.notes.map((note) => note.top + note.height));
      const ghost = document.createElement("div");
      ghost.className = "slice-ghost";
      ghost.dataset.eventIndices = target.eventIndices.join(",");
      ghost.style.left = `${noteLeft - 10}px`;
      ghost.style.top = `${target.visual.top}px`;
      ghost.style.width = `${Math.max(28, noteRight - noteLeft + 20)}px`;
      ghost.style.height = `${Math.max(noteBottom - noteTop + 28, target.visual.height)}px`;
      elements.scoreTargets.append(ghost);
    }
    const controls = createSliceControls(resolveIndex, target.measureNumber);
    controls.dataset.eventIndices = target.eventIndices.join(",");
    controls.style.left = `${target.visual.anchorLeft}px`;
    controls.style.top = `${Math.max(4, target.visual.top - 46)}px`;
    elements.scoreTargets.append(controls);
  }

  const seenNoteheads = new Set<string>();
  for (const visual of visualSteps) {
    const exactIndices = eventIndicesByStep.get(visual.step) ?? [];
    const nearestIndex = activeScore.events.reduce(
      (best, event) => {
        const distance = Math.abs((eventHorizontalPositions[event.index] ?? visual.left) - visual.left);
        return distance < best.distance ? { index: event.index, distance } : best;
      },
      { index: 0, distance: Number.POSITIVE_INFINITY },
    ).index;
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
      const key = `${Math.round(note.left)}:${Math.round(note.top)}:${midiPitch}`;
      if (seenNoteheads.has(key)) continue;
      seenNoteheads.add(key);
      const noteButton = document.createElement("button");
      noteButton.type = "button";
      noteButton.className = "note-target";
      noteButton.style.left = `${note.left - 4}px`;
      noteButton.style.top = `${note.top - 4}px`;
      noteButton.style.width = `${Math.max(16, note.width + 8)}px`;
      noteButton.style.height = `${Math.max(16, note.height + 8)}px`;
      const staffChord = [...new Set((sameStaff.length > 0 ? sameStaff : [ { midiPitch } ]).map((expected) => expected.midiPitch))]
        .sort((left, right) => left - right);
      noteButton.title = staffChord.length > 1 ? "Play this staff chord" : `Play single note ${noteName(midiPitch)}`;
      noteButton.setAttribute("aria-label", noteButton.title);
      installAuditionHandlers(noteButton, resolveIndex, staffChord);
      elements.scoreTargets.append(noteButton);
    }
  }
  moveOsmdCursor(highlightIndex);
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
  path.setAttribute("d", "M12 3v13m0 0 6-6m-6 6-6-6M4 20h16");
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
  let token: string | null = null;
  button.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    event.stopPropagation();
    token = `audition:${event.pointerId}:${crypto.randomUUID()}`;
    button.setPointerCapture(event.pointerId);
    void auditionDown(token, resolveIndex(), midiPitches);
  });
  const release = (): void => {
    if (token) void performUp(token);
    token = null;
  };
  button.addEventListener("pointerup", release);
  button.addEventListener("pointercancel", release);
  button.addEventListener("lostpointercapture", release);
  button.addEventListener("click", (event) => {
    if (event.detail !== 0) return;
    const keyboardToken = `audition-keyboard:${crypto.randomUUID()}`;
    void auditionDown(keyboardToken, resolveIndex(), midiPitches).then(() => performUp(keyboardToken));
  });
}

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

function fitSelect(select: HTMLSelectElement): void {
  const text = select.selectedOptions[0]?.textContent ?? "";
  select.style.width = `${Math.max(38, Math.min(190, text.length * 7 + 22))}px`;
}

async function refreshDevices(): Promise<void> {
  try {
    const [audioDevices, midiPorts, diagnostics] = await Promise.all([
      invoke<DeviceDto[]>("audio_devices"),
      invoke<MidiPortsDto>("midi_ports"),
      invoke<DiagnosticsDto>("diagnostics"),
    ]);
    populateSelect(elements.audioOutput, audioDevices);
    populateSelect(elements.midiInput, midiPorts.inputs, "Off");
    populateSelect(elements.midiOutput, midiPorts.outputs, "Off");
    if (midiPorts.selectedInput) elements.midiInput.value = midiPorts.selectedInput;
    if (midiPorts.selectedOutput) elements.midiOutput.value = midiPorts.selectedOutput;
    showDiagnostics(diagnostics);
    elements.diagnosticsButton.classList.toggle("not-ready", !diagnostics.ready);
  } catch {
    elements.diagnosticsButton.classList.add("not-ready");
  }
}

function showDiagnostics(diagnostics: DiagnosticsDto): void {
  lastDiagnostics = diagnostics;
  const rows: Array<[string, string]> = [
    ["State", diagnostics.ready ? "Ready" : diagnostics.message ?? "Unavailable"],
    ["Backend", diagnostics.audioBackend],
    ["Mode", diagnostics.asioStream ? "ASIO low latency" : "Shared low latency"],
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
    ["Direct WASAPI", diagnostics.directWasapiStream ? "Yes" : "No (CPAL fallback)"],
    ["Native ASIO", diagnostics.asioStream ? "Yes" : "No"],
    ["MIDI in", diagnostics.midiInput ?? "Off"],
    ["MIDI out", diagnostics.midiOutput ?? "Off"],
  ];
  if (lastUiNativeRoundTripMs !== null) {
    const callbackPeriodMs = diagnostics.sampleRate > 0
      ? diagnostics.bufferFrames * 1_000 / diagnostics.sampleRate
      : 0;
    rows.splice(5, 0,
      ["Last UI â†’ native reply", `${lastUiNativeRoundTripMs.toFixed(2)} ms (enqueue is earlier)`],
      ["UI â†’ endpoint bound", `< ${(lastUiNativeRoundTripMs + callbackPeriodMs + diagnostics.estimatedLatencyMs).toFixed(1)} ms`],
    );
  }
  if (diagnostics.outputDevice.toUpperCase().includes("QUAD-CAPTURE")) {
    rows.push([
      "Driver buffer",
      "QUAD-CAPTURE Control Panel â†’ Driver â†’ Driver Settings: lower Audio Buffer Size",
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
    const previousMidiError = lastDiagnostics?.midiOutputError;
    showDiagnostics(diagnostics);
    if (diagnostics.midiOutputError && diagnostics.midiOutputError !== previousMidiError) {
      toast(diagnostics.midiOutputError, "warning");
    }
    elements.diagnosticsButton.classList.toggle("not-ready", !diagnostics.ready);
  } catch {
    elements.diagnosticsButton.classList.add("not-ready");
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
      }
      if (payload.type === "ended") toast("End of score", "info");
      updatePosition();
    }),
    await listen<DiagnosticsDto>("audio-diagnostics", ({ payload }) => showDiagnostics(payload)),
    await listen<BeatMidiInput>("beat-midi-input", ({ payload }) => {
      if (payload.type === "down") void performDown(payload.token, payload.velocity);
      else void performUp(payload.token);
    }),
  );
}

elements.open.addEventListener("click", () => void chooseScore());
elements.emptyOpen.addEventListener("click", () => void chooseScore());
elements.panic.addEventListener("click", async () => {
  const nextMode = !midiFreePlay;
  await invokeSafe("set_midi_free_play", { enabled: nextMode });
  midiFreePlay = nextMode;
  updateMidiFreePlayButton();
});
elements.tap.addEventListener("pointerdown", (event) => {
  elements.tap.setPointerCapture(event.pointerId);
  void performDown(`pointer:${event.pointerId}`);
});
elements.tap.addEventListener("pointerup", (event) => void performUp(`pointer:${event.pointerId}`));
elements.tap.addEventListener("pointercancel", (event) => void performUp(`pointer:${event.pointerId}`));
elements.tap.addEventListener("lostpointercapture", (event) => {
  if (event instanceof PointerEvent) void performUp(`pointer:${event.pointerId}`);
});
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
    void invokeSafe("panic");
    return;
  }
  if ((event.code === "Space" || event.code === "Enter") && !event.repeat) {
    event.preventDefault();
    void performDown(`key:${event.code}`);
  }
});
document.addEventListener("keyup", (event) => {
  if (event.code === "Space" || event.code === "Enter") {
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
elements.audioOutput.addEventListener("change", () => void invokeSafe("set_audio_device", { id: elements.audioOutput.value }));
elements.midiInput.addEventListener("change", () => { fitSelect(elements.midiInput); void invokeSafe("set_midi_input", { id: elements.midiInput.value || null }); });
elements.midiOutput.addEventListener("change", () => { fitSelect(elements.midiOutput); void invokeSafe("set_midi_output", { id: elements.midiOutput.value || null }); });
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
function setZoomPercent(percent: number): void {
  const snapped = ZOOM_STEPS.reduce((nearest, candidate) =>
    Math.abs(candidate - percent) < Math.abs(nearest - percent) ? candidate : nearest,
  );
  zoom = snapped / 100;
  elements.zoomRange.value = String(snapped);
  elements.zoomValue.value = `${snapped}%`;
  renderOsmd();
}

function moveZoom(direction: -1 | 1): void {
  const current = Math.round(zoom * 100);
  const index = ZOOM_STEPS.findIndex((step) => step >= current);
  const currentIndex = index >= 0 && ZOOM_STEPS[index] === current ? index : Math.max(0, index - 1);
  setZoomPercent(ZOOM_STEPS[Math.max(0, Math.min(ZOOM_STEPS.length - 1, currentIndex + direction))]!);
}

elements.zoomOut.addEventListener("click", () => moveZoom(-1));
elements.zoomIn.addEventListener("click", () => moveZoom(1));
elements.zoomRange.addEventListener("input", () => setZoomPercent(Number(elements.zoomRange.value)));
window.addEventListener("resize", () => {
  if (osmd) window.requestAnimationFrame(renderOsmd);
});

void installListeners().then(refreshDevices).catch((error: unknown) => {
  setStatus("fault", "Core unavailable");
  toast(String(error), "error");
});
window.setInterval(() => void refreshDiagnostics(), 1_000);

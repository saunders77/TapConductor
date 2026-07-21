import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { OpenSheetMusicDisplay } from "opensheetmusicdisplay";
import type {
  CoreEvent,
  DeviceDto,
  DiagnosticsDto,
  LoadedScore,
  MidiPortsDto,
  TapEventDto,
} from "./types";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("Missing #app");

app.innerHTML = `
  <div class="shell">
    <header class="topbar">
      <div class="brand" aria-label="TapConductor">
        <span class="brand-mark" aria-hidden="true"><i></i><i></i><i></i></span>
        <span><strong>Tap</strong>Conductor</span>
      </div>
      <button id="open-score" class="primary-button" type="button">
        <span aria-hidden="true">＋</span> Open score
      </button>
      <div class="score-name" id="score-name">No score loaded</div>
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
      <label class="range-field">
        <span>Volume <output id="volume-value">80%</output></span>
        <input id="volume" type="range" min="0" max="100" value="80" />
      </label>
      <label class="range-field">
        <span>Tap velocity <output id="velocity-value">96</output></span>
        <input id="velocity" type="range" min="1" max="127" value="96" />
      </label>
      <button id="parts-button" class="deck-button" type="button" disabled>Parts <b id="parts-count">0</b></button>
      <button id="diagnostics-button" class="deck-button icon-button" type="button" title="Audio diagnostics">⌁</button>
      <button id="panic-button" class="panic-button" type="button" title="Release all notes (Escape)">Panic</button>
    </section>

    <aside id="parts-popover" class="popover hidden" aria-label="Active score parts"></aside>
    <aside id="diagnostics-popover" class="popover diagnostics hidden" aria-label="Audio diagnostics"></aside>

    <main class="workspace">
      <section class="score-panel" aria-label="Musical score">
        <div class="score-toolbar">
          <div class="mode-switch" role="group" aria-label="Score action">
            <button class="active" data-score-mode="perform" type="button">Perform</button>
            <button data-score-mode="audition" type="button">Audition</button>
            <button data-score-mode="position" type="button">Start here</button>
          </div>
          <div class="zoom-controls">
            <button id="zoom-out" type="button" aria-label="Zoom out">−</button>
            <output id="zoom-value">90%</output>
            <button id="zoom-in" type="button" aria-label="Zoom in">＋</button>
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

const byId = <T extends HTMLElement>(id: string): T => {
  const element = document.getElementById(id);
  if (!element) throw new Error(`Missing #${id}`);
  return element as T;
};

const elements = {
  open: byId<HTMLButtonElement>("open-score"),
  emptyOpen: byId<HTMLButtonElement>("empty-open"),
  scoreName: byId("score-name"),
  status: byId("status-pill"),
  audioOutput: byId<HTMLSelectElement>("audio-output"),
  midiInput: byId<HTMLSelectElement>("midi-input"),
  midiOutput: byId<HTMLSelectElement>("midi-output"),
  volume: byId<HTMLInputElement>("volume"),
  volumeValue: byId<HTMLOutputElement>("volume-value"),
  velocity: byId<HTMLInputElement>("velocity"),
  velocityValue: byId<HTMLOutputElement>("velocity-value"),
  parts: byId<HTMLButtonElement>("parts-button"),
  partsCount: byId("parts-count"),
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
  positionTitle: byId("position-title"),
  positionDetail: byId("position-detail"),
  nextTitle: byId("next-title"),
  nextDetail: byId("next-detail"),
  back: byId<HTMLButtonElement>("back-button"),
  tap: byId<HTMLButtonElement>("tap-button"),
  forward: byId<HTMLButtonElement>("forward-button"),
  toasts: byId("toast-region"),
};

type ScoreMode = "perform" | "audition" | "position";

let score: LoadedScore | null = null;
let cursorIndex = 0;
let highlightIndex = 0;
let zoom = 0.9;
let scoreMode: ScoreMode = "perform";
let osmd: OpenSheetMusicDisplay | null = null;
let osmdEventSteps: number[] = [];
let osmdCurrentStep = 0;
let lastDiagnostics: DiagnosticsDto | null = null;
let unlisteners: UnlistenFn[] = [];
const heldTokens = new Set<string>();
const pendingDowns = new Map<string, Promise<void>>();

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
  moveOsmdCursor(highlightIndex);
  document.querySelectorAll<HTMLElement>(".score-target").forEach((node) => {
    const indices = (node.dataset.eventIndices ?? node.dataset.eventIndex ?? "")
      .split(",")
      .map(Number);
    node.classList.toggle("current", indices.includes(highlightIndex));
  });
}

function moveOsmdCursor(index: number): void {
  if (!osmd || score?.format !== "music_xml") return;
  try {
    const visualStep = osmdEventSteps[index] ?? index;
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

async function displayScore(loaded: LoadedScore): Promise<void> {
  score = loaded;
  osmdEventSteps = [];
  osmdCurrentStep = 0;
  cursorIndex = 0;
  highlightIndex = 0;
  elements.scoreName.textContent = loaded.displayName;
  elements.empty.classList.add("hidden");
  elements.scoreStage.classList.remove("hidden");
  elements.tap.disabled = false;
  elements.parts.disabled = false;
  elements.partsCount.textContent = String(loaded.parts.filter((part) => part.enabled).length);
  renderParts();

  elements.osmd.replaceChildren();
  elements.scoreTargets.replaceChildren();
  if (loaded.format === "music_xml" && loaded.musicXml) {
    osmd = new OpenSheetMusicDisplay(elements.osmd, {
      autoResize: false,
      backend: "svg",
      drawTitle: true,
      drawingParameters: "compacttight",
      followCursor: true,
    });
    await osmd.load(loaded.musicXml);
    renderOsmd();
  } else {
    osmd = null;
    renderMidiRoll(loaded.events);
  }

  updatePosition();
  setStatus("ready", "Ready");
  loaded.warnings.forEach((warning) => toast(warning, "warning"));
}

function renderOsmd(): void {
  if (!osmd) return;
  osmd.Zoom = zoom;
  osmd.render();
  osmd.cursor.show();
  window.setTimeout(buildScoreTargets, 0);
}

function renderMidiRoll(events: TapEventDto[]): void {
  const wrapper = document.createElement("div");
  wrapper.className = "midi-roll";
  for (const event of events) {
    const group = document.createElement("button");
    group.type = "button";
    group.className = "midi-event score-target";
    group.dataset.eventIndex = String(event.index);
    group.style.setProperty("--event-height", String(Math.max(1, event.notes.length)));
    const measure = document.createElement("b");
    measure.textContent = event.measureNumber;
    const notes = document.createElement("span");
    notes.textContent = event.notes.map((note) => noteName(note.midiPitch)).join(" · ");
    group.append(measure, notes);
    installTargetHandlers(group, event.index);
    wrapper.append(group);
  }
  elements.osmd.append(wrapper);
}

function buildScoreTargets(): void {
  if (!osmd || !score) return;
  elements.scoreTargets.replaceChildren();
  const hostRect = elements.scoreStage.getBoundingClientRect();
  const cursor = osmd.cursor;
  cursor.reset();
  osmdCurrentStep = 0;
  cursor.show();

  type VisualStep = {
    step: number;
    measureIndex: number;
    numerator: number;
    denominator: number;
    left: number;
    top: number;
  };
  const visualSteps: VisualStep[] = [];
  const maximumSteps = Math.max(10_000, score.events.length * 8 + 100);
  for (let step = 0; step < maximumSteps && !cursor.Iterator.EndReached; step += 1) {
    const timestamp = cursor.Iterator.CurrentRelativeInMeasureTimestamp;
    const rect = cursor.cursorElement?.getBoundingClientRect();
    if (timestamp && rect) {
      visualSteps.push({
        step,
        measureIndex: cursor.Iterator.CurrentMeasureIndex,
        numerator: timestamp.GetExpandedNumerator(),
        denominator: timestamp.Denominator,
        left: rect.left - hostRect.left - 5,
        top: rect.top - hostRect.top - 25,
      });
    }
    cursor.next();
    osmdCurrentStep += 1;
  }

  const rationalMatches = (event: TapEventDto, step: VisualStep): boolean => {
    if (event.measureIndex !== step.measureIndex || step.denominator === 0) return false;
    const eventNumerator = BigInt(event.offset.numerator);
    const eventDenominator = BigInt(event.offset.denominator);
    const visualNumerator = BigInt(step.numerator);
    const visualDenominator = BigInt(step.denominator);
    const left = eventNumerator * visualDenominator;
    const right = visualNumerator * eventDenominator;
    // The normalized core uses MusicXML division units (quarter note = 1),
    // while OSMD versions commonly expose whole-note fractions. Accept the
    // direct representation too so this remains robust across OSMD changes.
    return left === right || left === right * 4n;
  };

  const groupedTargets = new Map<string, { eventIndices: number[]; visual: VisualStep; measureNumber: string }>();
  osmdEventSteps = [];
  for (const event of score.events) {
    const candidates = visualSteps.filter((step) => rationalMatches(event, step));
    const visual = candidates[Math.max(0, event.occurrence - 1)] ?? candidates[0] ?? visualSteps[event.index];
    if (!visual) continue;
    osmdEventSteps[event.index] = visual.step;
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
    const button = document.createElement("button");
    button.type = "button";
    button.className = "score-target";
    button.dataset.eventIndices = target.eventIndices.join(",");
    button.style.left = `${target.visual.left}px`;
    button.style.top = `${target.visual.top}px`;
    button.title = `Measure ${target.measureNumber}: audition or start here`;
    const symbol = document.createElement("span");
    symbol.textContent = "♪";
    button.append(symbol);
    installTargetHandlers(button, () =>
      target.eventIndices.find((index) => index >= cursorIndex) ?? target.eventIndices[0]!,
    );
    elements.scoreTargets.append(button);
  }
  moveOsmdCursor(highlightIndex);
}

async function actOnScoreTarget(index: number): Promise<void> {
  if (!score) return;
  if (scoreMode === "position") {
    await invokeSafe("set_cursor", { generation: score.generation, index });
    cursorIndex = index;
    highlightIndex = index;
    updatePosition();
    return;
  }
  if (scoreMode === "audition") {
    const token = `audition:${crypto.randomUUID()}`;
    await invokeSafe("audition_event", {
      generation: score.generation,
      index,
      token,
      velocity: Number(elements.velocity.value),
    });
    window.setTimeout(() => void invoke("release_input", { token }).catch(() => undefined), 10);
    return;
  }
}

function installTargetHandlers(button: HTMLButtonElement, index: number | (() => number)): void {
  const resolveIndex = (): number => (typeof index === "number" ? index : index());
  let performanceToken: string | null = null;
  button.addEventListener("pointerdown", (pointerEvent) => {
    pointerEvent.preventDefault();
    pointerEvent.stopPropagation();
    if (scoreMode === "perform") {
      performanceToken = `score-pointer:${pointerEvent.pointerId}:${crypto.randomUUID()}`;
      button.setPointerCapture(pointerEvent.pointerId);
      void performDown(performanceToken);
    } else {
      void actOnScoreTarget(resolveIndex());
    }
  });
  const release = (): void => {
    if (performanceToken) void performUp(performanceToken);
    performanceToken = null;
  };
  button.addEventListener("pointerup", release);
  button.addEventListener("pointercancel", release);
  button.addEventListener("lostpointercapture", release);
  button.addEventListener("click", (event) => {
    // Pointer input is handled on pointerdown for minimum latency. A click with
    // detail 0 is keyboard activation and needs an equivalent accessible path.
    if (event.detail !== 0) return;
    if (scoreMode === "perform") {
      const token = `score-keyboard:${crypto.randomUUID()}`;
      void performDown(token).then(() => performUp(token));
    } else {
      void actOnScoreTarget(resolveIndex());
    }
  });
}

async function performDown(token: string, velocity = Number(elements.velocity.value)): Promise<void> {
  if (!score || heldTokens.has(token)) return;
  heldTokens.add(token);
  const pending = invokeSafe<void>("performance_input_down", { token, velocity });
  pendingDowns.set(token, pending);
  try {
    await pending;
  } catch {
    heldTokens.delete(token);
    return;
  } finally {
    if (pendingDowns.get(token) === pending) pendingDowns.delete(token);
  }
}

async function performUp(token: string): Promise<void> {
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
  elements.partsPopover.replaceChildren();
  const heading = document.createElement("h3");
  heading.textContent = "Sounding parts";
  const explanation = document.createElement("p");
  explanation.textContent = "Simultaneous notes across every enabled part form one tap.";
  elements.partsPopover.append(heading, explanation);
  for (const part of score.parts) {
    const label = document.createElement("label");
    label.className = "check-row";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = part.enabled;
    const name = document.createElement("span");
    name.textContent = part.name;
    label.append(input, name);
    input.addEventListener("change", async () => {
      if (!score) return;
      const updated = await invokeSafe<LoadedScore>("set_part_enabled", {
        generation: score.generation,
        partId: part.id,
        enabled: input.checked,
      });
      await displayScore(updated);
    });
    elements.partsPopover.append(label);
  }
}

function populateSelect(select: HTMLSelectElement, devices: DeviceDto[], offLabel?: string): void {
  select.replaceChildren();
  if (offLabel) select.add(new Option(offLabel, ""));
  for (const device of devices) select.add(new Option(`${device.name}${device.isDefault ? " (default)" : ""}`, device.id));
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
    setStatus(diagnostics.ready ? "ready" : "loading", diagnostics.ready ? "Audio ready" : "Starting audio…");
  } catch {
    setStatus("fault", "Audio unavailable");
  }
}

function showDiagnostics(diagnostics: DiagnosticsDto): void {
  lastDiagnostics = diagnostics;
  const rows: Array<[string, string]> = [
    ["State", diagnostics.ready ? "Ready" : diagnostics.message ?? "Unavailable"],
    ["Backend", diagnostics.audioBackend],
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
    ["MIDI in", diagnostics.midiInput ?? "Off"],
    ["MIDI out", diagnostics.midiOutput ?? "Off"],
  ];
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
    setStatus(
      diagnostics.ready ? "ready" : "fault",
      diagnostics.ready ? "Audio ready" : diagnostics.message ?? "Audio unavailable",
    );
  } catch {
    setStatus("fault", "Core unavailable");
  }
}

async function installListeners(): Promise<void> {
  unlisteners.push(
    await listen<CoreEvent>("performance-event", ({ payload }) => {
      if (payload.type === "fault") {
        setStatus("fault", "Audio fault");
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
  );
}

elements.open.addEventListener("click", () => void chooseScore());
elements.emptyOpen.addEventListener("click", () => void chooseScore());
elements.panic.addEventListener("click", () => void invokeSafe("panic"));
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
});
elements.forward.addEventListener("click", async () => {
  if (!score) return;
  const index = Math.min(score.events.length - 1, cursorIndex + 1);
  await invokeSafe("set_cursor", { generation: score.generation, index });
  cursorIndex = index;
  highlightIndex = index;
  updatePosition();
});

document.addEventListener("keydown", (event) => {
  if (event.code === "Escape") {
    event.preventDefault();
    void invokeSafe("panic");
    return;
  }
  const targetIsControl = event.target instanceof Element && Boolean(event.target.closest("button, input, select, textarea, a"));
  if ((event.code === "Space" || event.code === "Enter") && !event.repeat && !targetIsControl) {
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
elements.velocity.addEventListener("input", () => (elements.velocityValue.value = elements.velocity.value));
elements.audioOutput.addEventListener("change", () => void invokeSafe("set_audio_device", { id: elements.audioOutput.value }));
elements.midiInput.addEventListener("change", () => void invokeSafe("set_midi_input", { id: elements.midiInput.value || null }));
elements.midiOutput.addEventListener("change", () => void invokeSafe("set_midi_output", { id: elements.midiOutput.value || null }));
elements.parts.addEventListener("click", () => elements.partsPopover.classList.toggle("hidden"));
elements.diagnosticsButton.addEventListener("click", () => {
  if (lastDiagnostics) showDiagnostics(lastDiagnostics);
  elements.diagnostics.classList.toggle("hidden");
});
elements.zoomOut.addEventListener("click", () => {
  zoom = Math.max(0.5, zoom - 0.1);
  elements.zoomValue.value = `${Math.round(zoom * 100)}%`;
  renderOsmd();
});
elements.zoomIn.addEventListener("click", () => {
  zoom = Math.min(1.8, zoom + 0.1);
  elements.zoomValue.value = `${Math.round(zoom * 100)}%`;
  renderOsmd();
});
document.querySelectorAll<HTMLButtonElement>("[data-score-mode]").forEach((button) => {
  button.addEventListener("click", () => {
    document.querySelectorAll("[data-score-mode]").forEach((node) => node.classList.remove("active"));
    button.classList.add("active");
    scoreMode = button.dataset.scoreMode as ScoreMode;
    elements.scoreStage.dataset.mode = scoreMode;
  });
});
window.addEventListener("resize", () => {
  if (osmd) window.requestAnimationFrame(renderOsmd);
});

void installListeners().then(refreshDevices).catch((error: unknown) => {
  setStatus("fault", "Core unavailable");
  toast(String(error), "error");
});
window.setInterval(() => void refreshDiagnostics(), 1_000);

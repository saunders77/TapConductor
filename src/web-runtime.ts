// Copyright (c) 2026 Michael Saunders
import type {
  BeatMidiInput,
  CoreEvent,
  DeviceDto,
  DiagnosticsDto,
  LoadedScore,
  MidiPortsDto,
  RationalDto,
} from "./types";
import { volumeToMidiVelocity } from "./midi-velocity";
import { PianoShortcutGate } from "./piano-shortcuts";
import {
  releaseBoundaryIndex,
  releaseBoundaryTimeline,
  releasesOnInput,
  type ScoreReleasePlan,
} from "./web-note-gate";
import { AsyncSerialQueue } from "./async-serial-queue";

type EventHandler<T> = (event: { payload: T }) => void;
type Listener = EventHandler<unknown>;

interface WasmScore {
  dto_json(): string;
  set_part_enabled(partId: string, enabled: boolean): void;
  free(): void;
}

interface WasmModule {
  default(moduleOrPath?: unknown): Promise<unknown>;
  WebScore: new (bytes: Uint8Array, fileName: string) => WasmScore;
}

type VoiceNote = {
  oscillator: OscillatorNode;
  gain: GainNode;
  midiPitch: number;
  releaseBoundaryIndex: number | null;
  releaseOnOriginalInputOnly: boolean;
  boundaryReached: boolean;
  releasing: boolean;
};

type Voice = {
  notes: VoiceNote[];
  releaseOnInput: boolean;
  releaseOnNextPlay: boolean;
  stopAt: number;
};

type SinkAudioContext = AudioContext & {
  setSinkId?: (sinkId: string) => Promise<void>;
  outputLatency?: number;
};

const DEFAULT_VELOCITY = 96;
const ENABLE_WEB_MIDI_ID = "__enable_web_midi__";
const NONE_AUDIO_OUTPUT_ID = "__none_mute__";
// Still far steeper than the held-note decay, but long enough for the
// key-up tail to remain clearly audible instead of sounding abruptly muted.
const WEB_RELEASE_SECONDS = 0.4;
type StandaloneGlobals = typeof globalThis & {
  __TAPCONDUCTOR_CHOIR_DEMO_URL__?: string;
  __TAPCONDUCTOR_PIANO_DEMO_URL__?: string;
  __TAPCONDUCTOR_WASM_JS__?: string;
  __TAPCONDUCTOR_WASM_BINARY__?: string;
};

const standaloneGlobals = globalThis as StandaloneGlobals;
const PIANO_DEMO_SCORE_URL = standaloneGlobals.__TAPCONDUCTOR_PIANO_DEMO_URL__ ?? new URL(
  "../assets/demo/Prelude in C Minor - Chopin 1839.musicxml",
  import.meta.url,
).href;
const CHOIR_DEMO_SCORE_URL = standaloneGlobals.__TAPCONDUCTOR_CHOIR_DEMO_URL__ ?? new URL(
  "../assets/demo/All-Night Vigil - Rachmaninoff 1915.musicxml",
  import.meta.url,
).href;

class BrowserAudio {
  private context: SinkAudioContext | null = null;
  private master: GainNode | null = null;
  private pianoWave: PeriodicWave | null = null;
  private volume = 1;
  private muted = false;
  private instrument: "piano" | "synth" = "piano";
  private readonly voices = new Map<string, Voice>();
  private starts = 0;
  private voiceSteals = 0;

  async ensureReady(): Promise<SinkAudioContext> {
    if (!this.context) {
      const context = new AudioContext() as SinkAudioContext;
      const master = context.createGain();
      const limiter = context.createDynamicsCompressor();
      master.gain.value = this.muted ? 0 : this.volume;
      limiter.threshold.value = -8;
      limiter.knee.value = 4;
      limiter.ratio.value = 16;
      limiter.attack.value = 0.003;
      limiter.release.value = 0.12;
      master.connect(limiter);
      limiter.connect(context.destination);
      this.context = context;
      this.master = master;
      // Match the bright, slightly inharmonic partial balance used by the
      // desktop fallback synth. Web Audio periodic waves are harmonic-only,
      // but this retains the much more piano-like upper-mode emphasis.
      const real = new Float32Array([0, 1, 0.48, 0.30, 0.21, 0.15, 0.11]);
      this.pianoWave = context.createPeriodicWave(real, new Float32Array(real.length));
    }
    if (this.context.state === "suspended") await this.context.resume();
    return this.context;
  }

  setVolume(value: number): void {
    this.volume = Math.max(0, Math.min(1, value));
    if (this.context && this.master) {
      this.master.gain.setTargetAtTime(this.muted ? 0 : this.volume, this.context.currentTime, 0.01);
    }
  }

  setMuted(muted: boolean): void {
    this.muted = muted;
    this.panic();
    if (this.context && this.master) {
      this.master.gain.setTargetAtTime(muted ? 0 : this.volume, this.context.currentTime, 0.01);
    }
  }

  isMuted(): boolean {
    return this.muted;
  }

  setInstrument(instrument: string): void {
    this.instrument = instrument === "synth" ? "synth" : "piano";
    this.panic();
  }

  async setSink(id: string): Promise<void> {
    const context = await this.ensureReady();
    if (!id) {
      if (context.setSinkId) await context.setSinkId("");
      return;
    }
    if (!context.setSinkId) {
      throw new Error("This browser cannot select a specific audio output. It will use the system default.");
    }
    await context.setSinkId(id);
  }

  async play(
    token: string,
    pitches: number[],
    velocity: number,
    rollMs: number,
    output?: MIDIOutput,
    midiVelocity = velocity,
    releasePlans?: readonly ScoreReleasePlan[],
    releaseOnNextPlay = false,
  ): Promise<void> {
    if (pitches.length === 0) return;
    const context = await this.ensureReady();
    this.releaseCompleted(context.currentTime, output);
    for (const [voiceToken, voice] of this.voices) {
      if (!voice.releaseOnNextPlay) continue;
      for (const note of voice.notes) this.releaseNote(note, context.currentTime, output);
      this.pruneVoice(voiceToken);
    }
    if (this.voices.size >= 64) {
      const oldest = this.voices.keys().next().value as string | undefined;
      if (oldest) {
        this.stop(oldest, context.currentTime + 0.045, output);
        this.voiceSteals += 1;
      }
    }

    const sorted = pitches
      .map((pitch, index) => ({
        pitch,
        releaseBoundaryIndex: releasePlans?.[index]?.boundaryIndex ?? null,
        releaseOnOriginalInputOnly: releasePlans?.[index]?.originalInputOnly ?? false,
      }))
      .sort((left, right) => left.pitch - right.pitch);
    const baseStart = context.currentTime + 0.005;
    const finalStart = baseStart + Math.max(0, sorted.length - 1) * rollMs / 1_000;
    const naturalStop = finalStart + 10;
    const notes: VoiceNote[] = [];
    const velocityGain = Math.sqrt(Math.max(1, Math.min(127, velocity)) / 127);
    // Chord-aware headroom prevents simultaneous oscillators from summing
    // above full scale. The limiter is a final guard, not the primary scaler.
    const chordScale = 1 / Math.max(1, sorted.length ** 0.7);
    const peakGain = velocityGain
      * chordScale
      * (this.instrument === "piano" ? 0.2 : 0.16);
    sorted.forEach(({ pitch, releaseBoundaryIndex, releaseOnOriginalInputOnly }, index) => {
      const start = baseStart + index * rollMs / 1_000;
      const oscillator = context.createOscillator();
      const noteGain = context.createGain();
      oscillator.frequency.value = 440 * 2 ** ((pitch - 69) / 12);
      if (this.instrument === "piano") {
        oscillator.setPeriodicWave(this.pianoWave!);
      } else {
        oscillator.type = "triangle";
      }
      // Every note, including delayed notes in a roll, gets its own attack.
      // Starting at near-silence removes the waveform discontinuity that
      // otherwise presents as a click or pop.
      noteGain.gain.setValueAtTime(0, start);
      noteGain.gain.linearRampToValueAtTime(peakGain, start + 0.012);
      if (this.instrument === "piano") {
        noteGain.gain.exponentialRampToValueAtTime(0.0001, naturalStop);
      } else {
        noteGain.gain.exponentialRampToValueAtTime(
          Math.max(0.0001, peakGain * 0.04),
          naturalStop,
        );
      }
      oscillator.connect(noteGain);
      noteGain.connect(this.master!);
      oscillator.start(start);
      oscillator.stop(naturalStop + 0.06);
      notes.push({
        oscillator,
        gain: noteGain,
        midiPitch: pitch,
        releaseBoundaryIndex,
        releaseOnOriginalInputOnly,
        boundaryReached: false,
        releasing: false,
      });
      if (midiVelocity > 0) {
        output?.send(
          new Uint8Array([0x90, pitch, Math.max(1, Math.min(127, midiVelocity))]),
          performance.now() + index * rollMs,
        );
      }
    });

    this.voices.set(token, {
      notes,
      releaseOnInput: releasePlans === undefined,
      releaseOnNextPlay,
      stopAt: naturalStop,
    });
    this.starts += 1;
  }

  releaseInput(token: string, output?: MIDIOutput): void {
    if (!this.context) return;
    const now = this.context.currentTime;
    for (const [voiceToken, voice] of this.voices) {
      for (const note of voice.notes) {
        if (
          (voice.releaseOnInput && voiceToken === token)
          || (
            !voice.releaseOnInput
            && releasesOnInput(
              {
                boundaryIndex: note.releaseBoundaryIndex,
                originalInputOnly: note.releaseOnOriginalInputOnly,
              },
              voiceToken === token,
            )
          )
        ) {
          this.releaseNote(note, now, output);
        }
      }
      this.pruneVoice(voiceToken);
    }
  }

  releaseNow(token: string, output?: MIDIOutput): void {
    this.releaseInput(token, output);
  }

  advanceScoreBoundary(eventIndex: number, output?: MIDIOutput): void {
    if (!this.context) return;
    const now = this.context.currentTime;
    for (const [token, voice] of this.voices) {
      if (voice.releaseOnInput) continue;
      for (const note of voice.notes) {
        if (
          note.releaseBoundaryIndex !== null
          && !note.boundaryReached
          && eventIndex >= note.releaseBoundaryIndex
        ) {
          note.boundaryReached = true;
          this.releaseNote(note, now, output);
        }
      }
      this.pruneVoice(token);
    }
  }

  panic(output?: MIDIOutput): void {
    if (this.context) {
      for (const token of [...this.voices.keys()]) {
        this.stop(token, this.context.currentTime + 0.025, output);
      }
    }
    for (let channel = 0; channel < 16; channel += 1) {
      output?.send(new Uint8Array([0xb0 | channel, 123, 0]));
    }
  }

  activeVoices(): number {
    return [...this.voices.values()].reduce(
      (sum, voice) => sum + voice.notes.filter((note) => !note.releasing).length,
      0,
    );
  }

  totalStarts(): number {
    return this.starts;
  }

  steals(): number {
    return this.voiceSteals;
  }

  sampleRate(): number {
    return this.context?.sampleRate ?? 48_000;
  }

  latencyMs(): number {
    if (!this.context) return 0;
    return ((this.context.baseLatency ?? 0) + (this.context.outputLatency ?? 0)) * 1_000;
  }

  state(): AudioContextState | "not-started" {
    return this.context?.state ?? "not-started";
  }

  private releaseCompleted(now: number, output?: MIDIOutput): void {
    for (const [token, voice] of this.voices) {
      if (voice.stopAt <= now) this.stop(token, now + 0.025, output);
    }
  }

  private stop(token: string, at: number, output?: MIDIOutput): void {
    const voice = this.voices.get(token);
    if (!voice || !this.context) return;
    const now = this.context.currentTime;
    const stopTime = Math.max(now + 0.005, at);
    for (const note of voice.notes) {
      this.releaseNote(note, now, output, stopTime);
    }
    this.voices.delete(token);
  }

  private releaseNote(
    note: VoiceNote,
    now: number,
    output?: MIDIOutput,
    requestedStop?: number,
  ): void {
    if (note.releasing) return;
    note.releasing = true;
    const releaseEnd = requestedStop
      ?? now + WEB_RELEASE_SECONDS;
    const gain = note.gain.gain;
    const currentGain = Math.max(0.0001, gain.value);
    gain.cancelScheduledValues(now);
    gain.setValueAtTime(currentGain, now);
    gain.exponentialRampToValueAtTime(0.0001, releaseEnd);
    try {
      note.oscillator.stop(releaseEnd + 0.03);
    } catch {
      // It was already scheduled to stop naturally.
    }
    output?.send(new Uint8Array([0x80, note.midiPitch, 0]));
  }

  private pruneVoice(token: string): void {
    const voice = this.voices.get(token);
    if (voice?.notes.every((note) => note.releasing)) this.voices.delete(token);
  }
}

export class WebRuntime {
  private readonly listeners = new Map<string, Set<Listener>>();
  private readonly audio = new BrowserAudio();
  private wasmModule: Promise<WasmModule> | null = null;
  private wasmScore: WasmScore | null = null;
  private score: LoadedScore | null = null;
  private cursor = 0;
  private heldTokens = new Set<string>();
  private rollRegularMs = 0;
  private rollAuditionMs = 120;
  private midiAccess: MIDIAccess | null = null;
  private midiPromise: Promise<MIDIAccess | null> | null = null;
  private selectedMidiInput: string | null = null;
  private selectedMidiOutput: string | null = null;
  private midiFreePlay = false;
  private readonly pianoShortcuts = new PianoShortcutGate();
  private legatoMode = false;
  private volume = 1;
  private readonly midiTokenPitches = new Map<string, number>();
  private lastMidiError: string | undefined;
  private releaseTimeline: RationalDto[] = [];
  private readonly performanceQueue = new AsyncSerialQueue();

  async invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    const value = await this.dispatch(command, args);
    return value as T;
  }

  listen<T>(event: string, handler: EventHandler<T>): () => void {
    const handlers = this.listeners.get(event) ?? new Set<Listener>();
    handlers.add(handler as Listener);
    this.listeners.set(event, handlers);
    return () => handlers.delete(handler as Listener);
  }

  private async dispatch(command: string, args: Record<string, unknown>): Promise<unknown> {
    switch (command) {
      case "load_score":
        return this.loadFile(this.requireFile(args.path));
      case "load_demo_score": {
        const kind = String(args.kind);
        const isChoir = kind === "choir";
        if (!isChoir && kind !== "piano") throw new Error(`Unknown demo score: ${kind}`);
        const url = isChoir ? CHOIR_DEMO_SCORE_URL : PIANO_DEMO_SCORE_URL;
        const fileName = isChoir ? "All-Night Vigil - Rachmaninoff 1915.musicxml" : "Prelude in C Minor - Chopin 1839.musicxml";
        const response = await fetch(url);
        if (!response.ok) throw new Error(`Unable to load the demo score (${response.status}).`);
        return this.loadBytes(new Uint8Array(await response.arrayBuffer()), fileName);
      }
      case "set_part_enabled":
        return this.setPartEnabled(String(args.partId), Boolean(args.enabled));
      case "set_cursor":
        this.requireGeneration(Number(args.generation));
        this.cursor = this.validIndex(Number(args.index));
        this.emit<CoreEvent>("performance-event", {
          type: "cursor",
          generation: this.score!.generation,
          index: this.cursor,
        });
        return undefined;
      case "performance_input_down":
        return this.performanceQueue.run(
          () => this.performanceDown(String(args.token), Number(args.velocity ?? DEFAULT_VELOCITY)),
        );
      case "release_input":
        return this.release(String(args.token));
      case "audition_event":
        return this.audition(
          String(args.token),
          Number(args.generation),
          Number(args.index),
          this.soundingPitches(Number(args.index)),
          Number(args.velocity ?? DEFAULT_VELOCITY),
        );
      case "audition_note":
        return this.audition(
          String(args.token),
          Number(args.generation),
          Number(args.index),
          [Number(args.midiPitch)],
          Number(args.velocity ?? DEFAULT_VELOCITY),
        );
      case "audition_chord":
        return this.audition(
          String(args.token),
          Number(args.generation),
          Number(args.index),
          (args.midiPitches as number[]) ?? [],
          Number(args.velocity ?? DEFAULT_VELOCITY),
        );
      case "panic":
        this.panic();
        return undefined;
      case "set_midi_free_play":
        this.midiFreePlay = Boolean(args.enabled);
        this.panic();
        return undefined;
      case "set_piano_shortcut_pitch":
        this.pianoShortcuts.setFunctionPitch(Number(args.midiPitch));
        return undefined;
      case "audio_devices":
        return this.audioDevices();
      case "set_audio_device":
        if (args.id === NONE_AUDIO_OUTPUT_ID) {
          this.audio.setMuted(true);
        } else {
          await this.audio.setSink(String(args.id ?? ""));
          this.audio.setMuted(false);
        }
        return undefined;
      case "reload_audio_systems":
        await this.ensureMidi(true);
        this.emit("audio-diagnostics", this.diagnostics());
        return undefined;
      case "set_instrument":
        this.audio.setInstrument(String(args.instrument));
        return undefined;
      case "set_volume":
        this.volume = Math.max(0, Math.min(1, Number(args.value)));
        this.audio.setVolume(this.volume);
        return undefined;
      case "set_roll_delays":
        this.rollRegularMs = Number(args.regularMs);
        this.rollAuditionMs = Number(args.auditionMs);
        return undefined;
      case "midi_ports":
        return this.midiPorts();
      case "set_midi_input":
        await this.selectMidiInput(args.id === null ? null : String(args.id));
        return undefined;
      case "set_midi_output":
        await this.selectMidiOutput(args.id === null ? null : String(args.id));
        return undefined;
      case "diagnostics":
        return this.diagnostics();
      case "set_tap_mode":
        return undefined;
      case "set_legato_mode":
        this.panic();
        this.legatoMode = Boolean(args.enabled);
        return undefined;
      default:
        throw new Error(`The web runtime does not implement '${command}'.`);
    }
  }

  private async loadFile(file: File): Promise<LoadedScore> {
    return this.loadBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  }

  private async loadBytes(bytes: Uint8Array, fileName: string): Promise<LoadedScore> {
    this.panic();
    this.wasmScore?.free();
    const module = await this.getWasm();
    this.wasmScore = new module.WebScore(bytes, fileName);
    this.score = JSON.parse(this.wasmScore.dto_json()) as LoadedScore;
    this.releaseTimeline = releaseBoundaryTimeline(this.score.events);
    this.cursor = 0;
    this.emit<CoreEvent>("performance-event", {
      type: "ready",
      generation: this.score.generation,
    });
    return this.score;
  }

  private async setPartEnabled(partId: string, enabled: boolean): Promise<LoadedScore> {
    if (!this.wasmScore) throw new Error("No score is loaded.");
    this.panic();
    this.wasmScore.set_part_enabled(partId, enabled);
    this.score = JSON.parse(this.wasmScore.dto_json()) as LoadedScore;
    this.releaseTimeline = releaseBoundaryTimeline(this.score.events);
    this.cursor = 0;
    this.emit<CoreEvent>("performance-event", {
      type: "ready",
      generation: this.score.generation,
    });
    return this.score;
  }

  private async performanceDown(token: string, velocity: number): Promise<void> {
    if (this.heldTokens.has(token)) return;
    const midiPitch = this.midiTokenPitches.get(token);
    if (this.midiFreePlay && midiPitch !== undefined) {
      this.heldTokens.add(token);
      await this.audio.play(token, [midiPitch], velocity, 0, this.midiOutput());
      return;
    }
    if (!this.score) throw new Error("Open a score before conducting.");
    this.heldTokens.add(token);
    const playedIndex = this.cursor;
    const event = this.score.events[playedIndex];
    if (!event) return;
    const midiOutput = this.midiOutput();
    if (this.legatoMode) this.audio.advanceScoreBoundary(playedIndex, midiOutput);
    await this.audio.play(
      token,
      event.notes.map((note) => note.midiPitch),
      velocity,
      this.rollRegularMs,
      midiOutput,
      midiPitch === undefined ? volumeToMidiVelocity(this.volume) : velocity,
      this.legatoMode ? this.releaseBoundaries(event.notes, playedIndex) : undefined,
    );
    const atEnd = playedIndex >= this.score.events.length - 1;
    this.cursor = atEnd ? playedIndex : playedIndex + 1;
    this.emit<CoreEvent>("performance-event", {
      type: "cursor",
      generation: this.score.generation,
      index: this.cursor,
      playedIndex,
    });
    if (atEnd) {
      this.emit<CoreEvent>("performance-event", {
        type: "ended",
        generation: this.score.generation,
      });
    }
  }

  private async audition(
    token: string,
    generation: number,
    index: number,
    pitches: number[],
    velocity: number,
  ): Promise<void> {
    this.requireGeneration(generation);
    this.validIndex(index);
    if (this.heldTokens.has(token)) return;
    this.heldTokens.add(token);
    await this.audio.play(
      token,
      pitches,
      velocity,
      this.rollAuditionMs,
      this.midiOutput(),
      volumeToMidiVelocity(this.volume),
      undefined,
      true,
    );
  }

  private release(token: string): void {
    if (!this.heldTokens.delete(token)) return;
    if (this.midiFreePlay && this.midiTokenPitches.has(token)) {
      this.audio.releaseNow(token, this.midiOutput());
    } else {
      this.audio.releaseInput(token, this.midiOutput());
    }
  }

  private panic(): void {
    this.audio.panic(this.midiOutput());
    this.heldTokens.clear();
    this.pianoShortcuts.reset();
  }

  private soundingPitches(index: number): number[] {
    if (!this.score) throw new Error("No score is loaded.");
    const event = this.score.events[this.validIndex(index)]!;
    const position = event.absolute;
    const pitches = this.score.events
      .slice(0, index + 1)
      .flatMap((candidate) => candidate.notes)
      .filter((note) => compareRational(note.end, position) > 0)
      .map((note) => note.midiPitch);
    return [...new Set(pitches)].sort((left, right) => left - right);
  }

  private releaseBoundaries(
    notes: LoadedScore["events"][number]["notes"],
    playedIndex: number,
  ): ScoreReleasePlan[] {
    if (!this.score) throw new Error("No score is loaded.");
    return notes.map((note) => ({
      boundaryIndex: releaseBoundaryIndex(
        this.score!.events,
        playedIndex,
        note.end,
        note.isStaccato,
        this.releaseTimeline,
      ),
      originalInputOnly: note.isStaccato,
    }));
  }

  private async audioDevices(): Promise<DeviceDto[]> {
    if (!navigator.mediaDevices?.enumerateDevices) return [];
    const devices = await navigator.mediaDevices.enumerateDevices();
    return devices
      .filter((device) => device.kind === "audiooutput")
      .map((device, index) => ({
        id: device.deviceId,
        name: device.label || `Audio ${index + 1}`,
        isDefault: device.deviceId === "default",
      }));
  }

  private async ensureMidi(force = false): Promise<MIDIAccess | null> {
    if (this.midiAccess && !force) return this.midiAccess;
    if (this.midiPromise && !force) return this.midiPromise;
    if (!("requestMIDIAccess" in navigator)) return null;
    this.midiPromise = navigator.requestMIDIAccess({ sysex: false })
      .then((access) => {
        this.midiAccess = access;
        access.onstatechange = () => {
          if (this.selectedMidiInput && !access.inputs.has(this.selectedMidiInput)) {
            this.selectedMidiInput = null;
          }
          if (this.selectedMidiOutput && !access.outputs.has(this.selectedMidiOutput)) {
            this.selectedMidiOutput = null;
          }
        };
        this.lastMidiError = undefined;
        return access;
      })
      .catch((error: unknown) => {
        this.lastMidiError = `Web MIDI unavailable: ${String(error)}`;
        return null;
      });
    return this.midiPromise;
  }

  private midiPorts(): MidiPortsDto {
    if (!this.midiAccess && "requestMIDIAccess" in navigator) {
      const enable = { id: ENABLE_WEB_MIDI_ID, name: "Enable Web MIDI…" };
      return { inputs: [enable], outputs: [enable] };
    }
    const toDevices = (ports: Iterable<MIDIPort>): DeviceDto[] =>
      [...ports]
        .filter((port) => port.state === "connected")
        .map((port) => ({
          id: port.id,
          name: [port.manufacturer, port.name].filter(Boolean).join(" ") || "MIDI device",
        }));
    return {
      inputs: toDevices(this.midiAccess?.inputs.values() ?? []),
      outputs: toDevices(this.midiAccess?.outputs.values() ?? []),
      selectedInput: this.selectedMidiInput ?? undefined,
      selectedOutput: this.selectedMidiOutput ?? undefined,
    };
  }

  private async selectMidiInput(id: string | null): Promise<void> {
    await this.ensureMidi();
    if (id === ENABLE_WEB_MIDI_ID) {
      id = [...this.midiAccess?.inputs.keys() ?? []][0] ?? null;
    }
    for (const input of this.midiAccess?.inputs.values() ?? []) input.onmidimessage = null;
    this.selectedMidiInput = id;
    if (!id) return;
    const input = this.midiAccess?.inputs.get(id);
    if (!input) throw new Error("The selected MIDI input is unavailable.");
    input.onmidimessage = (event) => {
      if (event.data) this.onMidiMessage(input.id, event.data);
    };
  }

  private async selectMidiOutput(id: string | null): Promise<void> {
    await this.ensureMidi();
    if (id === ENABLE_WEB_MIDI_ID) {
      id = [...this.midiAccess?.outputs.keys() ?? []][0] ?? null;
    }
    if (id && !this.midiAccess?.outputs.has(id)) {
      throw new Error("The selected MIDI output is unavailable.");
    }
    this.selectedMidiOutput = id;
  }

  private onMidiMessage(inputId: string, data: Uint8Array): void {
    const status = data[0] ?? 0;
    const kind = status & 0xf0;
    const pitch = data[1] ?? 0;
    const velocity = data[2] ?? 0;
    if (kind !== 0x80 && kind !== 0x90) return;
    const token = `midi:${inputId}:${status & 0x0f}:${pitch}`;
    if (kind === 0x90 && velocity > 0) {
      const shortcut = this.pianoShortcuts.process({ type: "down", token, pitch });
      if (shortcut.type === "consume") {
        if (shortcut.event) this.emit("piano-shortcut", shortcut.event);
        return;
      }
      this.midiTokenPitches.set(token, pitch);
      this.emit<BeatMidiInput>("beat-midi-input", { type: "down", token, velocity });
    } else {
      const shortcut = this.pianoShortcuts.process({ type: "up", token });
      if (shortcut.type === "consume") {
        if (shortcut.event) this.emit("piano-shortcut", shortcut.event);
        return;
      }
      this.emit<BeatMidiInput>("beat-midi-input", { type: "up", token });
      this.midiTokenPitches.delete(token);
    }
  }

  private midiOutput(): MIDIOutput | undefined {
    return this.selectedMidiOutput
      ? this.midiAccess?.outputs.get(this.selectedMidiOutput)
      : undefined;
  }

  private diagnostics(): DiagnosticsDto {
    const midiPorts = this.midiPorts();
    const state = this.audio.state();
    const ready = state !== "closed";
    return {
      audioBackend: "Web Audio",
      outputDevice: this.audio.isMuted() ? "None (Mute)" : "Browser / system default",
      sampleRate: this.audio.sampleRate(),
      bufferFrames: 128,
      estimatedLatencyMs: this.audio.latencyMs(),
      callbackUnderruns: 0,
      backendErrors: 0,
      lateCommands: 0,
      invalidAudioBuffers: 0,
      voiceSteals: this.audio.steals(),
      queueOverflows: 0,
      activeVoices: this.audio.activeVoices(),
      directWasapiStream: false,
      asioStream: false,
      midiInput: midiPorts.inputs.find((port) => port.id === this.selectedMidiInput)?.name,
      midiOutput: midiPorts.outputs.find((port) => port.id === this.selectedMidiOutput)?.name,
      midiOutputError: this.lastMidiError,
      ready,
      message: state === "suspended"
        ? "Audio is suspended until you interact with the page."
        : state === "not-started"
          ? "Audio will start with your first tap."
          : undefined,
    };
  }

  private requireFile(value: unknown): File {
    if (!(value instanceof File)) throw new Error("The browser did not provide a readable score file.");
    return value;
  }

  private requireGeneration(generation: number): void {
    if (!this.score) throw new Error("No score is loaded.");
    if (generation !== this.score.generation) {
      throw new Error(`The score changed; expected generation ${this.score.generation}.`);
    }
  }

  private validIndex(index: number): number {
    if (!this.score || !Number.isInteger(index) || index < 0 || index >= this.score.events.length) {
      throw new Error(`Score event index ${index} is out of range.`);
    }
    return index;
  }

  private async getWasm(): Promise<WasmModule> {
    if (!this.wasmModule) {
      const moduleUrl = standaloneGlobals.__TAPCONDUCTOR_WASM_JS__
        ?? `${import.meta.env.BASE_URL}wasm/tapconductor_web.js`;
      this.wasmModule = import(/* @vite-ignore */ moduleUrl).then(async (loaded: unknown) => {
        const module = loaded as WasmModule;
        const binaryUrl = standaloneGlobals.__TAPCONDUCTOR_WASM_BINARY__;
        await module.default(binaryUrl ? { module_or_path: binaryUrl } : undefined);
        return module;
      });
    }
    return this.wasmModule;
  }

  private emit<T>(event: string, payload: T): void {
    for (const handler of this.listeners.get(event) ?? []) {
      handler({ payload } as { payload: unknown });
    }
  }
}

function compareRational(left: RationalDto, right: RationalDto): number {
  return left.numerator * right.denominator - right.numerator * left.denominator;
}

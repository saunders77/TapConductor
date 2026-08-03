export interface RationalDto {
  numerator: number;
  denominator: number;
}

export interface NoteDto {
  sourceId: string;
  partId: string;
  partIndex: number;
  staff: number;
  voice: string;
  midiPitch: number;
  isGrace: boolean;
  isStaccato: boolean;
  end: RationalDto;
}

export interface TapEventDto {
  id: string;
  index: number;
  measureIndex: number;
  measureNumber: string;
  occurrence: number;
  absolute: RationalDto;
  offset: RationalDto;
  positionOrder: number;
  notes: NoteDto[];
}

export interface BeatDto {
  absolute: RationalDto;
  measureIndex: number;
  beatIndex: number;
  beatsInMeasure: number;
  beatType: number;
}

export interface PartDto {
  id: string;
  name: string;
  enabled: boolean;
}

export interface LoadedScore {
  generation: number;
  path: string;
  displayName: string;
  format: "music_xml" | "midi";
  musicXml?: string;
  events: TapEventDto[];
  beats: BeatDto[];
  parts: PartDto[];
  warnings: string[];
}

export interface DeviceDto {
  id: string;
  name: string;
  isDefault?: boolean;
}

export interface MidiPortsDto {
  inputs: DeviceDto[];
  outputs: DeviceDto[];
  selectedInput?: string;
  selectedOutput?: string;
}

export interface DiagnosticsDto {
  audioBackend: string;
  outputDevice: string;
  sampleRate: number;
  bufferFrames: number;
  estimatedLatencyMs: number;
  callbackUnderruns: number;
  backendErrors: number;
  lateCommands: number;
  invalidAudioBuffers: number;
  voiceSteals: number;
  queueOverflows: number;
  activeVoices: number;
  directWasapiStream: boolean;
  asioStream: boolean;
  wasapiPeriods?: {
    sampleRate: number;
    channels: number;
    defaultFrames: number;
    fundamentalFrames: number;
    minimumFrames: number;
    maximumFrames: number;
  };
  midiInput?: string;
  midiOutput?: string;
  midiOutputError?: string;
  ready: boolean;
  message?: string;
}

export type CoreEvent =
  | { type: "cursor"; generation: number; index: number; playedIndex?: number }
  | { type: "ready"; generation: number }
  | { type: "ended"; generation: number }
  | { type: "fault"; message: string };

export type BeatMidiInput =
  | { type: "down"; token: string; velocity: number }
  | { type: "up"; token: string };

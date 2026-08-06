// Copyright (c) 2026 Michael Saunders
//! Low-latency audio primitives for TapConductor.
//!
//! The audio callback path is deliberately small: a bounded SPSC queue feeds
//! fixed-size commands into [`AudioEngine`], which renders up to each command's
//! exact sample boundary before mutating the [`Sampler`]. No callback operation
//! allocates, locks, logs, or performs I/O.

mod command;
mod diagnostics;
mod engine;
mod piano;
mod queue;
mod sampled_piano;

pub mod backend;

pub use command::{
    AudioCommand, Chord, ChordError, Note, SampleTime, VoiceGroupId, MAX_NOTES_PER_CHORD,
};
pub use diagnostics::{AudioDiagnosticSnapshot, AudioDiagnostics};
pub use engine::{
    audio_engine, AudioCommandSender, AudioEngine, AudioRenderCallback, RenderCallbackInfo,
    RenderStatus, Sampler, VoiceStart,
};
pub use piano::{PianoConfig, PianoSynth};
pub use queue::{spsc_channel, Consumer, Producer, QueueFull};
pub use sampled_piano::{PianoInstrument, SalamanderBank, SalamanderLoadError, SampledPiano};

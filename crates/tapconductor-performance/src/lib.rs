// Copyright (c) 2026 Michael Saunders
//! Deterministic, device-independent performance state for TapConductor.
//!
//! The engine deliberately deals only in audio sample positions. Callers map
//! native input timestamps onto the active audio stream's monotonic sample
//! clock before submitting commands. No UI timer or wall clock participates in
//! note release decisions.

#![forbid(unsafe_code)]

mod engine;
mod gate;
mod types;

pub use engine::{ActiveGroup, EngineConfig, EngineError, PerformanceCommand, PerformanceEngine};
pub use gate::{DefaultPianoGate, GateError, GatePolicy};
pub use types::{
    AudioCommand, Chord, ChordError, ChordRollOrder, EventId, Generation, IgnoreReason, InputId,
    MidiPitch, PerformanceEvent, SafetyReason, SampleRate, SampleTime, ScoreSequence,
    ScoreSequenceError, Slice, SliceReleaseBoundary, StaffSlice, Transition, TriggerKind, Velocity,
    VoiceGroupId, MAX_CHORD_NOTES, TRANSITION_AUDIO_CAPACITY,
};

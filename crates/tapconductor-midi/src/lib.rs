// Copyright (c) 2026 Michael Saunders
//! MIDI 1.0 input/output primitives for TapConductor.
//!
//! The semantic API uses 16-bit velocities and timestamped messages so the
//! performance model does not need to change when higher-resolution MIDI
//! backends are added. The optional `midir-backend` feature supplies the first
//! Windows/macOS implementation; tests and the core mapper need no native MIDI
//! dependency.

pub mod backend;
mod mapping;
mod message;
mod output;

pub use mapping::{
    MapResult, MidiInputConfig, MidiInputMapper, MidiInputToken, MidiTapEvent, VelocityCurve,
};
pub use message::{
    parse_midi1, Midi1Packet, MidiChannel, MidiMessage, MidiNote, MidiParseError, MidiTimestamp,
    TimestampedMidiMessage, Velocity,
};
pub use output::{
    MidiOutChord, MidiOutError, MidiOutGroupId, MidiOutNote, MidiOutState, MAX_MIDI_OUT_CHORD_NOTES,
};

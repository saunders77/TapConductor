use std::path::PathBuf;

use thiserror::Error;

/// Failures that make a score unsafe to use as a live performance timeline.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("could not read score file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("input exceeds the configured {limit_name} limit ({actual} > {limit} bytes)")]
    InputTooLarge {
        limit_name: &'static str,
        actual: u64,
        limit: u64,
    },

    #[error("invalid MusicXML: {0}")]
    InvalidXml(String),

    #[error("invalid compressed MusicXML archive: {0}")]
    InvalidArchive(String),

    #[error("compressed MusicXML archive has no valid META-INF/container.xml rootfile")]
    MissingMxlRootfile,

    #[error("unsupported score document type: {0}")]
    UnsupportedDocument(String),

    #[error("unsupported playback navigation at {context}: {construct}")]
    UnsupportedNavigation { context: String, construct: String },

    #[error("invalid score timing at {context}: {message}")]
    InvalidTiming { context: String, message: String },

    #[error("score exceeds the configured {kind} limit ({limit})")]
    ResourceLimit { kind: &'static str, limit: usize },

    #[error("invalid Standard MIDI File: {0}")]
    InvalidMidi(String),

    #[error("SMPTE/timecode MIDI timing is not supported")]
    UnsupportedMidiTiming,

    #[error("MIDI format 2 (asynchronous patterns) is not supported")]
    UnsupportedMidiFormat,

    #[error(transparent)]
    Rational(#[from] RationalError),
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RationalError {
    #[error("a rational denominator cannot be zero")]
    ZeroDenominator,
    #[error("rational arithmetic overflow")]
    Overflow,
    #[error("invalid rational number: {0}")]
    InvalidNumber(&'static str),
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum PartSelectionError {
    #[error("unknown score part: {0}")]
    UnknownPart(String),
}

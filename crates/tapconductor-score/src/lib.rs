//! Native, deterministic score ingestion for TapConductor.
//!
//! This crate deliberately does not use engraving coordinates to decide which notes are
//! simultaneous. MusicXML divisions (or MIDI ticks) are converted to reduced [`Rational`] score
//! positions, ties are resolved, and attacks at equal positions are grouped into [`TapEvent`]s.

mod diagnostic;
mod error;
mod import;
mod model;
mod rational;

pub use diagnostic::{ImportWarning, SourceContext, WarningCode, WarningSeverity};
pub use error::{ImportError, PartSelectionError, RationalError};
pub use import::{
    display_musicxml_text, import_bytes, import_midi, import_musicxml, import_mxl, import_path,
    ImportOptions,
};
pub use model::{
    NormalizedScore, NoteAttack, PartInfo, ScoreFormat, ScoreMetadata, ScorePosition, SourceAnchor,
    SpelledPitch, Step, TapEvent, TieInfo,
};
pub use rational::Rational;

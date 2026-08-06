// Copyright (c) 2026 Michael Saunders
use serde::{Deserialize, Serialize};

/// Stable warning identifiers suitable for UI localization and import telemetry.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningCode {
    GraceNoteSkipped,
    CueNoteSkipped,
    HiddenNoteSkipped,
    UnpitchedNoteSkipped,
    MicrotonalPitchSkipped,
    PitchOutOfRange,
    MissingPitch,
    InvalidDuration,
    OverfullMeasure,
    InconsistentMeasureDuration,
    UnmatchedTieStop,
    UnterminatedTie,
    ReplacedOpenTie,
    UnsupportedElement,
    EmptyPart,
    MidiNoteWithoutOff,
    MidiNoteOffWithoutOn,
    MidiMetaIgnored,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningSeverity {
    Info,
    Warning,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceContext {
    pub part_id: Option<String>,
    pub measure_id: Option<String>,
    pub measure_index: Option<usize>,
    pub source_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    pub code: WarningCode,
    pub severity: WarningSeverity,
    pub message: String,
    pub context: SourceContext,
}

impl ImportWarning {
    pub(crate) fn warning(
        code: WarningCode,
        message: impl Into<String>,
        context: SourceContext,
    ) -> Self {
        Self {
            code,
            severity: WarningSeverity::Warning,
            message: message.into(),
            context,
        }
    }

    pub(crate) fn info(
        code: WarningCode,
        message: impl Into<String>,
        context: SourceContext,
    ) -> Self {
        Self {
            code,
            severity: WarningSeverity::Info,
            message: message.into(),
            context,
        }
    }
}

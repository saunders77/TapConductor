// Copyright (c) 2026 Michael Saunders
use serde::Serialize;
use std::path::Path;
use tapconductor_score::{NormalizedScore, Rational, ScoreFormat, TapEvent};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RationalDto {
    numerator: i64,
    denominator: i64,
}

impl From<Rational> for RationalDto {
    fn from(value: Rational) -> Self {
        Self {
            numerator: value.numerator(),
            denominator: value.denominator(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    source_id: String,
    part_id: String,
    part_index: usize,
    staff: u16,
    voice: String,
    midi_pitch: u8,
    is_grace: bool,
    is_staccato: bool,
    end: RationalDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TapEventDto {
    id: String,
    index: usize,
    measure_index: usize,
    measure_number: String,
    occurrence: u32,
    absolute: RationalDto,
    offset: RationalDto,
    position_order: u32,
    notes: Vec<NoteDto>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeatDto {
    absolute: RationalDto,
    measure_index: usize,
    beat_index: u32,
    beats_in_measure: u32,
    beat_type: u32,
}

impl TapEventDto {
    fn from_event(index: usize, event: &TapEvent) -> Self {
        Self {
            id: event.id.clone(),
            index,
            measure_index: event.position.measure_index,
            measure_number: event.position.measure_id.clone(),
            occurrence: event.position.occurrence,
            absolute: event.position.absolute.into(),
            offset: event.position.offset.into(),
            position_order: event.position.position_order,
            notes: event
                .attacks
                .iter()
                .map(|attack| NoteDto {
                    source_id: attack.source_id.clone(),
                    part_id: attack.source_anchor.part_id.clone(),
                    part_index: attack.part_index,
                    staff: attack.staff,
                    voice: attack.voice.clone(),
                    midi_pitch: attack.midi_pitch,
                    is_grace: attack.position_order != u32::MAX,
                    is_staccato: attack.staccato,
                    end: attack.end.into(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartDto {
    id: String,
    name: String,
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreFormatDto {
    MusicXml,
    Midi,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedScoreDto {
    generation: u64,
    path: String,
    display_name: String,
    format: ScoreFormatDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    music_xml: Option<String>,
    events: Vec<TapEventDto>,
    beats: Vec<BeatDto>,
    parts: Vec<PartDto>,
    warnings: Vec<String>,
    structural_duration: Option<RationalDto>,
}

impl LoadedScoreDto {
    pub fn new(
        generation: u64,
        path: &Path,
        score: &NormalizedScore,
        events: &[TapEvent],
        enabled_part_ids: &std::collections::BTreeSet<String>,
        music_xml: Option<String>,
    ) -> Self {
        let display_name = score
            .metadata
            .title
            .as_deref()
            .or(score.metadata.movement_title.as_deref())
            .map(str::to_owned)
            .or_else(|| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled score".to_owned());
        let beats = score
            .playback_measures
            .iter()
            .enumerate()
            .flat_map(|(measure_index, measure)| {
                let beat_length = Rational::new(4, i64::from(measure.beat_type))
                    .expect("imported time-signature denominators are positive");
                (0..measure.beats).map(move |beat_index| BeatDto {
                    absolute: measure
                        .start
                        .checked_add(
                            beat_length
                                .checked_mul_i64(i64::from(beat_index))
                                .expect("bounded beat index multiplication"),
                        )
                        .expect("validated score beat position")
                        .into(),
                    measure_index,
                    beat_index,
                    beats_in_measure: measure.beats,
                    beat_type: measure.beat_type,
                })
            })
            .collect();
        Self {
            generation,
            path: path.to_string_lossy().into_owned(),
            display_name,
            format: match score.format {
                ScoreFormat::MusicXml => ScoreFormatDto::MusicXml,
                ScoreFormat::Midi => ScoreFormatDto::Midi,
            },
            music_xml,
            events: events
                .iter()
                .enumerate()
                .map(|(index, event)| TapEventDto::from_event(index, event))
                .collect(),
            beats,
            parts: score
                .parts
                .iter()
                .map(|part| PartDto {
                    id: part.id.clone(),
                    name: part.name.clone(),
                    enabled: enabled_part_ids.contains(&part.id),
                })
                .collect(),
            warnings: score
                .warnings
                .iter()
                .map(|warning| warning.message.clone())
                .collect(),
            structural_duration: score.playback_measures.last().and_then(|measure| {
                measure
                    .start
                    .checked_add(measure.duration)
                    .ok()
                    .map(Into::into)
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiPortsDto {
    pub inputs: Vec<DeviceDto>,
    pub outputs: Vec<DeviceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_output: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDto {
    pub audio_backend: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub buffer_frames: u32,
    pub estimated_latency_ms: f64,
    /// CPAL does not currently expose a reliable underrun counter. Keep the
    /// compatibility field at zero until the direct WASAPI renderer can
    /// measure it rather than presenting backend errors as underruns.
    pub callback_underruns: u64,
    pub backend_errors: u64,
    pub late_commands: u64,
    pub invalid_audio_buffers: u64,
    pub voice_steals: u64,
    pub queue_overflows: u64,
    pub active_voices: u32,
    pub direct_wasapi_stream: bool,
    pub asio_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasapi_periods: Option<WasapiPeriodsDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_input: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_output_error: Option<String>,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasapiPeriodsDto {
    pub sample_rate: u32,
    pub channels: u16,
    pub default_frames: u32,
    pub fundamental_frames: u32,
    pub minimum_frames: u32,
    pub maximum_frames: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CoreEventDto {
    Cursor {
        generation: u64,
        index: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        played_index: Option<usize>,
    },
    Ready {
        generation: u64,
    },
    Ended {
        generation: u64,
    },
    Fault {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum BeatMidiInputDto {
    Down { token: String, velocity: u8 },
    Up { token: String },
}

#[cfg(test)]
mod tests {
    use super::CoreEventDto;

    #[test]
    fn event_payload_fields_use_frontend_camel_case() {
        let json = serde_json::to_value(CoreEventDto::Cursor {
            generation: 7,
            index: 3,
            played_index: Some(2),
        })
        .expect("event serializes");
        assert_eq!(json["type"], "cursor");
        assert_eq!(json["playedIndex"], 2);
        assert!(json.get("played_index").is_none());
    }
}

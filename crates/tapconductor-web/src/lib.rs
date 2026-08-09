// Copyright (c) 2026 Michael Saunders
//! Small WebAssembly boundary around the platform-neutral score importer.
//!
//! Audio, browser MIDI, and file selection deliberately remain in TypeScript,
//! where they can use the browser APIs directly. Keeping score normalization
//! here ensures native and web builds agree about repeats, ties, parts, and
//! exact musical positions.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use tapconductor_score::{
    display_musicxml_text, import_bytes, ImportOptions, NormalizedScore, Rational, ScoreFormat,
    TapEvent,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WebScore {
    generation: u64,
    file_name: String,
    score: NormalizedScore,
    active_parts: BTreeSet<String>,
    music_xml: Option<String>,
}

#[wasm_bindgen]
impl WebScore {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8], file_name: String) -> Result<WebScore, JsValue> {
        let options = ImportOptions::default();
        let score = import_bytes(bytes, &options).map_err(js_error)?;
        let music_xml = display_musicxml_text(bytes, &options).map_err(js_error)?;
        let active_parts = score.all_part_ids();
        let events = score
            .tap_events_for_parts(&active_parts)
            .map_err(js_error)?;
        if events.is_empty() {
            return Err(JsValue::from_str(
                "The selected score contains no playable pitched note attacks.",
            ));
        }
        Ok(Self {
            generation: 1,
            file_name,
            score,
            active_parts,
            music_xml,
        })
    }

    pub fn set_part_enabled(&mut self, part_id: String, enabled: bool) -> Result<(), JsValue> {
        let mut next = self.active_parts.clone();
        if enabled {
            next.insert(part_id);
        } else {
            next.remove(&part_id);
        }
        let events = self.score.tap_events_for_parts(&next).map_err(js_error)?;
        if events.is_empty() {
            return Err(JsValue::from_str(
                "At least one enabled part must contain a playable pitched note attack.",
            ));
        }
        self.active_parts = next;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| JsValue::from_str("Score generation counter exhausted."))?;
        Ok(())
    }

    pub fn dto_json(&self) -> Result<String, JsValue> {
        let events = self
            .score
            .tap_events_for_parts(&self.active_parts)
            .map_err(js_error)?;
        let display_name = self
            .score
            .metadata
            .title
            .as_deref()
            .or(self.score.metadata.movement_title.as_deref())
            .unwrap_or(&self.file_name);
        let dto = serde_json::json!({
            "generation": self.generation,
            "path": self.file_name,
            "displayName": display_name,
            "format": match self.score.format {
                ScoreFormat::MusicXml => "music_xml",
                ScoreFormat::Midi => "midi",
            },
            "musicXml": self.music_xml,
            "events": events.iter().enumerate().map(event_json).collect::<Vec<_>>(),
            "beats": self.score.playback_beats().into_iter().map(|beat| serde_json::json!({
                "absolute": rational_json(beat.absolute),
                "measureIndex": beat.measure_index,
                "beatIndex": beat.beat_index,
                "beatsInMeasure": beat.beats_in_measure,
                "beatType": beat.beat_type,
            })).collect::<Vec<_>>(),
            "parts": self.score.parts.iter().map(|part| serde_json::json!({
                "id": part.id,
                "name": part.name,
                "enabled": self.active_parts.contains(&part.id),
            })).collect::<Vec<_>>(),
            "warnings": &self.score.warnings,
            "structuralDuration": self.score.playback_measures.last().and_then(|measure| {
                measure.start.checked_add(measure.duration).ok().map(rational_json)
            }),
        });
        serde_json::to_string(&dto).map_err(js_error)
    }
}

fn event_json((index, event): (usize, &TapEvent)) -> serde_json::Value {
    serde_json::json!({
        "id": event.id,
        "index": index,
        "measureIndex": event.position.measure_index,
        "measureNumber": event.position.measure_id,
        "occurrence": event.position.occurrence,
        "absolute": rational_json(event.position.absolute),
        "offset": rational_json(event.position.offset),
        "notes": event.attacks.iter().map(|attack| serde_json::json!({
            "sourceId": attack.source_id,
            "partId": attack.source_anchor.part_id,
            "partIndex": attack.part_index,
            "staff": attack.staff,
            "voice": attack.voice,
            "midiPitch": attack.midi_pitch,
            "isStaccato": attack.staccato,
            "end": rational_json(attack.end),
        })).collect::<Vec<_>>(),
    })
}

fn rational_json(value: Rational) -> serde_json::Value {
    serde_json::json!({
        "numerator": value.numerator(),
        "denominator": value.denominator(),
    })
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

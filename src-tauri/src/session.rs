use crate::dto::LoadedScoreDto;
use std::{collections::BTreeSet, fs, path::PathBuf};
use tapconductor_score::{ImportOptions, NormalizedScore, TapEvent};

pub struct ScoreSession {
    generation: u64,
    path: PathBuf,
    score: NormalizedScore,
    active_parts: BTreeSet<String>,
    events: Vec<TapEvent>,
    music_xml: Option<String>,
}

impl ScoreSession {
    pub fn load(path: PathBuf, generation: u64) -> Result<Self, String> {
        let options = ImportOptions::default();
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("Unable to inspect {}: {error}", path.display()))?;
        if metadata.len() > options.max_input_bytes {
            return Err(format!(
                "The selected score is {} bytes; the input limit is {} bytes.",
                metadata.len(),
                options.max_input_bytes
            ));
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
        let score = tapconductor_score::import_bytes(&bytes, &options)
            .map_err(|error| error.to_string())?;
        let active_parts = score.all_part_ids();
        let events = score
            .tap_events_for_parts(&active_parts)
            .map_err(|error| error.to_string())?;
        let music_xml = tapconductor_score::display_musicxml_text(&bytes, &options)
            .map_err(|error| error.to_string())?;
        if events.is_empty() {
            return Err("The selected score contains no playable pitched note attacks.".to_owned());
        }
        Ok(Self {
            generation,
            path,
            score,
            active_parts,
            events,
            music_xml,
        })
    }

    pub fn set_generation(&mut self, generation: u64) {
        self.generation = generation;
    }

    pub fn events(&self) -> &[TapEvent] {
        &self.events
    }

    pub fn sounding_pitches_at(&self, index: usize) -> Result<Vec<u8>, String> {
        let position = self
            .events
            .get(index)
            .ok_or_else(|| format!("Score event index {index} is out of range."))?
            .position
            .absolute;
        let mut pitches: Vec<u8> = self
            .events
            .iter()
            .take(index + 1)
            .flat_map(|event| event.attacks.iter())
            .filter(|attack| attack.onset <= position && position < attack.end)
            .map(|attack| attack.midi_pitch)
            .collect();
        pitches.sort_unstable();
        pitches.dedup();
        Ok(pitches)
    }

    pub fn set_part_enabled(&mut self, part_id: &str, enabled: bool) -> Result<(), String> {
        if !self.score.parts.iter().any(|part| part.id == part_id) {
            return Err(format!("Unknown score part '{part_id}'."));
        }
        let mut active_parts = self.active_parts.clone();
        if enabled {
            active_parts.insert(part_id.to_owned());
        } else {
            active_parts.remove(part_id);
        }
        let events = self
            .score
            .tap_events_for_parts(&active_parts)
            .map_err(|error| error.to_string())?;
        if events.is_empty() {
            return Err(
                "At least one enabled part must contain a playable pitched note attack.".to_owned(),
            );
        }
        self.active_parts = active_parts;
        self.events = events;
        Ok(())
    }

    pub fn dto(&self) -> LoadedScoreDto {
        LoadedScoreDto::new(
            self.generation,
            &self.path,
            &self.score,
            &self.events,
            &self.active_parts,
            self.music_xml.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ScoreSession;
    use std::path::PathBuf;

    #[test]
    fn sounding_pitches_include_held_notes_but_not_notes_ending_at_the_position() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/tapconductor-score/tests/fixtures/ties.musicxml");
        let session = ScoreSession::load(fixture, 1).unwrap();

        assert_eq!(session.sounding_pitches_at(1).unwrap(), vec![60, 62]);
        assert_eq!(session.sounding_pitches_at(2).unwrap(), vec![64]);
    }

    #[test]
    fn bundled_demo_score_is_importable() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../assets/demo/TapConductor-Demo.musicxml");
        let session = ScoreSession::load(fixture, 1).unwrap();

        assert!(session.events().len() >= 10);
    }
}

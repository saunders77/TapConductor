// Copyright (c) 2026 Michael Saunders
use crate::dto::LoadedScoreDto;
use std::{collections::BTreeSet, fs::File, io::Read, path::PathBuf};
use tapconductor_score::{ImportOptions, NormalizedScore, NoteAttack, Rational, TapEvent};

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
        let mut file = File::open(&path)
            .map_err(|error| format!("Unable to open {}: {error}", path.display()))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("Unable to inspect {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "The selected score is not a regular file: {}",
                path.display()
            ));
        }
        if metadata.len() > options.max_input_bytes {
            return Err(format!(
                "The selected score is {} bytes; the input limit is {} bytes.",
                metadata.len(),
                options.max_input_bytes
            ));
        }
        let capacity = usize::try_from(metadata.len().min(options.max_input_bytes)).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        file.by_ref()
            .take(options.max_input_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > options.max_input_bytes {
            return Err(format!(
                "The selected score exceeds the input limit of {} bytes.",
                options.max_input_bytes
            ));
        }
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
        Ok(sounding_pitches_in_staff_order(
            self.events
                .iter()
                .take(index + 1)
                .flat_map(|event| event.attacks.iter()),
            position,
        ))
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

fn sounding_pitches_in_staff_order<'a>(
    attacks: impl Iterator<Item = &'a NoteAttack>,
    position: Rational,
) -> Vec<u8> {
    let mut attacks: Vec<_> = attacks
        .filter(|attack| attack.onset <= position && position < attack.end)
        .collect();
    // Parts and staffs are laid out top-to-bottom in ascending index order.
    // Audition the visual stack from bottom to top, while retaining the
    // conventional low-to-high roll within each individual staff.
    attacks.sort_by(|left, right| {
        right
            .part_index
            .cmp(&left.part_index)
            .then(right.staff.cmp(&left.staff))
            .then(left.midi_pitch.cmp(&right.midi_pitch))
            .then(left.source_id.cmp(&right.source_id))
    });
    let mut seen = BTreeSet::new();
    attacks
        .into_iter()
        .map(|attack| attack.midi_pitch)
        .filter(|pitch| seen.insert(*pitch))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ScoreSession, sounding_pitches_in_staff_order};
    use std::path::PathBuf;
    use tapconductor_score::{NoteAttack, Rational, SourceAnchor, TieInfo};

    fn attack(part_index: usize, staff: u16, pitch: u8) -> NoteAttack {
        let source_id = format!("{part_index}-{staff}-{pitch}");
        NoteAttack {
            source_anchor: SourceAnchor {
                source_id: source_id.clone(),
                xml_id: None,
                part_id: format!("part-{part_index}"),
                measure_id: "1".to_owned(),
                measure_index: 0,
                occurrence: 1,
                offset: Rational::ZERO,
                staff,
                voice: "1".to_owned(),
            },
            source_id,
            part_index,
            staff,
            voice: "1".to_owned(),
            written_pitch: None,
            midi_pitch: pitch,
            midi_channel: None,
            onset: Rational::ZERO,
            position_order: u32::MAX,
            end: Rational::from_integer(2),
            staccato: false,
            tie: TieInfo::default(),
            velocity_hint: None,
        }
    }

    #[test]
    fn sounding_pitches_roll_bottom_staff_first_even_when_it_is_higher() {
        let attacks = [
            attack(0, 1, 40),
            attack(0, 2, 80),
            attack(0, 2, 70),
            attack(1, 1, 90),
        ];

        assert_eq!(
            sounding_pitches_in_staff_order(attacks.iter(), Rational::ONE),
            vec![90, 70, 80, 40]
        );
    }

    #[test]
    fn sounding_pitches_include_held_notes_but_not_notes_ending_at_the_position() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/tapconductor-score/tests/fixtures/ties.musicxml");
        let session = ScoreSession::load(fixture, 1).unwrap();

        assert_eq!(session.sounding_pitches_at(1).unwrap(), vec![60, 62]);
        assert_eq!(session.sounding_pitches_at(2).unwrap(), vec![64]);
    }

    #[test]
    fn bundled_demo_scores_are_importable() {
        for file_name in [
            "Prelude in C Minor - Chopin 1839.musicxml",
            "All-Night Vigil - Rachmaninoff 1915.musicxml",
        ] {
            let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../assets/demo")
                .join(file_name);
            let session = ScoreSession::load(fixture, 1).unwrap();

            assert!(
                !session.events().is_empty(),
                "{file_name} has no playable events"
            );
        }
    }
}

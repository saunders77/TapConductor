// Copyright (c) 2026 Michael Saunders
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ImportWarning, PartSelectionError, Rational};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScoreFormat {
    MusicXml,
    Midi,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreMetadata {
    pub title: Option<String>,
    pub movement_title: Option<String>,
    pub composer: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartInfo {
    pub id: String,
    pub name: String,
    pub abbreviation: Option<String>,
    pub order: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Step {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}

impl Step {
    pub(crate) const fn semitone(self) -> i16 {
        match self {
            Self::C => 0,
            Self::D => 2,
            Self::E => 4,
            Self::F => 5,
            Self::G => 7,
            Self::A => 9,
            Self::B => 11,
        }
    }
}

/// Written pitch is retained for display and diagnostics; `NoteAttack::midi_pitch` is concert
/// pitch and is the value used by piano and MIDI output modes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpelledPitch {
    pub step: Step,
    pub alter: Rational,
    pub octave: i8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAnchor {
    /// Stable importer-owned ID, unique within the source score.
    pub source_id: String,
    /// Original MusicXML ID when one was present.
    pub xml_id: Option<String>,
    pub part_id: String,
    pub measure_id: String,
    pub measure_index: usize,
    /// One-based occurrence of this visual measure in expanded playback order.
    pub occurrence: u32,
    pub offset: Rational,
    pub staff: u16,
    pub voice: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TieInfo {
    pub starts_tie: bool,
    /// Anchors of tied continuations suppressed from the tap timeline.
    pub continuations: Vec<SourceAnchor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteAttack {
    pub source_id: String,
    pub source_anchor: SourceAnchor,
    pub part_index: usize,
    pub staff: u16,
    pub voice: String,
    pub written_pitch: Option<SpelledPitch>,
    /// Concert-pitch MIDI note number (0 through 127).
    pub midi_pitch: u8,
    /// Optional zero-based source MIDI channel.
    pub midi_channel: Option<u8>,
    pub onset: Rational,
    /// Orders this attack among playable moments at the same notated onset.
    /// `u32::MAX` denotes the principal rhythmic event after any grace notes.
    pub position_order: u32,
    pub end: Rational,
    /// Whether the source notation marks this attack staccato.
    #[serde(default)]
    pub staccato: bool,
    pub tie: TieInfo,
    pub velocity_hint: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScorePosition {
    pub absolute: Rational,
    /// Orders distinct playable moments that share the same notated timestamp.
    /// Grace-note groups use ascending values; the principal rhythmic event is last.
    pub position_order: u32,
    pub measure_index: usize,
    pub measure_id: String,
    pub occurrence: u32,
    pub offset: Rational,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TapEvent {
    /// Stable across active-part filtering because it is derived from the absolute playback onset.
    pub id: String,
    pub position: ScorePosition,
    pub attacks: Vec<NoteAttack>,
    pub release_boundaries: Vec<Rational>,
    pub display_anchors: Vec<SourceAnchor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackMeasureInfo {
    pub source_measure_index: usize,
    pub measure_id: String,
    pub occurrence: u32,
    pub start: Rational,
    pub duration: Rational,
    pub beats: u32,
    pub beat_type: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedScore {
    pub format: ScoreFormat,
    pub metadata: ScoreMetadata,
    pub parts: Vec<PartInfo>,
    /// All playable, tie-resolved attacks in expanded playback order.
    pub attacks: Vec<NoteAttack>,
    /// Tap events with every part active.
    pub tap_events: Vec<TapEvent>,
    pub playback_measure_count: usize,
    pub playback_measures: Vec<PlaybackMeasureInfo>,
    pub warnings: Vec<ImportWarning>,
}

impl NormalizedScore {
    pub(crate) fn new(
        format: ScoreFormat,
        metadata: ScoreMetadata,
        parts: Vec<PartInfo>,
        mut attacks: Vec<NoteAttack>,
        playback_measure_count: usize,
        warnings: Vec<ImportWarning>,
    ) -> Self {
        sort_attacks(&mut attacks);
        let tap_events = group_attacks(attacks.iter());
        let playback_measures = (0..playback_measure_count)
            .map(|index| PlaybackMeasureInfo {
                source_measure_index: index,
                measure_id: (index + 1).to_string(),
                occurrence: 1,
                start: Rational::from_integer((index as i64).saturating_mul(4)),
                duration: Rational::from_integer(4),
                beats: 4,
                beat_type: 4,
            })
            .collect();
        Self {
            format,
            metadata,
            parts,
            attacks,
            tap_events,
            playback_measure_count,
            playback_measures,
            warnings,
        }
    }

    /// Rebuild tap events for the selected parts without reparsing or re-resolving ties.
    ///
    /// An empty set intentionally produces no tap events. Unknown IDs are rejected to prevent a UI
    /// typo from silently muting a part during a live performance.
    pub fn tap_events_for_parts(
        &self,
        active_parts: &BTreeSet<String>,
    ) -> Result<Vec<TapEvent>, PartSelectionError> {
        let known: BTreeSet<&str> = self.parts.iter().map(|part| part.id.as_str()).collect();
        if let Some(unknown) = active_parts
            .iter()
            .find(|part_id| !known.contains(part_id.as_str()))
        {
            return Err(PartSelectionError::UnknownPart(unknown.clone()));
        }

        Ok(group_attacks(self.attacks.iter().filter(|attack| {
            active_parts.contains(&attack.source_anchor.part_id)
        })))
    }

    pub fn all_part_ids(&self) -> BTreeSet<String> {
        self.parts.iter().map(|part| part.id.clone()).collect()
    }
}

fn sort_attacks(attacks: &mut [NoteAttack]) {
    attacks.sort_by(|left, right| {
        left.onset
            .cmp(&right.onset)
            .then(left.position_order.cmp(&right.position_order))
            .then(left.part_index.cmp(&right.part_index))
            .then(left.staff.cmp(&right.staff))
            .then(left.voice.cmp(&right.voice))
            .then(left.midi_pitch.cmp(&right.midi_pitch))
            .then(left.source_id.cmp(&right.source_id))
    });
}

fn group_attacks<'a>(attacks: impl Iterator<Item = &'a NoteAttack>) -> Vec<TapEvent> {
    let mut positions: BTreeMap<(Rational, u32), Vec<NoteAttack>> = BTreeMap::new();
    for attack in attacks {
        positions
            .entry((attack.onset, attack.position_order))
            .or_default()
            .push(attack.clone());
    }

    positions
        .into_iter()
        .map(|((onset, position_order), mut attacks)| {
            sort_attacks(&mut attacks);
            let representative = &attacks[0].source_anchor;
            let position = ScorePosition {
                absolute: onset,
                position_order,
                measure_index: representative.measure_index,
                measure_id: representative.measure_id.clone(),
                occurrence: representative.occurrence,
                offset: representative.offset,
            };

            let mut release_boundaries: Vec<Rational> =
                attacks.iter().map(|attack| attack.end).collect();
            release_boundaries.sort_unstable();
            release_boundaries.dedup();

            let mut display_anchors = Vec::new();
            for attack in &attacks {
                display_anchors.push(attack.source_anchor.clone());
                display_anchors.extend(attack.tie.continuations.iter().cloned());
            }
            display_anchors.sort_by(|left, right| left.source_id.cmp(&right.source_id));
            display_anchors.dedup_by(|left, right| left.source_id == right.source_id);

            TapEvent {
                id: format!(
                    "at:{}:{}:{}",
                    onset.numerator(),
                    onset.denominator(),
                    position_order
                ),
                position,
                attacks,
                release_boundaries,
                display_anchors,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attack(part: &str, part_index: usize, onset: Rational, pitch: u8) -> NoteAttack {
        let anchor = SourceAnchor {
            source_id: format!("{part}-{pitch}"),
            xml_id: None,
            part_id: part.to_owned(),
            measure_id: "1".to_owned(),
            measure_index: 0,
            occurrence: 1,
            offset: onset,
            staff: 1,
            voice: "1".to_owned(),
        };
        NoteAttack {
            source_id: anchor.source_id.clone(),
            source_anchor: anchor,
            part_index,
            staff: 1,
            voice: "1".to_owned(),
            written_pitch: None,
            midi_pitch: pitch,
            midi_channel: None,
            onset,
            position_order: u32::MAX,
            end: onset.checked_add(Rational::ONE).unwrap(),
            staccato: false,
            tie: TieInfo::default(),
            velocity_hint: None,
        }
    }

    #[test]
    fn groups_exact_onsets_across_parts() {
        let score = NormalizedScore::new(
            ScoreFormat::MusicXml,
            ScoreMetadata::default(),
            vec![
                PartInfo {
                    id: "p1".into(),
                    name: "One".into(),
                    abbreviation: None,
                    order: 0,
                },
                PartInfo {
                    id: "p2".into(),
                    name: "Two".into(),
                    abbreviation: None,
                    order: 1,
                },
            ],
            vec![
                attack("p2", 1, Rational::ZERO, 67),
                attack("p1", 0, Rational::ONE, 62),
                attack("p1", 0, Rational::ZERO, 60),
            ],
            1,
            Vec::new(),
        );

        assert_eq!(score.tap_events.len(), 2);
        assert_eq!(
            score.tap_events[0]
                .attacks
                .iter()
                .map(|a| a.midi_pitch)
                .collect::<Vec<_>>(),
            vec![60, 67]
        );
    }

    #[test]
    fn active_parts_remove_empty_positions_and_validate_ids() {
        let score = NormalizedScore::new(
            ScoreFormat::MusicXml,
            ScoreMetadata::default(),
            vec![
                PartInfo {
                    id: "p1".into(),
                    name: "One".into(),
                    abbreviation: None,
                    order: 0,
                },
                PartInfo {
                    id: "p2".into(),
                    name: "Two".into(),
                    abbreviation: None,
                    order: 1,
                },
            ],
            vec![
                attack("p1", 0, Rational::ZERO, 60),
                attack("p2", 1, Rational::ONE, 67),
            ],
            1,
            Vec::new(),
        );

        let only_p2 = BTreeSet::from(["p2".to_owned()]);
        let events = score.tap_events_for_parts(&only_p2).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].attacks[0].midi_pitch, 67);

        let unknown = BTreeSet::from(["missing".to_owned()]);
        assert_eq!(
            score.tap_events_for_parts(&unknown),
            Err(PartSelectionError::UnknownPart("missing".to_owned()))
        );
    }
}

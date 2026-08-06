// Copyright (c) 2026 Michael Saunders
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use midly::{Format, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

use super::ImportOptions;
use crate::{
    ImportError, ImportWarning, NormalizedScore, NoteAttack, PartInfo, Rational, ScoreFormat,
    ScoreMetadata, SourceAnchor, SourceContext, TieInfo, WarningCode,
};

#[derive(Clone, Debug)]
struct MidiNote {
    track: usize,
    channel: u8,
    pitch: u8,
    velocity: u8,
    onset_tick: u64,
    end_tick: Option<u64>,
    event_order: usize,
}

pub(super) fn parse_midi(
    bytes: &[u8],
    options: &ImportOptions,
) -> Result<NormalizedScore, ImportError> {
    let smf = Smf::parse(bytes).map_err(|error| ImportError::InvalidMidi(error.to_string()))?;
    if smf.header.format == Format::Sequential {
        return Err(ImportError::UnsupportedMidiFormat);
    }
    let ticks_per_quarter = match smf.header.timing {
        Timing::Metrical(ticks) => u64::from(ticks.as_int()),
        Timing::Timecode(_, _) => return Err(ImportError::UnsupportedMidiTiming),
    };
    if ticks_per_quarter == 0 {
        return Err(ImportError::InvalidMidi(
            "ticks per quarter note is zero".to_owned(),
        ));
    }

    let mut notes = Vec::new();
    let mut pending: BTreeMap<(usize, u8, u8), VecDeque<usize>> = BTreeMap::new();
    let mut track_names = vec![None; smf.tracks.len()];
    let mut warnings = Vec::new();
    let mut sequence_title = None;
    let mut event_order = 0_usize;

    for (track_index, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0_u64;
        for event in track {
            tick = tick
                .checked_add(u64::from(event.delta.as_int()))
                .ok_or_else(|| ImportError::InvalidMidi("absolute tick overflow".to_owned()))?;
            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    let channel = channel.as_int();
                    match message {
                        MidiMessage::NoteOn { key, vel } if vel.as_int() > 0 => {
                            if notes.len() >= options.max_notes {
                                return Err(ImportError::ResourceLimit {
                                    kind: "MIDI note count",
                                    limit: options.max_notes,
                                });
                            }
                            let index = notes.len();
                            notes.push(MidiNote {
                                track: track_index,
                                channel,
                                pitch: key.as_int(),
                                velocity: vel.as_int(),
                                onset_tick: tick,
                                end_tick: None,
                                event_order,
                            });
                            event_order = event_order.saturating_add(1);
                            pending
                                .entry((track_index, channel, key.as_int()))
                                .or_default()
                                .push_back(index);
                        }
                        MidiMessage::NoteOff { key, .. } | MidiMessage::NoteOn { key, vel: _ } => {
                            close_midi_note(
                                &mut notes,
                                &mut pending,
                                (track_index, channel, key.as_int()),
                                tick,
                                &mut warnings,
                            );
                        }
                        _ => {}
                    }
                }
                TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                    let name = String::from_utf8_lossy(name).trim().to_owned();
                    if !name.is_empty() {
                        if sequence_title.is_none() && track_index == 0 {
                            sequence_title = Some(name.clone());
                        }
                        track_names[track_index] = Some(name);
                    }
                }
                _ => {}
            }
        }
    }

    if notes.is_empty() {
        return Err(ImportError::InvalidMidi(
            "file contains no Note On events with non-zero velocity".to_owned(),
        ));
    }

    let fallback_duration = (ticks_per_quarter / 4).max(1);
    for queue in pending.values() {
        for index in queue {
            let note = &mut notes[*index];
            note.end_tick = Some(note.onset_tick.saturating_add(fallback_duration));
            warnings.push(ImportWarning::warning(
                WarningCode::MidiNoteWithoutOff,
                "MIDI note has no matching Note Off; a short fallback duration was assigned",
                SourceContext {
                    part_id: Some(midi_part_id(note.track)),
                    source_id: Some(midi_source_id(note.track, note.event_order)),
                    ..SourceContext::default()
                },
            ));
        }
    }

    notes.sort_by(|left, right| {
        left.onset_tick
            .cmp(&right.onset_tick)
            .then(left.track.cmp(&right.track))
            .then(left.event_order.cmp(&right.event_order))
    });

    let sounding_tracks: BTreeSet<usize> = notes.iter().map(|note| note.track).collect();
    let parts: Vec<PartInfo> = sounding_tracks
        .iter()
        .map(|track| PartInfo {
            id: midi_part_id(*track),
            name: track_names[*track]
                .clone()
                .unwrap_or_else(|| format!("Track {}", track + 1)),
            abbreviation: None,
            order: *track,
        })
        .collect();

    let measure_ticks = ticks_per_quarter
        .checked_mul(4)
        .ok_or_else(|| ImportError::InvalidMidi("measure tick span overflow".to_owned()))?;
    let mut attacks = Vec::with_capacity(notes.len());
    let mut playback_measure_count = 0_usize;
    for note in notes {
        let measure_index_u64 = note.onset_tick / measure_ticks;
        let measure_index =
            usize::try_from(measure_index_u64).map_err(|_| ImportError::ResourceLimit {
                kind: "MIDI pseudo-measure index",
                limit: options.max_playback_measures,
            })?;
        if measure_index >= options.max_playback_measures {
            return Err(ImportError::ResourceLimit {
                kind: "MIDI pseudo-measure count",
                limit: options.max_playback_measures,
            });
        }
        playback_measure_count = playback_measure_count.max(measure_index.saturating_add(1));
        let onset = ticks_to_rational(note.onset_tick, ticks_per_quarter)?;
        let end = ticks_to_rational(
            note.end_tick
                .unwrap_or(note.onset_tick.saturating_add(fallback_duration)),
            ticks_per_quarter,
        )?;
        let offset = ticks_to_rational(note.onset_tick % measure_ticks, ticks_per_quarter)?;
        let source_id = midi_source_id(note.track, note.event_order);
        let anchor = SourceAnchor {
            source_id: source_id.clone(),
            xml_id: None,
            part_id: midi_part_id(note.track),
            measure_id: (measure_index + 1).to_string(),
            measure_index,
            occurrence: 1,
            offset,
            staff: 1,
            voice: format!("channel-{}", note.channel + 1),
        };
        attacks.push(NoteAttack {
            source_id,
            source_anchor: anchor,
            part_index: note.track,
            staff: 1,
            voice: format!("channel-{}", note.channel + 1),
            written_pitch: None,
            midi_pitch: note.pitch,
            midi_channel: Some(note.channel),
            onset,
            position_order: u32::MAX,
            end,
            staccato: false,
            tie: TieInfo::default(),
            velocity_hint: Some(note.velocity),
        });
    }

    Ok(NormalizedScore::new(
        ScoreFormat::Midi,
        ScoreMetadata {
            title: sequence_title,
            movement_title: None,
            composer: None,
        },
        parts,
        attacks,
        playback_measure_count,
        warnings,
    ))
}

fn close_midi_note(
    notes: &mut [MidiNote],
    pending: &mut BTreeMap<(usize, u8, u8), VecDeque<usize>>,
    key: (usize, u8, u8),
    tick: u64,
    warnings: &mut Vec<ImportWarning>,
) {
    let index = pending.get_mut(&key).and_then(VecDeque::pop_front);
    if let Some(index) = index {
        notes[index].end_tick = Some(tick.max(notes[index].onset_tick));
        if pending.get(&key).is_some_and(VecDeque::is_empty) {
            pending.remove(&key);
        }
    } else {
        warnings.push(ImportWarning::info(
            WarningCode::MidiNoteOffWithoutOn,
            format!(
                "unmatched MIDI Note Off for channel {} pitch {} was ignored",
                key.1 + 1,
                key.2
            ),
            SourceContext {
                part_id: Some(midi_part_id(key.0)),
                ..SourceContext::default()
            },
        ));
    }
}

fn ticks_to_rational(ticks: u64, ticks_per_quarter: u64) -> Result<Rational, ImportError> {
    let ticks = i64::try_from(ticks)
        .map_err(|_| ImportError::InvalidMidi("absolute tick exceeds i64".to_owned()))?;
    let ticks_per_quarter = i64::try_from(ticks_per_quarter)
        .map_err(|_| ImportError::InvalidMidi("tick division exceeds i64".to_owned()))?;
    Ok(Rational::new(ticks, ticks_per_quarter)?)
}

fn midi_part_id(track: usize) -> String {
    format!("midi-track-{}", track + 1)
}

fn midi_source_id(track: usize, event_order: usize) -> String {
    format!("midi-track-{}/note-{}", track + 1, event_order + 1)
}

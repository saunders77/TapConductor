use crate::backend::{MidiBackendError, MidiOutputConnection};
use crate::{MidiChannel, MidiMessage, MidiNote, Velocity};
use core::fmt;

pub const MAX_MIDI_OUT_CHORD_NOTES: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiOutGroupId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiOutNote {
    pub note: MidiNote,
    pub velocity: Velocity,
}

impl Default for MidiOutNote {
    fn default() -> Self {
        Self {
            note: MidiNote::MIN,
            velocity: Velocity::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiOutChord {
    notes: [MidiOutNote; MAX_MIDI_OUT_CHORD_NOTES],
    len: u8,
}

impl MidiOutChord {
    pub const fn empty() -> Self {
        Self {
            notes: [MidiOutNote {
                note: MidiNote::MIN,
                velocity: Velocity::ZERO,
            }; MAX_MIDI_OUT_CHORD_NOTES],
            len: 0,
        }
    }

    pub fn try_from_slice(notes: &[MidiOutNote]) -> Result<Self, MidiOutError> {
        if notes.is_empty() {
            return Err(MidiOutError::EmptyChord);
        }
        if notes.len() > MAX_MIDI_OUT_CHORD_NOTES {
            return Err(MidiOutError::ChordTooLarge);
        }
        let mut result = Self::empty();
        result.notes[..notes.len()].copy_from_slice(notes);
        result.len = notes.len() as u8;
        Ok(result)
    }

    pub fn as_slice(&self) -> &[MidiOutNote] {
        &self.notes[..self.len as usize]
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy)]
struct ActiveOutputGroup {
    id: MidiOutGroupId,
    channel: MidiChannel,
    pitches: [u8; MAX_MIDI_OUT_CHORD_NOTES],
    len: u8,
}

impl ActiveOutputGroup {
    const EMPTY: Self = Self {
        id: MidiOutGroupId(0),
        channel: MidiChannel::from_status(0),
        pitches: [0; MAX_MIDI_OUT_CHORD_NOTES],
        len: 0,
    };
}

/// Tracks MIDI OUT voice groups without confusing overlapping repeated
/// pitches. MIDI 1.0 has no note IDs, so a Note Off is suppressed until the
/// last group using a channel/pitch pair is released.
pub struct MidiOutState<const GROUPS: usize = 128> {
    groups: [Option<ActiveOutputGroup>; GROUPS],
    pitch_references: [[u16; 128]; 16],
    used_channels: u16,
}

impl<const GROUPS: usize> Default for MidiOutState<GROUPS> {
    fn default() -> Self {
        assert!(GROUPS > 0, "MIDI OUT group capacity must be non-zero");
        Self {
            groups: [None; GROUPS],
            pitch_references: [[0; 128]; 16],
            used_channels: 0,
        }
    }
}

impl<const GROUPS: usize> MidiOutState<GROUPS> {
    pub fn active_group_count(&self) -> usize {
        self.groups.iter().filter(|group| group.is_some()).count()
    }

    /// Sends a chord immediately. The audio/sample scheduler should call this
    /// at the same target boundary as the internal sampler command.
    pub fn play_group(
        &mut self,
        output: &mut dyn MidiOutputConnection,
        group_id: MidiOutGroupId,
        channel: MidiChannel,
        chord: &MidiOutChord,
    ) -> Result<(), MidiOutError> {
        if self
            .groups
            .iter()
            .flatten()
            .any(|group| group.id == group_id)
        {
            return Err(MidiOutError::DuplicateGroup(group_id));
        }
        if chord.is_empty() {
            return Err(MidiOutError::EmptyChord);
        }

        for note in chord.as_slice() {
            self.play_group_note(output, group_id, channel, *note)?;
        }
        Ok(())
    }

    /// Adds one note to a voice group, creating the group when needed.
    ///
    /// This supports schedulers that roll a chord over time while preserving
    /// the same overlap-safe release semantics as [`Self::play_group`].
    pub fn play_group_note(
        &mut self,
        output: &mut dyn MidiOutputConnection,
        group_id: MidiOutGroupId,
        channel: MidiChannel,
        note: MidiOutNote,
    ) -> Result<(), MidiOutError> {
        let slot = if let Some(slot) = self
            .groups
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|group| group.id == group_id))
        {
            if self.groups[slot]
                .as_ref()
                .is_some_and(|group| group.channel != channel)
            {
                return Err(MidiOutError::DuplicateGroup(group_id));
            }
            slot
        } else {
            let slot = self
                .groups
                .iter()
                .position(Option::is_none)
                .ok_or(MidiOutError::GroupCapacity)?;
            self.groups[slot] = Some(ActiveOutputGroup {
                id: group_id,
                channel,
                ..ActiveOutputGroup::EMPTY
            });
            slot
        };

        let active = self.groups[slot].as_mut().expect("group slot is active");
        let pitch = note.note.get();
        // Multiple score parts may contain the same unison in one slice.
        if active.pitches[..active.len as usize].contains(&pitch) {
            return Ok(());
        }
        if active.len as usize == MAX_MIDI_OUT_CHORD_NOTES {
            return Err(MidiOutError::ChordTooLarge);
        }
        self.used_channels |= 1 << channel.zero_based();
        output
            .send(MidiMessage::NoteOn {
                channel,
                note: note.note,
                velocity: note.velocity,
            })
            .map_err(MidiOutError::Backend)?;
        let reference = &mut self.pitch_references[channel.zero_based() as usize][pitch as usize];
        *reference = reference.saturating_add(1);
        active.pitches[active.len as usize] = pitch;
        active.len += 1;
        Ok(())
    }

    pub fn release_group(
        &mut self,
        output: &mut dyn MidiOutputConnection,
        group_id: MidiOutGroupId,
    ) -> Result<(), MidiOutError> {
        let slot = self
            .groups
            .iter()
            .position(|entry| entry.as_ref().is_some_and(|group| group.id == group_id))
            .ok_or(MidiOutError::UnknownGroup(group_id))?;
        let group = self.groups[slot].take().expect("located group");
        let mut first_error = None;
        for pitch in &group.pitches[..group.len as usize] {
            let reference =
                &mut self.pitch_references[group.channel.zero_based() as usize][*pitch as usize];
            *reference = reference.saturating_sub(1);
            if *reference == 0 {
                let message = MidiMessage::NoteOff {
                    channel: group.channel,
                    note: MidiNote::new(*pitch).expect("stored valid MIDI pitch"),
                    velocity: Velocity::ZERO,
                };
                if let Err(error) = output.send(message) {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        first_error.map_or(Ok(()), |error| Err(MidiOutError::Backend(error)))
    }

    /// Sends both All Sound Off and All Notes Off on every used channel, then
    /// clears local tracking even if the device has disconnected.
    pub fn panic(&mut self, output: &mut dyn MidiOutputConnection) -> Result<(), MidiOutError> {
        let mut first_error = None;
        for channel_index in 0..16 {
            if self.used_channels & (1 << channel_index) == 0 {
                continue;
            }
            let channel = MidiChannel::new(channel_index as u8).expect("bounded channel");
            for controller in [120, 123] {
                if let Err(error) = output.send(MidiMessage::ControlChange {
                    channel,
                    controller,
                    value: 0,
                }) {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        self.groups.fill(None);
        self.pitch_references.fill([0; 128]);
        self.used_channels = 0;
        first_error.map_or(Ok(()), |error| Err(MidiOutError::Backend(error)))
    }
}

#[derive(Debug)]
pub enum MidiOutError {
    EmptyChord,
    ChordTooLarge,
    GroupCapacity,
    DuplicateGroup(MidiOutGroupId),
    UnknownGroup(MidiOutGroupId),
    Backend(MidiBackendError),
}

impl fmt::Display for MidiOutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChord => f.write_str("MIDI OUT chord is empty"),
            Self::ChordTooLarge => f.write_str("MIDI OUT chord exceeds fixed capacity"),
            Self::GroupCapacity => f.write_str("MIDI OUT voice-group capacity reached"),
            Self::DuplicateGroup(group) => {
                write!(f, "MIDI OUT group {:?} is already active", group)
            }
            Self::UnknownGroup(group) => write!(f, "MIDI OUT group {:?} is not active", group),
            Self::Backend(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for MidiOutError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingOutput(Vec<MidiMessage>);

    impl MidiOutputConnection for RecordingOutput {
        fn send(&mut self, message: MidiMessage) -> Result<(), MidiBackendError> {
            self.0.push(message);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailsSecondNoteOn {
        messages: Vec<MidiMessage>,
        note_on_attempts: usize,
    }

    impl MidiOutputConnection for FailsSecondNoteOn {
        fn send(&mut self, message: MidiMessage) -> Result<(), MidiBackendError> {
            if matches!(message, MidiMessage::NoteOn { .. }) {
                self.note_on_attempts += 1;
                if self.note_on_attempts == 2 {
                    return Err(MidiBackendError::new("test send", "simulated disconnect"));
                }
            }
            self.messages.push(message);
            Ok(())
        }
    }

    fn one_note(note: u8) -> MidiOutChord {
        MidiOutChord::try_from_slice(&[MidiOutNote {
            note: MidiNote::new(note).unwrap(),
            velocity: Velocity::MAX,
        }])
        .unwrap()
    }

    #[test]
    fn older_group_cannot_turn_off_retriggered_pitch() {
        let mut state = MidiOutState::<4>::default();
        let mut output = RecordingOutput::default();
        let channel = MidiChannel::new(0).unwrap();
        state
            .play_group(&mut output, MidiOutGroupId(1), channel, &one_note(60))
            .unwrap();
        state
            .play_group(&mut output, MidiOutGroupId(2), channel, &one_note(60))
            .unwrap();
        state.release_group(&mut output, MidiOutGroupId(1)).unwrap();
        assert_eq!(output.0.len(), 2, "first release must suppress Note Off");
        state.release_group(&mut output, MidiOutGroupId(2)).unwrap();
        assert!(matches!(output.0[2], MidiMessage::NoteOff { .. }));
    }

    #[test]
    fn releasing_a_chord_sends_note_off_for_every_unique_pitch() {
        let mut state = MidiOutState::<4>::default();
        let mut output = RecordingOutput::default();
        let chord = MidiOutChord::try_from_slice(&[
            MidiOutNote {
                note: MidiNote::new(60).unwrap(),
                velocity: Velocity::MAX,
            },
            MidiOutNote {
                note: MidiNote::new(64).unwrap(),
                velocity: Velocity::MAX,
            },
            MidiOutNote {
                note: MidiNote::new(67).unwrap(),
                velocity: Velocity::MAX,
            },
        ])
        .unwrap();
        state
            .play_group(
                &mut output,
                MidiOutGroupId(1),
                MidiChannel::new(0).unwrap(),
                &chord,
            )
            .unwrap();
        state.release_group(&mut output, MidiOutGroupId(1)).unwrap();

        let released: Vec<u8> = output
            .0
            .iter()
            .filter_map(|message| match message {
                MidiMessage::NoteOff { note, .. } => Some(note.get()),
                _ => None,
            })
            .collect();
        assert_eq!(released, vec![60, 64, 67]);
    }

    #[test]
    fn notes_can_be_added_to_a_group_for_rolled_output() {
        let mut state = MidiOutState::<4>::default();
        let mut output = RecordingOutput::default();
        let channel = MidiChannel::new(0).unwrap();
        for pitch in [60, 64, 67] {
            state
                .play_group_note(
                    &mut output,
                    MidiOutGroupId(1),
                    channel,
                    MidiOutNote {
                        note: MidiNote::new(pitch).unwrap(),
                        velocity: Velocity::MAX,
                    },
                )
                .unwrap();
        }
        state.release_group(&mut output, MidiOutGroupId(1)).unwrap();
        assert_eq!(output.0.len(), 6);
    }

    #[test]
    fn panic_sends_safety_controllers_and_clears_tracking() {
        let mut state = MidiOutState::<4>::default();
        let mut output = RecordingOutput::default();
        state
            .play_group(
                &mut output,
                MidiOutGroupId(1),
                MidiChannel::new(2).unwrap(),
                &one_note(64),
            )
            .unwrap();
        state.panic(&mut output).unwrap();
        assert_eq!(state.active_group_count(), 0);
        assert!(output.0.iter().any(|message| matches!(
            message,
            MidiMessage::ControlChange {
                controller: 120,
                ..
            }
        )));
        assert!(output.0.iter().any(|message| matches!(
            message,
            MidiMessage::ControlChange {
                controller: 123,
                ..
            }
        )));
    }

    #[test]
    fn partial_chord_failure_still_arms_channel_panic() {
        let mut state = MidiOutState::<4>::default();
        let mut output = FailsSecondNoteOn::default();
        let chord = MidiOutChord::try_from_slice(&[
            MidiOutNote {
                note: MidiNote::new(60).unwrap(),
                velocity: Velocity::MAX,
            },
            MidiOutNote {
                note: MidiNote::new(64).unwrap(),
                velocity: Velocity::MAX,
            },
        ])
        .unwrap();
        assert!(state
            .play_group(
                &mut output,
                MidiOutGroupId(1),
                MidiChannel::new(0).unwrap(),
                &chord,
            )
            .is_err());

        state.panic(&mut output).unwrap();
        assert!(output.messages.iter().any(|message| matches!(
            message,
            MidiMessage::ControlChange {
                controller: 120,
                ..
            }
        )));
        assert!(output.messages.iter().any(|message| matches!(
            message,
            MidiMessage::ControlChange {
                controller: 123,
                ..
            }
        )));
    }
}

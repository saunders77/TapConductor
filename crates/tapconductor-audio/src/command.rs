// Copyright (c) 2026 Michael Saunders
use core::fmt;

/// Largest chord accepted on the real-time command path.
///
/// This is intentionally generous for orchestral tutti slices while keeping
/// every command fixed-size and allocation-free.
pub const MAX_NOTES_PER_CHORD: usize = 64;

/// An absolute frame on the output stream's monotonic sample clock.
pub type SampleTime = u64;

/// Identifies one performed slice instance, not a pitch or score event.
///
/// The same score slice can be played repeatedly and receives a fresh group ID
/// each time. This lets repeated pitches in overlapping taps release safely.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct VoiceGroupId(pub u64);

/// One equal-tempered note attack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct Note {
    /// MIDI note number, 0 through 127.
    pub pitch: u8,
    /// Forward-compatible 16-bit velocity. MIDI 1.0 velocity maps across the
    /// full range; zero is normalized to the quietest non-zero attack.
    pub velocity: u16,
}

impl Note {
    pub const fn new(pitch: u8, velocity: u16) -> Self {
        Self { pitch, velocity }
    }

    pub const fn from_midi1(pitch: u8, velocity: u8) -> Self {
        let velocity = if velocity == 0 { 1 } else { velocity };
        Self {
            pitch,
            velocity: (velocity as u32 * u16::MAX as u32 / 127) as u16,
        }
    }
}

/// A fixed-capacity chord carried inline in a single queue slot.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Chord {
    notes: [Note; MAX_NOTES_PER_CHORD],
    len: u8,
}

impl Default for Chord {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl Chord {
    pub const fn empty() -> Self {
        Self {
            notes: [Note {
                pitch: 0,
                velocity: 0,
            }; MAX_NOTES_PER_CHORD],
            len: 0,
        }
    }

    pub fn try_from_slice(notes: &[Note]) -> Result<Self, ChordError> {
        if notes.is_empty() {
            return Err(ChordError::Empty);
        }
        if notes.len() > MAX_NOTES_PER_CHORD {
            return Err(ChordError::TooManyNotes {
                supplied: notes.len(),
                maximum: MAX_NOTES_PER_CHORD,
            });
        }
        if let Some(note) = notes.iter().find(|note| note.pitch > 127) {
            return Err(ChordError::InvalidPitch(note.pitch));
        }

        let mut chord = Self::empty();
        chord.notes[..notes.len()].copy_from_slice(notes);
        chord.len = notes.len() as u8;
        Ok(chord)
    }

    pub fn push(&mut self, note: Note) -> Result<(), ChordError> {
        if note.pitch > 127 {
            return Err(ChordError::InvalidPitch(note.pitch));
        }
        let index = self.len as usize;
        if index == MAX_NOTES_PER_CHORD {
            return Err(ChordError::TooManyNotes {
                supplied: index + 1,
                maximum: MAX_NOTES_PER_CHORD,
            });
        }
        self.notes[index] = note;
        self.len += 1;
        Ok(())
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[Note] {
        &self.notes[..self.len()]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChordError {
    Empty,
    InvalidPitch(u8),
    TooManyNotes { supplied: usize, maximum: usize },
}

impl fmt::Display for ChordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a playable chord must contain at least one note"),
            Self::InvalidPitch(pitch) => write!(f, "MIDI pitch {pitch} is outside 0..=127"),
            Self::TooManyNotes { supplied, maximum } => {
                write!(f, "chord has {supplied} notes; maximum is {maximum}")
            }
        }
    }
}

impl std::error::Error for ChordError {}

/// A command consumed by the audio callback at an exact sample-frame boundary.
// The chord stays inline deliberately: boxing would allocate on the live
// producer path and defeat the fixed-capacity real-time design.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioCommand {
    /// Start all notes in `chord` before rendering frame `at`.
    PlaySlice {
        group: VoiceGroupId,
        at: SampleTime,
        chord: Chord,
    },
    /// Engage the piano key-up envelope without ending the logical group.
    DampenGroup { group: VoiceGroupId, at: SampleTime },
    /// Deliver note-off to every voice owned by `group` before frame `at`.
    ReleaseGroup { group: VoiceGroupId, at: SampleTime },
    /// Immediately silence all groups. This overrides normal gate minima.
    Panic { at: SampleTime },
    /// Change software gain at a sample boundary. Values are clamped to 0..=2.
    SetMasterGain { gain: f32, at: SampleTime },
}

impl AudioCommand {
    pub const fn at(&self) -> SampleTime {
        match *self {
            Self::PlaySlice { at, .. }
            | Self::DampenGroup { at, .. }
            | Self::ReleaseGroup { at, .. }
            | Self::Panic { at }
            | Self::SetMasterGain { at, .. } => at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_is_bounded_and_preserves_order() {
        let notes = [Note::new(60, 10), Note::new(64, 20), Note::new(67, 30)];
        let chord = Chord::try_from_slice(&notes).unwrap();
        assert_eq!(chord.as_slice(), notes);
        assert_eq!(Chord::try_from_slice(&[]), Err(ChordError::Empty));
    }

    #[test]
    fn midi_one_velocity_expands_to_sixteen_bits() {
        assert_eq!(Note::from_midi1(60, 127).velocity, u16::MAX);
        assert_ne!(Note::from_midi1(60, 0).velocity, 0);
    }
}

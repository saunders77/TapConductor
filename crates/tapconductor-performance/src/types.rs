use core::fmt;
use std::collections::HashSet;

/// Hard upper bound carried inline by a real-time `PlaySlice` command.
pub const MAX_CHORD_NOTES: usize = 64;

/// A transition can release the preceding group and play the new group.
pub const TRANSITION_AUDIO_CAPACITY: usize = MAX_CHORD_NOTES * 2;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(transparent)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

id_type!(EventId);
id_type!(InputId);
id_type!(VoiceGroupId);

/// Identifies the exact score snapshot accepted by the engine.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Generation(u64);

impl Generation {
    /// Reconstructs a generation carried by an external request. Only values
    /// previously emitted by an engine will pass that engine's validation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn first() -> Self {
        Self(1)
    }

    pub(crate) const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// An absolute frame on the current audio stream's monotonic sample clock.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SampleTime(u64);

impl SampleTime {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(frame: u64) -> Self {
        Self(frame)
    }

    #[must_use]
    pub const fn frame(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_add(self, frames: u64) -> Option<Self> {
        match self.0.checked_add(frames) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Non-zero audio sample rate in frames per second.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct SampleRate(u32);

impl SampleRate {
    #[must_use]
    pub const fn new(frames_per_second: u32) -> Option<Self> {
        if frames_per_second == 0 {
            None
        } else {
            Some(Self(frames_per_second))
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A validated MIDI 1.0 note number.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiPitch(u8);

impl MidiPitch {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(127);

    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// MIDI-2-ready normalized velocity. Zero is deliberately not a note-on.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Velocity(u16);

impl Velocity {
    /// Fixed MVP velocity, approximately 75% of the available range.
    pub const DEFAULT: Self = Self(0xc000);
    pub const MAX: Self = Self(u16::MAX);

    #[must_use]
    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChordError {
    Empty,
    TooManyNotes { maximum: usize },
    PitchOutOfRange { index: usize, value: u8 },
}

impl fmt::Display for ChordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a playable slice cannot be empty"),
            Self::TooManyNotes { maximum } => {
                write!(
                    formatter,
                    "a slice cannot contain more than {maximum} notes"
                )
            }
            Self::PitchOutOfRange { index, value } => write!(
                formatter,
                "MIDI pitch {value} at chord index {index} is outside 0..=127"
            ),
        }
    }
}

impl std::error::Error for ChordError {}

/// Inline chord storage: copying a play command never allocates.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Chord {
    pitches: [MidiPitch; MAX_CHORD_NOTES],
    len: u8,
}

impl Chord {
    pub fn from_pitches(pitches: &[MidiPitch]) -> Result<Self, ChordError> {
        if pitches.is_empty() {
            return Err(ChordError::Empty);
        }
        if pitches.len() > MAX_CHORD_NOTES {
            return Err(ChordError::TooManyNotes {
                maximum: MAX_CHORD_NOTES,
            });
        }

        let mut result = Self {
            pitches: [MidiPitch::MIN; MAX_CHORD_NOTES],
            len: pitches.len() as u8,
        };
        result.pitches[..pitches.len()].copy_from_slice(pitches);
        Ok(result)
    }

    pub fn from_midi_numbers(pitches: &[u8]) -> Result<Self, ChordError> {
        if pitches.is_empty() {
            return Err(ChordError::Empty);
        }
        if pitches.len() > MAX_CHORD_NOTES {
            return Err(ChordError::TooManyNotes {
                maximum: MAX_CHORD_NOTES,
            });
        }

        let mut result = Self {
            pitches: [MidiPitch::MIN; MAX_CHORD_NOTES],
            len: pitches.len() as u8,
        };
        for (index, value) in pitches.iter().copied().enumerate() {
            result.pitches[index] =
                MidiPitch::new(value).ok_or(ChordError::PitchOutOfRange { index, value })?;
        }
        Ok(result)
    }

    #[must_use]
    pub fn pitches(&self) -> &[MidiPitch] {
        &self.pitches[..usize::from(self.len)]
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Debug for Chord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Chord")
            .field(&self.pitches())
            .finish()
    }
}

/// The playback information the performance engine needs for one score onset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Slice {
    id: EventId,
    staff_groups: [Option<StaffSlice>; MAX_CHORD_NOTES],
    len: u8,
    staff_scoped: bool,
}

impl Slice {
    #[must_use]
    pub const fn new(id: EventId, chord: Chord) -> Self {
        Self {
            id,
            staff_groups: [Some(StaffSlice::new(0, chord, SliceReleaseBoundary::NextTrigger)); MAX_CHORD_NOTES],
            len: 1,
            staff_scoped: false,
        }
    }

    /// Creates a score-backed slice whose voices remain gated until playback
    /// reaches the first onset at or after their resolved written/tied end.
    #[must_use]
    pub const fn with_release_boundary(
        id: EventId,
        chord: Chord,
        release_on: Option<EventId>,
    ) -> Self {
        Self {
            id,
            staff_groups: [Some(StaffSlice::new(0, chord, SliceReleaseBoundary::from_event(release_on))); MAX_CHORD_NOTES],
            len: 1,
            staff_scoped: false,
        }
    }

    pub fn from_staff_groups(id: EventId, groups: &[StaffSlice]) -> Result<Self, ChordError> {
        if groups.is_empty() {
            return Err(ChordError::Empty);
        }
        if groups.len() > MAX_CHORD_NOTES {
            return Err(ChordError::TooManyNotes { maximum: MAX_CHORD_NOTES });
        }
        let mut staff_groups = [None; MAX_CHORD_NOTES];
        for (index, group) in groups.iter().copied().enumerate() {
            staff_groups[index] = Some(group);
        }
        Ok(Self { id, staff_groups, len: groups.len() as u8, staff_scoped: true })
    }

    #[must_use]
    pub const fn id(self) -> EventId {
        self.id
    }

    #[must_use]
    pub fn chord(self) -> Chord {
        let mut pitches = [MidiPitch::MIN; MAX_CHORD_NOTES];
        let mut len = 0;
        for group in self.staff_groups() {
            for pitch in group.chord().pitches() {
                pitches[len] = *pitch;
                len += 1;
            }
        }
        Chord::from_pitches(&pitches[..len]).expect("a slice always contains a bounded non-empty chord")
    }

    #[must_use]
    pub fn staff_groups(self) -> impl ExactSizeIterator<Item = StaffSlice> {
        self.staff_groups
            .into_iter()
            .take(usize::from(self.len))
            .map(|group| group.expect("the slice group prefix is populated"))
    }

    #[must_use]
    pub const fn is_staff_scoped(self) -> bool {
        self.staff_scoped
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaffSlice {
    staff: u16,
    chord: Chord,
    release_boundary: SliceReleaseBoundary,
}

impl StaffSlice {
    #[must_use]
    pub const fn new(staff: u16, chord: Chord, release_boundary: SliceReleaseBoundary) -> Self {
        Self { staff, chord, release_boundary }
    }

    #[must_use]
    pub const fn staff(self) -> u16 { self.staff }

    #[must_use]
    pub const fn chord(self) -> Chord { self.chord }

    #[must_use]
    pub const fn release_boundary(self) -> SliceReleaseBoundary { self.release_boundary }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SliceReleaseBoundary {
    NextTrigger,
    OnEvent(EventId),
    EndOfScore,
}

impl SliceReleaseBoundary {
    #[must_use]
    pub const fn from_event(event: Option<EventId>) -> Self {
        match event {
            Some(event) => Self::OnEvent(event),
            None => Self::EndOfScore,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreSequenceError {
    DuplicateEventId(EventId),
}

impl fmt::Display for ScoreSequenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateEventId(id) => {
                write!(formatter, "event ID {} occurs more than once", id.get())
            }
        }
    }
}

impl std::error::Error for ScoreSequenceError {}

/// Slices in already-expanded playback order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreSequence {
    slices: Vec<Slice>,
}

impl ScoreSequence {
    pub fn new(slices: Vec<Slice>) -> Result<Self, ScoreSequenceError> {
        let mut ids = HashSet::with_capacity(slices.len());
        for slice in &slices {
            if !ids.insert(slice.id) {
                return Err(ScoreSequenceError::DuplicateEventId(slice.id));
            }
        }
        Ok(Self { slices })
    }

    #[must_use]
    pub fn slices(&self) -> &[Slice] {
        &self.slices
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slices.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    pub(crate) fn find(&self, id: EventId) -> Option<(usize, Slice)> {
        self.slices
            .iter()
            .copied()
            .enumerate()
            .find(|(_, slice)| slice.id == id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerKind {
    Tap,
    Audition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyReason {
    Panic,
    Reposition,
    ScoreLoad,
    ScoreUnload,
    ActivePartsChanged,
    AudioDeviceLost,
    Shutdown,
}

/// Commands for the bounded audio queue. `roll_interval_frames` spaces chord
/// attacks from lowest to highest pitch; zero retains simultaneous playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioCommand {
    PlaySlice {
        at: SampleTime,
        group: VoiceGroupId,
        chord: Chord,
        velocity: Velocity,
        roll_interval_frames: u32,
    },
    ReleaseGroup {
        at: SampleTime,
        group: VoiceGroupId,
    },
    Panic {
        at: SampleTime,
        reason: SafetyReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreReason {
    InputAlreadyHeld,
    InputWasNotHeld,
    EndOfScore,
}

/// Authoritative state notification for asynchronous UI reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerformanceEvent {
    ScoreReady {
        generation: Generation,
        next: Option<EventId>,
    },
    ScoreUnloaded {
        previous_generation: Generation,
    },
    Triggered {
        generation: Generation,
        event: EventId,
        next: Option<EventId>,
        group: VoiceGroupId,
        kind: TriggerKind,
        at: SampleTime,
    },
    CursorMoved {
        generation: Generation,
        next: EventId,
    },
    InputReleased {
        input: InputId,
        scheduled_release: Option<SampleTime>,
    },
    Ignored {
        reason: IgnoreReason,
    },
    SafetyStop {
        reason: SafetyReason,
    },
}

/// Allocation-free result of one state transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transition {
    audio: [Option<AudioCommand>; TRANSITION_AUDIO_CAPACITY],
    audio_len: u8,
    event: Option<PerformanceEvent>,
}

impl Transition {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            audio: [None; TRANSITION_AUDIO_CAPACITY],
            audio_len: 0,
            event: None,
        }
    }

    #[must_use]
    pub const fn event(&self) -> Option<&PerformanceEvent> {
        self.event.as_ref()
    }

    #[must_use]
    pub const fn audio_len(&self) -> usize {
        self.audio_len as usize
    }

    pub fn audio_commands(&self) -> impl ExactSizeIterator<Item = &AudioCommand> {
        self.audio[..self.audio_len()].iter().map(|command| {
            command
                .as_ref()
                .expect("the populated transition prefix contains commands")
        })
    }

    pub(crate) fn with_event(event: PerformanceEvent) -> Self {
        Self {
            event: Some(event),
            ..Self::none()
        }
    }

    pub(crate) fn set_event(&mut self, event: PerformanceEvent) {
        self.event = Some(event);
    }

    pub(crate) fn push_audio(&mut self, command: AudioCommand) {
        let index = self.audio_len();
        assert!(
            index < TRANSITION_AUDIO_CAPACITY,
            "internal transition audio capacity invariant"
        );
        self.audio[index] = Some(command);
        self.audio_len += 1;
    }
}

impl Default for Transition {
    fn default() -> Self {
        Self::none()
    }
}

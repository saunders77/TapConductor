use core::fmt;

use crate::{
    AudioCommand, Chord, DefaultPianoGate, EventId, GateError, GatePolicy, Generation,
    IgnoreReason, InputId, MidiPitch, PerformanceEvent, SafetyReason, SampleRate, SampleTime,
    ScoreSequence, Slice, SliceReleaseBoundary, StaffSlice, Transition, TriggerKind, Velocity,
    VoiceGroupId, MAX_CHORD_NOTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    pub max_active_groups: usize,
    pub max_held_inputs: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_active_groups: 256,
            max_held_inputs: 64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineError {
    InvalidConfiguration,
    NoScoreLoaded,
    StaleGeneration {
        expected: Generation,
        received: Generation,
    },
    EventNotFound(EventId),
    NonMonotonicSampleTime {
        previous: SampleTime,
        received: SampleTime,
    },
    ActiveGroupCapacityExceeded {
        maximum: usize,
    },
    HeldInputCapacityExceeded {
        maximum: usize,
    },
    SampleRateChangeWhileActive,
    GenerationExhausted,
    VoiceGroupIdExhausted,
    Gate(GateError),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration => {
                formatter.write_str("performance engine capacities must be non-zero")
            }
            Self::NoScoreLoaded => formatter.write_str("no score is loaded"),
            Self::StaleGeneration { expected, received } => write!(
                formatter,
                "stale score generation {}; current generation is {}",
                received.get(),
                expected.get()
            ),
            Self::EventNotFound(id) => write!(formatter, "score event {} was not found", id.get()),
            Self::NonMonotonicSampleTime { previous, received } => write!(
                formatter,
                "sample time moved backwards from {} to {}",
                previous.frame(),
                received.frame()
            ),
            Self::ActiveGroupCapacityExceeded { maximum } => write!(
                formatter,
                "the configured limit of {maximum} active voice groups was reached"
            ),
            Self::HeldInputCapacityExceeded { maximum } => write!(
                formatter,
                "the configured limit of {maximum} held physical inputs was reached"
            ),
            Self::SampleRateChangeWhileActive => formatter.write_str(
                "the audio sample rate cannot change while performance voices are active",
            ),
            Self::GenerationExhausted => formatter.write_str("score generation counter exhausted"),
            Self::VoiceGroupIdExhausted => formatter.write_str("voice group ID counter exhausted"),
            Self::Gate(error) => write!(formatter, "gate policy failed: {error}"),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Gate(error) => Some(error),
            _ => None,
        }
    }
}

impl From<GateError> for EngineError {
    fn from(value: GateError) -> Self {
        Self::Gate(value)
    }
}

/// One compact command entering the authoritative state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PerformanceCommand {
    Tap {
        generation: Generation,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    },
    Audition {
        generation: Generation,
        event: EventId,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    },
    AuditionNote {
        generation: Generation,
        event: EventId,
        pitch: MidiPitch,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    },
    AuditionChord {
        generation: Generation,
        event: EventId,
        chord: Chord,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    },
    InputReleased {
        input: InputId,
        at: SampleTime,
    },
    Reposition {
        generation: Generation,
        event: EventId,
        at: SampleTime,
    },
    Panic {
        at: SampleTime,
    },
    SafetyStop {
        at: SampleTime,
        reason: SafetyReason,
    },
    AdvanceClock {
        to: SampleTime,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VoiceGroup {
    id: VoiceGroupId,
    generation: Generation,
    event: EventId,
    input: InputId,
    attack_at: SampleTime,
    input_released_at: Option<SampleTime>,
    first_later_trigger_at: Option<SampleTime>,
    release_scheduled_at: Option<SampleTime>,
    release_boundary: SliceReleaseBoundary,
    staff: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HeldInput {
    id: InputId,
    group: Option<VoiceGroupId>,
}

/// Read-only diagnostic view of an independently gated performance gesture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveGroup {
    pub id: VoiceGroupId,
    pub generation: Generation,
    pub event: EventId,
    pub input: InputId,
    pub attack_at: SampleTime,
    pub input_released_at: Option<SampleTime>,
    pub first_later_trigger_at: Option<SampleTime>,
    pub release_scheduled_at: Option<SampleTime>,
}

impl From<VoiceGroup> for ActiveGroup {
    fn from(value: VoiceGroup) -> Self {
        Self {
            id: value.id,
            generation: value.generation,
            event: value.event,
            input: value.input,
            attack_at: value.attack_at,
            input_released_at: value.input_released_at,
            first_later_trigger_at: value.first_later_trigger_at,
            release_scheduled_at: value.release_scheduled_at,
        }
    }
}

/// Cursor and gate state. The generic policy keeps future modes off the attack path.
pub struct PerformanceEngine<G = DefaultPianoGate> {
    sample_rate: SampleRate,
    gate: G,
    config: EngineConfig,
    score: Option<ScoreSequence>,
    generation: Option<Generation>,
    last_generation: Option<Generation>,
    cursor_index: usize,
    last_observed_time: Option<SampleTime>,
    next_group_id: u64,
    active_groups: Vec<VoiceGroup>,
    held_inputs: Vec<HeldInput>,
    regular_roll_ms: u16,
    audition_roll_ms: u16,
    legato_mode: bool,
}

impl PerformanceEngine<DefaultPianoGate> {
    pub fn with_default_gate(
        sample_rate: SampleRate,
        config: EngineConfig,
    ) -> Result<Self, EngineError> {
        Self::new(sample_rate, DefaultPianoGate, config)
    }
}

impl<G: GatePolicy> PerformanceEngine<G> {
    pub fn new(
        sample_rate: SampleRate,
        gate: G,
        config: EngineConfig,
    ) -> Result<Self, EngineError> {
        if config.max_active_groups == 0 || config.max_held_inputs == 0 {
            return Err(EngineError::InvalidConfiguration);
        }

        Ok(Self {
            sample_rate,
            gate,
            config,
            score: None,
            generation: None,
            last_generation: None,
            cursor_index: 0,
            last_observed_time: None,
            next_group_id: 1,
            active_groups: Vec::with_capacity(config.max_active_groups),
            held_inputs: Vec::with_capacity(config.max_held_inputs),
            regular_roll_ms: 0,
            audition_roll_ms: 120,
            legato_mode: false,
        })
    }

    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    pub fn set_roll_delays(&mut self, regular_ms: u16, audition_ms: u16) {
        self.regular_roll_ms = regular_ms;
        self.audition_roll_ms = audition_ms;
    }

    /// Enables score-aware legato release gestures. It is off by default.
    pub fn set_legato_mode(&mut self, enabled: bool) {
        self.legato_mode = enabled;
    }

    #[must_use]
    pub const fn legato_mode(&self) -> bool {
        self.legato_mode
    }

    /// Updates the device clock rate after a safety stop. The score and cursor
    /// remain intact, while all future gate deadlines use the new rate.
    pub fn set_sample_rate(&mut self, sample_rate: SampleRate) -> Result<(), EngineError> {
        if !self.active_groups.is_empty() {
            return Err(EngineError::SampleRateChangeWhileActive);
        }
        self.sample_rate = sample_rate;
        Ok(())
    }

    #[must_use]
    pub const fn generation(&self) -> Option<Generation> {
        self.generation
    }

    #[must_use]
    pub const fn cursor_index(&self) -> usize {
        self.cursor_index
    }

    #[must_use]
    pub fn next_event(&self) -> Option<EventId> {
        self.score
            .as_ref()
            .and_then(|score| score.slices().get(self.cursor_index))
            .copied()
            .map(Slice::id)
    }

    #[must_use]
    pub fn active_group_count(&self) -> usize {
        self.active_groups.len()
    }

    pub fn active_groups(&self) -> impl ExactSizeIterator<Item = ActiveGroup> + '_ {
        self.active_groups.iter().copied().map(ActiveGroup::from)
    }

    /// Installs a new expanded playback sequence and invalidates every old UI request.
    pub fn load_score(
        &mut self,
        score: ScoreSequence,
        at: SampleTime,
    ) -> Result<Transition, EngineError> {
        let generation = match self.last_generation {
            Some(previous) => previous
                .checked_next()
                .ok_or(EngineError::GenerationExhausted)?,
            None => Generation::first(),
        };
        // Loading is a safety boundary. Clamp a delayed request to the latest
        // observed sample rather than risk leaving an old voice sounding.
        let at = self.observe_safety_time(at);

        self.stop_all_state();
        self.score = Some(score);
        self.generation = Some(generation);
        self.last_generation = Some(generation);
        self.cursor_index = 0;

        let mut transition = Transition::with_event(PerformanceEvent::ScoreReady {
            generation,
            next: self.next_event(),
        });
        transition.push_audio(AudioCommand::Panic {
            at,
            reason: SafetyReason::ScoreLoad,
        });
        Ok(transition)
    }

    pub fn unload_score(&mut self, at: SampleTime) -> Result<Transition, EngineError> {
        let previous_generation = self.generation.ok_or(EngineError::NoScoreLoaded)?;
        let at = self.observe_safety_time(at);
        self.stop_all_state();
        self.score = None;
        self.generation = None;
        self.cursor_index = 0;

        let mut transition = Transition::with_event(PerformanceEvent::ScoreUnloaded {
            previous_generation,
        });
        transition.push_audio(AudioCommand::Panic {
            at,
            reason: SafetyReason::ScoreUnload,
        });
        Ok(transition)
    }

    pub fn handle(&mut self, command: PerformanceCommand) -> Result<Transition, EngineError> {
        match command {
            PerformanceCommand::Tap {
                generation,
                input,
                at,
                velocity,
            } => self.tap(generation, input, at, velocity),
            PerformanceCommand::Audition {
                generation,
                event,
                input,
                at,
                velocity,
            } => self.audition(generation, event, input, at, velocity),
            PerformanceCommand::AuditionNote {
                generation,
                event,
                pitch,
                input,
                at,
                velocity,
            } => self.audition_note(generation, event, pitch, input, at, velocity),
            PerformanceCommand::AuditionChord {
                generation,
                event,
                chord,
                input,
                at,
                velocity,
            } => self.audition_chord(generation, event, chord, input, at, velocity),
            PerformanceCommand::InputReleased { input, at } => self.release_input(input, at),
            PerformanceCommand::Reposition {
                generation,
                event,
                at,
            } => self.reposition(generation, event, at),
            PerformanceCommand::Panic { at } => self.safety_stop(at, SafetyReason::Panic),
            PerformanceCommand::SafetyStop { at, reason } => self.safety_stop(at, reason),
            PerformanceCommand::AdvanceClock { to } => {
                self.observe_time(to)?;
                Ok(Transition::none())
            }
        }
    }

    fn tap(
        &mut self,
        generation: Generation,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    ) -> Result<Transition, EngineError> {
        self.require_generation(generation)?;
        self.observe_time(at)?;
        if self.input_is_held(input) {
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            }));
        }

        let slice = self
            .score
            .as_ref()
            .expect("generation validation proves a score is loaded")
            .slices()
            .get(self.cursor_index)
            .copied();

        let Some(slice) = slice else {
            self.push_held_input(HeldInput {
                id: input,
                group: None,
            })?;
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::EndOfScore,
            }));
        };

        let transition =
            self.trigger_slice(generation, slice, input, at, velocity, TriggerKind::Tap)?;
        self.cursor_index += 1;
        Ok(transition)
    }

    fn audition(
        &mut self,
        generation: Generation,
        event: EventId,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    ) -> Result<Transition, EngineError> {
        self.require_generation(generation)?;
        let slice = self
            .score
            .as_ref()
            .expect("generation validation proves a score is loaded")
            .find(event)
            .map(|(_, slice)| slice)
            .ok_or(EngineError::EventNotFound(event))?;
        self.observe_time(at)?;
        if self.input_is_held(input) {
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            }));
        }
        self.trigger_slice(
            generation,
            slice,
            input,
            at,
            velocity,
            TriggerKind::Audition,
        )
    }

    fn audition_note(
        &mut self,
        generation: Generation,
        event: EventId,
        pitch: MidiPitch,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    ) -> Result<Transition, EngineError> {
        self.require_generation(generation)?;
        // Validate the source event even though this audition uses a
        // one-pitch chord. This keeps stale/out-of-range UI requests subject
        // to the same generation and event checks as whole-slice auditions.
        self.score
            .as_ref()
            .expect("generation validation proves a score is loaded")
            .find(event)
            .ok_or(EngineError::EventNotFound(event))?;
        self.observe_time(at)?;
        if self.input_is_held(input) {
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            }));
        }
        let chord = Chord::from_pitches(&[pitch]).expect("one valid MIDI pitch forms a chord");
        self.trigger_slice(
            generation,
            Slice::new(event, chord),
            input,
            at,
            velocity,
            TriggerKind::Audition,
        )
    }

    fn audition_chord(
        &mut self,
        generation: Generation,
        event: EventId,
        chord: Chord,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
    ) -> Result<Transition, EngineError> {
        self.require_generation(generation)?;
        self.score
            .as_ref()
            .expect("generation validation proves a score is loaded")
            .find(event)
            .ok_or(EngineError::EventNotFound(event))?;
        self.observe_time(at)?;
        if self.input_is_held(input) {
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            }));
        }
        self.trigger_slice(
            generation,
            Slice::new(event, chord),
            input,
            at,
            velocity,
            TriggerKind::Audition,
        )
    }

    fn trigger_slice(
        &mut self,
        generation: Generation,
        slice: Slice,
        input: InputId,
        at: SampleTime,
        velocity: Velocity,
        kind: TriggerKind,
    ) -> Result<Transition, EngineError> {
        let staff_scoped_tap = kind == TriggerKind::Tap && slice.is_staff_scoped();
        let mut staff_groups: [Option<StaffSlice>; MAX_CHORD_NOTES] = [None; MAX_CHORD_NOTES];
        let group_count = if staff_scoped_tap {
            for (index, group) in slice.staff_groups().enumerate() {
                staff_groups[index] = Some(group);
            }
            slice.staff_groups().len()
        } else {
            staff_groups[0] = Some(StaffSlice::new(
                0,
                slice.chord(),
                if kind == TriggerKind::Tap {
                    slice
                        .staff_groups()
                        .next()
                        .expect("a slice has one legacy group")
                        .release_boundary()
                } else {
                    SliceReleaseBoundary::NextTrigger
                },
            ));
            1
        };
        if self.active_groups.len().saturating_add(group_count) > self.config.max_active_groups {
            return Err(EngineError::ActiveGroupCapacityExceeded {
                maximum: self.config.max_active_groups,
            });
        }
        if self.held_inputs.len() == self.config.max_held_inputs {
            return Err(EngineError::HeldInputCapacityExceeded {
                maximum: self.config.max_held_inputs,
            });
        }
        let next_group_id = self
            .next_group_id
            .checked_add(group_count as u64)
            .ok_or(EngineError::VoiceGroupIdExhausted)?;

        let mut transition = Transition::none();
        if staff_scoped_tap && self.legato_mode {
            // A qualifying future note attack releases every matching held
            // score note, regardless of which physical key originally struck
            // it or whether that key is still down.
            for waiting in self.active_groups.iter().filter(|waiting| {
                waiting.release_scheduled_at.is_none()
                    && matches!(
                        waiting.release_boundary,
                        SliceReleaseBoundary::OnEvent(event) if slice.id().get() >= event.get()
                    )
            }) {
                self.gate.note_off_at(
                    self.sample_rate,
                    Some(waiting.input_released_at.unwrap_or(at)),
                    Some(at),
                )?;
            }
            for waiting in &mut self.active_groups {
                let boundary_reached = matches!(
                    waiting.release_boundary,
                    SliceReleaseBoundary::OnEvent(event) if slice.id().get() >= event.get()
                );
                if waiting.release_scheduled_at.is_none() && boundary_reached {
                    let release_at = self
                        .gate
                        .note_off_at(
                            self.sample_rate,
                            Some(waiting.input_released_at.unwrap_or(at)),
                            Some(at),
                        )?
                        .expect("a release and trigger always produce a gate deadline");
                    waiting.first_later_trigger_at = Some(at);
                    waiting.release_scheduled_at = Some(release_at);
                    transition.push_audio(AudioCommand::DampenGroup {
                        at,
                        group: waiting.id,
                    });
                    transition.push_audio(AudioCommand::ReleaseGroup {
                        at: release_at,
                        group: waiting.id,
                    });
                }
            }
        } else if !staff_scoped_tap {
            if let Some(index) = self.active_groups.iter().position(|group| {
                group.first_later_trigger_at.is_none()
                    && (kind == TriggerKind::Audition
                        || match group.release_boundary {
                            SliceReleaseBoundary::InputRelease => true,
                            SliceReleaseBoundary::NextTrigger => true,
                            SliceReleaseBoundary::OnEvent(event) => slice.id().get() >= event.get(),
                            SliceReleaseBoundary::EndOfScore => false,
                        })
            }) {
                let waiting_release = self.gate.note_off_at(
                    self.sample_rate,
                    self.active_groups[index].input_released_at,
                    Some(at),
                )?;
                let waiting = &mut self.active_groups[index];
                waiting.first_later_trigger_at = Some(at);
                if let Some(release_at) = waiting_release {
                    waiting.release_scheduled_at = Some(release_at);
                    transition.push_audio(AudioCommand::ReleaseGroup {
                        at: release_at,
                        group: waiting.id,
                    });
                }
            }
        }

        self.next_group_id = next_group_id;
        let first_group_id = VoiceGroupId::new(self.next_group_id - group_count as u64);
        for (index, staff_group) in staff_groups.iter().take(group_count).enumerate() {
            let staff_group = staff_group
                .as_ref()
                .expect("the staff group prefix is populated");
            let group_id = VoiceGroupId::new(first_group_id.get() + index as u64);
            self.active_groups.push(VoiceGroup {
                id: group_id,
                generation,
                event: slice.id(),
                input,
                attack_at: at,
                input_released_at: None,
                first_later_trigger_at: (staff_scoped_tap
                    && matches!(
                        staff_group.release_boundary(),
                        SliceReleaseBoundary::InputRelease
                    ))
                .then_some(at),
                release_scheduled_at: None,
                release_boundary: staff_group.release_boundary(),
                staff: staff_group.staff(),
            });
            transition.push_audio(AudioCommand::PlaySlice {
                at,
                group: group_id,
                chord: staff_group.chord(),
                velocity,
                roll_interval_frames: match kind {
                    TriggerKind::Tap => {
                        self.sample_rate
                            .get()
                            .saturating_mul(u32::from(self.regular_roll_ms))
                            / 1_000
                    }
                    TriggerKind::Audition => {
                        self.sample_rate
                            .get()
                            .saturating_mul(u32::from(self.audition_roll_ms))
                            / 1_000
                    }
                },
            });
        }
        self.held_inputs.push(HeldInput {
            id: input,
            group: if staff_scoped_tap {
                None
            } else {
                Some(first_group_id)
            },
        });
        transition.set_event(PerformanceEvent::Triggered {
            generation,
            event: slice.id(),
            next: if kind == TriggerKind::Tap {
                self.score
                    .as_ref()
                    .and_then(|score| score.slices().get(self.cursor_index + 1))
                    .copied()
                    .map(Slice::id)
            } else {
                self.next_event()
            },
            group: first_group_id,
            kind,
            at,
        });
        Ok(transition)
    }

    fn release_input(&mut self, input: InputId, at: SampleTime) -> Result<Transition, EngineError> {
        self.observe_time(at)?;
        let Some(binding_index) = self.held_inputs.iter().position(|held| held.id == input) else {
            return Ok(Transition::with_event(PerformanceEvent::Ignored {
                reason: IgnoreReason::InputWasNotHeld,
            }));
        };
        let releases_group = |group: &VoiceGroup| {
            group.release_scheduled_at.is_none()
                && if self.legato_mode {
                    matches!(group.release_boundary, SliceReleaseBoundary::InputRelease)
                        || (group.input == input
                            && matches!(group.release_boundary, SliceReleaseBoundary::NextTrigger))
                } else {
                    group.input == input
                }
        };
        // Validate every gate calculation before changing the input latch or
        // any group. A timestamp overflow must leave the gesture retryable.
        for _group in self
            .active_groups
            .iter()
            .filter(|group| releases_group(group))
        {
            self.gate
                .note_off_at(self.sample_rate, Some(at), Some(at))?;
        }

        self.held_inputs.remove(binding_index);
        let mut transition = Transition::none();
        let mut scheduled_release = None;
        for group in &mut self.active_groups {
            if group.input == input && group.input_released_at.is_none() {
                group.input_released_at = Some(at);
            }
            if group.release_scheduled_at.is_some()
                || if self.legato_mode {
                    !matches!(group.release_boundary, SliceReleaseBoundary::InputRelease)
                        && !(group.input == input
                            && matches!(group.release_boundary, SliceReleaseBoundary::NextTrigger))
                } else {
                    group.input != input
                }
            {
                continue;
            }
            let release_at = self
                .gate
                .note_off_at(self.sample_rate, Some(at), Some(at))?
                .expect("a release and trigger always produce a gate deadline");
            group.release_scheduled_at = Some(release_at);
            scheduled_release = scheduled_release.or(Some(release_at));
            transition.push_audio(AudioCommand::DampenGroup {
                at,
                group: group.id,
            });
            transition.push_audio(AudioCommand::ReleaseGroup {
                at: release_at,
                group: group.id,
            });
        }
        transition.set_event(PerformanceEvent::InputReleased {
            input,
            scheduled_release,
        });
        Ok(transition)
    }

    fn reposition(
        &mut self,
        generation: Generation,
        event: EventId,
        at: SampleTime,
    ) -> Result<Transition, EngineError> {
        self.require_generation(generation)?;
        let index = self
            .score
            .as_ref()
            .expect("generation validation proves a score is loaded")
            .find(event)
            .map(|(index, _)| index)
            .ok_or(EngineError::EventNotFound(event))?;
        let at = self.observe_safety_time(at);
        self.stop_all_state();
        self.cursor_index = index;

        let mut transition = Transition::with_event(PerformanceEvent::CursorMoved {
            generation,
            next: event,
        });
        transition.push_audio(AudioCommand::Panic {
            at,
            reason: SafetyReason::Reposition,
        });
        Ok(transition)
    }

    fn safety_stop(
        &mut self,
        at: SampleTime,
        reason: SafetyReason,
    ) -> Result<Transition, EngineError> {
        // Panic-class actions are intentionally generation-free and cannot be
        // defeated by a delayed timestamp. "Immediate" means no earlier than
        // the newest sample the state machine has already observed.
        let at = self.observe_safety_time(at);
        self.stop_all_state();
        let mut transition = Transition::with_event(PerformanceEvent::SafetyStop { reason });
        transition.push_audio(AudioCommand::Panic { at, reason });
        Ok(transition)
    }

    fn require_generation(&self, received: Generation) -> Result<(), EngineError> {
        let expected = self.generation.ok_or(EngineError::NoScoreLoaded)?;
        if received != expected {
            return Err(EngineError::StaleGeneration { expected, received });
        }
        Ok(())
    }

    fn observe_time(&mut self, at: SampleTime) -> Result<(), EngineError> {
        if let Some(previous) = self.last_observed_time {
            if at < previous {
                return Err(EngineError::NonMonotonicSampleTime {
                    previous,
                    received: at,
                });
            }
        }
        self.last_observed_time = Some(at);
        self.prune_released_groups(at);
        Ok(())
    }

    fn observe_safety_time(&mut self, requested: SampleTime) -> SampleTime {
        let at = self
            .last_observed_time
            .map_or(requested, |previous| core::cmp::max(previous, requested));
        self.last_observed_time = Some(at);
        self.prune_released_groups(at);
        at
    }

    fn prune_released_groups(&mut self, at: SampleTime) {
        self.active_groups.retain(|group| {
            group
                .release_scheduled_at
                .map_or(true, |release| release > at)
        });
    }

    fn input_is_held(&self, input: InputId) -> bool {
        self.held_inputs.iter().any(|held| held.id == input)
    }

    fn push_held_input(&mut self, input: HeldInput) -> Result<(), EngineError> {
        if self.held_inputs.len() == self.config.max_held_inputs {
            return Err(EngineError::HeldInputCapacityExceeded {
                maximum: self.config.max_held_inputs,
            });
        }
        self.held_inputs.push(input);
        Ok(())
    }

    fn stop_all_state(&mut self) {
        self.active_groups.clear();
        // Retain the physical-down latch across a safety stop. Auto-repeat or
        // duplicate pointer-down events remain suppressed until the matching up.
        for input in &mut self.held_inputs {
            input.group = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GatePolicy;

    const RATE: SampleRate = match SampleRate::new(48_000) {
        Some(rate) => rate,
        None => panic!("test sample rate is non-zero"),
    };
    const MINIMUM: u64 = 4_800;

    fn time(frame: u64) -> SampleTime {
        SampleTime::new(frame)
    }

    fn input(id: u64) -> InputId {
        InputId::new(id)
    }

    fn event(id: u64) -> EventId {
        EventId::new(id)
    }

    fn chord(pitches: &[u8]) -> Chord {
        Chord::from_midi_numbers(pitches).unwrap()
    }

    fn score() -> ScoreSequence {
        ScoreSequence::new(vec![
            Slice::new(event(10), chord(&[60, 64, 67])),
            Slice::new(event(20), chord(&[60])),
            Slice::new(event(30), chord(&[62, 65])),
        ])
        .unwrap()
    }

    fn engine() -> PerformanceEngine {
        PerformanceEngine::with_default_gate(RATE, EngineConfig::default()).unwrap()
    }

    fn load(engine: &mut PerformanceEngine, at: u64) -> Generation {
        engine.load_score(score(), time(at)).unwrap();
        engine.generation().unwrap()
    }

    fn tap(
        engine: &mut PerformanceEngine,
        generation: Generation,
        input_id: u64,
        at: u64,
    ) -> Transition {
        engine
            .handle(PerformanceCommand::Tap {
                generation,
                input: input(input_id),
                at: time(at),
                velocity: Velocity::DEFAULT,
            })
            .unwrap()
    }

    fn release(engine: &mut PerformanceEngine, input_id: u64, at: u64) -> Transition {
        engine
            .handle(PerformanceCommand::InputReleased {
                input: input(input_id),
                at: time(at),
            })
            .unwrap()
    }

    fn commands(transition: &Transition) -> Vec<AudioCommand> {
        transition.audio_commands().copied().collect()
    }

    #[test]
    fn gate_uses_exact_ceil_frames_for_many_rates() {
        for rate in [1, 2, 3, 44_100, 48_000, 96_000, 192_001] {
            let rate = SampleRate::new(rate).unwrap();
            let frames = DefaultPianoGate::minimum_frames(rate);
            assert!(frames * 1_000 >= u64::from(rate.get()) * 100);
            if frames > 0 {
                assert!((frames - 1) * 1_000 < u64::from(rate.get()) * 100);
            }
        }
        assert_eq!(DefaultPianoGate::minimum_frames(RATE), MINIMUM);
    }

    #[test]
    fn sample_rate_can_change_only_after_a_safety_stop() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 1, 10);
        let rate_44_1 = SampleRate::new(44_100).unwrap();

        assert_eq!(
            engine.set_sample_rate(rate_44_1),
            Err(EngineError::SampleRateChangeWhileActive)
        );

        engine
            .handle(PerformanceCommand::Panic { at: time(11) })
            .unwrap();
        engine.set_sample_rate(rate_44_1).unwrap();
        assert_eq!(engine.sample_rate(), rate_44_1);
        assert_eq!(DefaultPianoGate::minimum_frames(rate_44_1), 4_410);
    }

    #[test]
    fn gate_requires_both_release_and_a_later_trigger() {
        let gate = DefaultPianoGate;
        assert_eq!(gate.note_off_at(RATE, None, None).unwrap(), None);
        assert_eq!(gate.note_off_at(RATE, Some(time(10)), None).unwrap(), None);
        assert_eq!(gate.note_off_at(RATE, None, Some(time(20))).unwrap(), None);
        assert_eq!(
            gate.note_off_at(RATE, Some(time(10)), Some(time(20)))
                .unwrap(),
            Some(time(10 + MINIMUM))
        );
        assert_eq!(
            gate.note_off_at(RATE, Some(time(10)), Some(time(30_000)))
                .unwrap(),
            Some(time(30_000))
        );
    }

    #[test]
    fn gate_reports_sample_clock_overflow() {
        let result = DefaultPianoGate.note_off_at(
            RATE,
            Some(time(u64::MAX - MINIMUM + 1)),
            Some(time(u64::MAX)),
        );
        assert_eq!(result, Err(GateError::SampleTimeOverflow));
    }

    #[test]
    fn gate_failure_does_not_break_the_physical_input_pair() {
        let mut engine = engine();
        let start = u64::MAX - MINIMUM - 1_000;
        let generation = load(&mut engine, start);
        tap(&mut engine, generation, 1, start + 1);
        tap(&mut engine, generation, 2, start + 2);
        let overflowing_release = u64::MAX - MINIMUM + 1;
        let error = engine
            .handle(PerformanceCommand::InputReleased {
                input: input(1),
                at: time(overflowing_release),
            })
            .unwrap_err();
        assert_eq!(error, EngineError::Gate(GateError::SampleTimeOverflow));

        let repeated = tap(&mut engine, generation, 1, overflowing_release);
        assert_eq!(
            repeated.event(),
            Some(&PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            })
        );
    }

    #[test]
    fn tap_attacks_whole_chord_now_and_advances_once() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let result = tap(&mut engine, generation, 1, 100);
        let command = commands(&result);
        assert_eq!(command.len(), 1);
        let AudioCommand::PlaySlice {
            at,
            group,
            chord: played,
            velocity,
            ..
        } = command[0]
        else {
            panic!("expected a play command");
        };
        assert_eq!(at, time(100));
        assert_eq!(played.pitches(), chord(&[60, 64, 67]).pitches());
        assert_eq!(velocity, Velocity::DEFAULT);
        assert_eq!(engine.cursor_index(), 1);
        assert_eq!(engine.next_event(), Some(event(20)));
        assert_eq!(
            result.event(),
            Some(&PerformanceEvent::Triggered {
                generation,
                event: event(10),
                next: Some(event(20)),
                group,
                kind: TriggerKind::Tap,
                at: time(100),
            })
        );
    }

    #[test]
    fn non_legato_key_up_schedules_release_without_a_later_tap() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 1, 10);
        let released = release(&mut engine, 1, 20);
        assert_eq!(
            commands(&released),
            vec![
                AudioCommand::DampenGroup {
                    at: time(20),
                    group: VoiceGroupId::new(1),
                },
                AudioCommand::ReleaseGroup {
                    at: time(20 + MINIMUM),
                    group: VoiceGroupId::new(1),
                },
            ]
        );
        engine
            .handle(PerformanceCommand::AdvanceClock {
                to: time(20 + MINIMUM + 10_000),
            })
            .unwrap();
        assert_eq!(engine.active_group_count(), 0);
    }

    #[test]
    fn later_tap_inside_minimum_attacks_now_and_releases_old_group_at_deadline() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        release(&mut engine, 1, 200);

        let second = tap(&mut engine, generation, 2, 300);
        let audio = commands(&second);
        assert_eq!(audio.len(), 2);
        assert_eq!(
            audio[0],
            AudioCommand::ReleaseGroup {
                at: time(200 + MINIMUM),
                group: first_group,
            }
        );
        let AudioCommand::PlaySlice { at, group, .. } = audio[1] else {
            panic!("second command must attack");
        };
        assert_eq!(at, time(300));
        assert_ne!(group, first_group);
        assert_eq!(engine.active_group_count(), 2);
    }

    #[test]
    fn later_tap_after_release_deadline_only_attacks_the_new_group() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let first = tap(&mut engine, generation, 1, 10);
        let _old_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        release(&mut engine, 1, 20);
        let later_at = 20 + MINIMUM + 500;
        let result = tap(&mut engine, generation, 2, later_at);
        assert!(matches!(
            commands(&result).as_slice(),
            [AudioCommand::PlaySlice { at, .. }] if *at == time(later_at)
        ));
    }

    #[test]
    fn written_duration_keeps_group_sounding_across_an_intermediate_slice() {
        let mut engine = engine();
        let score = ScoreSequence::new(vec![
            Slice::with_release_boundary(event(10), chord(&[60]), Some(event(30))),
            Slice::new(event(20), chord(&[64])),
            Slice::new(event(30), chord(&[67])),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();

        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        release(&mut engine, 1, 110);

        let intermediate = tap(&mut engine, generation, 2, 200);
        assert!(matches!(
            commands(&intermediate).as_slice(),
            [AudioCommand::PlaySlice { .. }]
        ));
        assert_eq!(
            engine
                .active_groups()
                .find(|group| group.id == first_group)
                .unwrap()
                .first_later_trigger_at,
            None
        );

        let boundary = tap(&mut engine, generation, 3, 300);
        assert_eq!(
            commands(&boundary)[0],
            AudioCommand::ReleaseGroup {
                at: time(110 + MINIMUM),
                group: first_group,
            }
        );
    }

    #[test]
    fn tap_release_is_scoped_to_staffs_whose_written_duration_has_ended() {
        let mut engine = engine();
        engine.set_legato_mode(true);
        let score = ScoreSequence::new(vec![
            Slice::from_staff_groups(
                event(10),
                &[
                    StaffSlice::new(1, chord(&[60]), SliceReleaseBoundary::OnEvent(event(20))),
                    StaffSlice::new(2, chord(&[48]), SliceReleaseBoundary::OnEvent(event(30))),
                ],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(20),
                &[StaffSlice::new(
                    1,
                    chord(&[62]),
                    SliceReleaseBoundary::OnEvent(event(30)),
                )],
            )
            .unwrap(),
            Slice::new(event(30), chord(&[64])),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();

        let first = tap(&mut engine, generation, 1, 100);
        let first_groups: Vec<_> = commands(&first)
            .iter()
            .filter_map(|command| match command {
                AudioCommand::PlaySlice { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(first_groups.len(), 2);
        let early_release = release(&mut engine, 1, 110);
        assert!(commands(&early_release).is_empty());

        let second = tap(&mut engine, generation, 2, 100 + MINIMUM);
        assert_eq!(
            commands(&second)[0],
            AudioCommand::DampenGroup {
                at: time(100 + MINIMUM),
                group: first_groups[0],
            }
        );
        let released: Vec<_> = commands(&second)
            .iter()
            .filter_map(|command| match command {
                AudioCommand::ReleaseGroup { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(released, vec![first_groups[0]]);
        assert!(engine
            .active_groups()
            .any(|group| group.id == first_groups[1] && group.release_scheduled_at.is_none()));
    }

    #[test]
    fn legato_future_key_down_releases_even_while_original_key_is_held() {
        let mut engine = engine();
        engine.set_legato_mode(true);
        let score = ScoreSequence::new(vec![
            Slice::from_staff_groups(
                event(10),
                &[StaffSlice::new(
                    1,
                    chord(&[60]),
                    SliceReleaseBoundary::OnEvent(event(20)),
                )],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(20),
                &[StaffSlice::new(
                    1,
                    chord(&[62]),
                    SliceReleaseBoundary::InputRelease,
                )],
            )
            .unwrap(),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();

        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };

        // The future attack is the release gesture; the original key may
        // still be physically held.
        let boundary = tap(&mut engine, generation, 2, 200);
        assert!(matches!(
            commands(&boundary).as_slice(),
            [
                AudioCommand::DampenGroup { group, .. },
                AudioCommand::ReleaseGroup { group: released, .. },
                AudioCommand::PlaySlice { .. }
            ] if *group == first_group && *released == first_group
        ));

        // Its eventual original key-up is only physical bookkeeping.
        let released = release(&mut engine, 1, 300);
        assert!(!commands(&released).iter().any(|command| matches!(
            command,
            AudioCommand::DampenGroup { group, .. }
                | AudioCommand::ReleaseGroup { group, .. } if *group == first_group
        )));
    }

    #[test]
    fn note_not_crossing_the_next_score_point_dampens_on_input_release() {
        let mut engine = engine();
        let score = ScoreSequence::new(vec![Slice::from_staff_groups(
            event(10),
            &[StaffSlice::new(
                1,
                chord(&[60]),
                SliceReleaseBoundary::InputRelease,
            )],
        )
        .unwrap()])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();
        let played = tap(&mut engine, generation, 1, 100);
        let group = match commands(&played)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };

        let released = release(&mut engine, 1, 200);
        assert_eq!(
            commands(&released),
            vec![
                AudioCommand::DampenGroup {
                    at: time(200),
                    group,
                },
                AudioCommand::ReleaseGroup {
                    at: time(200 + MINIMUM),
                    group,
                },
            ]
        );
    }

    #[test]
    fn non_legato_future_key_down_does_not_release_a_held_note() {
        let mut engine = engine();
        let score = ScoreSequence::new(vec![
            Slice::from_staff_groups(
                event(10),
                &[StaffSlice::new(
                    1,
                    chord(&[60]),
                    SliceReleaseBoundary::OnEvent(event(20)),
                )],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(20),
                &[StaffSlice::new(
                    1,
                    chord(&[62]),
                    SliceReleaseBoundary::InputRelease,
                )],
            )
            .unwrap(),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();
        tap(&mut engine, generation, 1, 100);

        let second = tap(&mut engine, generation, 2, 200);
        assert!(matches!(
            commands(&second).as_slice(),
            [AudioCommand::PlaySlice { .. }]
        ));
    }

    #[test]
    fn legato_key_up_releases_all_input_release_notes_not_only_its_own() {
        let mut engine = engine();
        engine.set_legato_mode(true);
        let score = ScoreSequence::new(vec![
            Slice::from_staff_groups(
                event(10),
                &[StaffSlice::new(
                    1,
                    chord(&[60]),
                    SliceReleaseBoundary::InputRelease,
                )],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(20),
                &[StaffSlice::new(
                    1,
                    chord(&[62]),
                    SliceReleaseBoundary::InputRelease,
                )],
            )
            .unwrap(),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();
        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        let second = tap(&mut engine, generation, 2, 200);
        let second_group = match commands(&second)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };

        let released = release(&mut engine, 2, 300);
        let dampened: Vec<_> = commands(&released)
            .iter()
            .filter_map(|command| match command {
                AudioCommand::DampenGroup { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(dampened, vec![first_group, second_group]);
    }

    #[test]
    fn notes_on_one_staff_release_independently_by_written_duration() {
        let mut engine = engine();
        engine.set_legato_mode(true);
        let score = ScoreSequence::new(vec![
            Slice::from_staff_groups(
                event(10),
                &[
                    StaffSlice::new(1, chord(&[60]), SliceReleaseBoundary::OnEvent(event(20))),
                    StaffSlice::new(1, chord(&[67]), SliceReleaseBoundary::OnEvent(event(30))),
                ],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(20),
                &[StaffSlice::new(
                    1,
                    chord(&[62]),
                    SliceReleaseBoundary::OnEvent(event(30)),
                )],
            )
            .unwrap(),
            Slice::from_staff_groups(
                event(30),
                &[StaffSlice::new(
                    1,
                    chord(&[69]),
                    SliceReleaseBoundary::EndOfScore,
                )],
            )
            .unwrap(),
        ])
        .unwrap();
        engine.load_score(score, time(0)).unwrap();
        let generation = engine.generation().unwrap();
        let first = tap(&mut engine, generation, 1, 100);
        let groups: Vec<_> = commands(&first)
            .iter()
            .filter_map(|command| match command {
                AudioCommand::PlaySlice { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        release(&mut engine, 1, 110);

        let second = tap(&mut engine, generation, 2, 100 + MINIMUM);
        let released: Vec<_> = commands(&second)
            .iter()
            .filter_map(|command| match command {
                AudioCommand::ReleaseGroup { group, .. } => Some(*group),
                _ => None,
            })
            .collect();
        assert_eq!(released, vec![groups[0]]);
        assert!(engine
            .active_groups()
            .any(|group| group.id == groups[1] && group.release_scheduled_at.is_none()));
    }

    #[test]
    fn held_group_survives_many_later_taps_then_gets_full_post_release_minimum() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        tap(&mut engine, generation, 2, 200);
        release(&mut engine, 2, 210);
        tap(&mut engine, generation, 3, 300);
        release(&mut engine, 3, 310);

        let released = release(&mut engine, 1, 1_000);
        assert_eq!(
            commands(&released),
            vec![
                AudioCommand::DampenGroup {
                    at: time(1_000),
                    group: first_group,
                },
                AudioCommand::ReleaseGroup {
                    at: time(1_000 + MINIMUM),
                    group: first_group,
                },
            ]
        );
        let first_state = engine
            .active_groups()
            .find(|group| group.id == first_group)
            .unwrap();
        assert_eq!(first_state.first_later_trigger_at, Some(time(200)));
        assert_eq!(first_state.input_released_at, Some(time(1_000)));
    }

    #[test]
    fn rapid_taps_age_as_independent_overlapping_groups() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let mut groups = Vec::new();
        for index in 0..3_u64 {
            let at = 100 + index * 100;
            let result = tap(&mut engine, generation, index + 1, at);
            groups.push(match commands(&result).last().copied().unwrap() {
                AudioCommand::PlaySlice { group, .. } => group,
                _ => unreachable!(),
            });
            release(&mut engine, index + 1, at + 10);
        }
        assert_eq!(engine.active_group_count(), 3);
        assert!(groups.windows(2).all(|pair| pair[0] != pair[1]));

        let state: Vec<_> = engine.active_groups().collect();
        assert_eq!(
            state[0].release_scheduled_at,
            Some(time(100 + 10 + MINIMUM))
        );
        assert_eq!(
            state[1].release_scheduled_at,
            Some(time(200 + 10 + MINIMUM))
        );
        assert_eq!(
            state[2].release_scheduled_at,
            Some(time(300 + 10 + MINIMUM))
        );
    }

    #[test]
    fn repeated_pitch_occurrences_have_isolated_group_ids() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        release(&mut engine, 1, 110);
        let second = tap(&mut engine, generation, 2, 120);
        let audio = commands(&second);
        let second_group = match audio[1] {
            AudioCommand::PlaySlice { group, chord, .. } => {
                assert_eq!(chord.pitches(), &[MidiPitch::new(60).unwrap()]);
                group
            }
            _ => unreachable!(),
        };
        assert_ne!(first_group, second_group);
        assert!(matches!(
            audio[0],
            AudioCommand::ReleaseGroup { group, .. } if group == first_group
        ));
    }

    #[test]
    fn auto_repeat_is_ignored_until_matching_release() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 7, 10);
        let repeated = tap(&mut engine, generation, 7, 11);
        assert!(commands(&repeated).is_empty());
        assert_eq!(
            repeated.event(),
            Some(&PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            })
        );
        assert_eq!(engine.cursor_index(), 1);

        release(&mut engine, 7, 12);
        tap(&mut engine, generation, 7, 13);
        assert_eq!(engine.cursor_index(), 2);
    }

    #[test]
    fn audition_does_not_advance_cursor_but_is_a_later_sounding_trigger() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let first = tap(&mut engine, generation, 1, 100);
        let first_group = match commands(&first)[0] {
            AudioCommand::PlaySlice { group, .. } => group,
            _ => unreachable!(),
        };
        release(&mut engine, 1, 110);
        let cursor_before = engine.cursor_index();
        let audition = engine
            .handle(PerformanceCommand::Audition {
                generation,
                event: event(30),
                input: input(2),
                at: time(120),
                velocity: Velocity::DEFAULT,
            })
            .unwrap();
        assert_eq!(engine.cursor_index(), cursor_before);
        assert_eq!(commands(&audition).len(), 2);
        assert!(matches!(
            commands(&audition)[0],
            AudioCommand::ReleaseGroup { group, .. } if group == first_group
        ));
        assert!(matches!(
            audition.event(),
            Some(PerformanceEvent::Triggered {
                event: id,
                next: Some(next),
                kind: TriggerKind::Audition,
                ..
            }) if *id == event(30) && *next == event(20)
        ));
    }

    #[test]
    fn single_note_audition_plays_only_requested_pitch_without_advancing() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        let audition = engine
            .handle(PerformanceCommand::AuditionNote {
                generation,
                event: event(10),
                pitch: MidiPitch::new(64).unwrap(),
                input: input(4),
                at: time(50),
                velocity: Velocity::DEFAULT,
            })
            .unwrap();

        assert_eq!(engine.cursor_index(), 0);
        assert!(matches!(
            commands(&audition).as_slice(),
            [AudioCommand::PlaySlice { chord, .. }]
                if chord.pitches() == [MidiPitch::new(64).unwrap()]
        ));
        assert!(matches!(
            audition.event(),
            Some(PerformanceEvent::Triggered {
                event: id,
                kind: TriggerKind::Audition,
                ..
            }) if *id == event(10)
        ));
    }

    #[test]
    fn reposition_immediately_panics_without_auditioning_or_advancing() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 1, 100);
        let moved = engine
            .handle(PerformanceCommand::Reposition {
                generation,
                event: event(30),
                at: time(101),
            })
            .unwrap();
        assert_eq!(engine.cursor_index(), 2);
        assert_eq!(engine.next_event(), Some(event(30)));
        assert_eq!(engine.active_group_count(), 0);
        assert_eq!(
            commands(&moved),
            vec![AudioCommand::Panic {
                at: time(101),
                reason: SafetyReason::Reposition,
            }]
        );

        // The original key remains physically latched through the safety stop.
        let repeated = tap(&mut engine, generation, 1, 102);
        assert!(commands(&repeated).is_empty());
        release(&mut engine, 1, 103);
        tap(&mut engine, generation, 1, 104);
        assert_eq!(engine.cursor_index(), 3);
    }

    #[test]
    fn every_safety_override_releases_all_groups_immediately() {
        for reason in [
            SafetyReason::Panic,
            SafetyReason::ActivePartsChanged,
            SafetyReason::AudioDeviceLost,
            SafetyReason::Shutdown,
        ] {
            let mut engine = engine();
            let generation = load(&mut engine, 0);
            tap(&mut engine, generation, 1, 10);
            let result = if reason == SafetyReason::Panic {
                engine
                    .handle(PerformanceCommand::Panic { at: time(11) })
                    .unwrap()
            } else {
                engine
                    .handle(PerformanceCommand::SafetyStop {
                        at: time(11),
                        reason,
                    })
                    .unwrap()
            };
            assert_eq!(engine.active_group_count(), 0);
            assert_eq!(
                commands(&result),
                vec![AudioCommand::Panic {
                    at: time(11),
                    reason,
                }]
            );
        }
    }

    #[test]
    fn panic_clamps_a_delayed_timestamp_and_always_releases() {
        let mut engine = engine();
        let generation = load(&mut engine, 100);
        tap(&mut engine, generation, 1, 110);
        let result = engine
            .handle(PerformanceCommand::Panic { at: time(10) })
            .unwrap();
        assert_eq!(engine.active_group_count(), 0);
        assert_eq!(
            commands(&result),
            vec![AudioCommand::Panic {
                at: time(110),
                reason: SafetyReason::Panic,
            }]
        );
    }

    #[test]
    fn loading_and_unloading_scores_are_immediate_safety_boundaries() {
        let mut engine = engine();
        let first = load(&mut engine, 0);
        tap(&mut engine, first, 1, 10);
        let loaded = engine.load_score(score(), time(11)).unwrap();
        let second = engine.generation().unwrap();
        assert_eq!(second.get(), first.get() + 1);
        assert_eq!(engine.active_group_count(), 0);
        assert!(matches!(
            commands(&loaded)[0],
            AudioCommand::Panic {
                reason: SafetyReason::ScoreLoad,
                ..
            }
        ));
        let unloaded = engine.unload_score(time(12)).unwrap();
        assert!(engine.generation().is_none());
        assert!(matches!(
            commands(&unloaded)[0],
            AudioCommand::Panic {
                reason: SafetyReason::ScoreUnload,
                ..
            }
        ));
    }

    #[test]
    fn stale_generation_is_rejected_without_moving_time_cursor_or_audio() {
        let mut engine = engine();
        let old = load(&mut engine, 100);
        engine.load_score(score(), time(200)).unwrap();
        let current = engine.generation().unwrap();
        let error = engine
            .handle(PerformanceCommand::Reposition {
                generation: old,
                event: event(30),
                at: time(50),
            })
            .unwrap_err();
        assert_eq!(
            error,
            EngineError::StaleGeneration {
                expected: current,
                received: old,
            }
        );
        assert_eq!(engine.cursor_index(), 0);
        // The rejected time did not poison the monotonic clock.
        tap(&mut engine, current, 1, 201);
    }

    #[test]
    fn non_monotonic_time_is_rejected_before_a_transition() {
        let mut engine = engine();
        let generation = load(&mut engine, 100);
        let error = engine
            .handle(PerformanceCommand::Tap {
                generation,
                input: input(1),
                at: time(99),
                velocity: Velocity::DEFAULT,
            })
            .unwrap_err();
        assert_eq!(
            error,
            EngineError::NonMonotonicSampleTime {
                previous: time(100),
                received: time(99),
            }
        );
        assert_eq!(engine.cursor_index(), 0);
    }

    #[test]
    fn capacity_failure_does_not_advance_cursor_or_modify_existing_gate() {
        let mut engine = PerformanceEngine::with_default_gate(
            RATE,
            EngineConfig {
                max_active_groups: 1,
                max_held_inputs: 2,
            },
        )
        .unwrap();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 1, 10);
        let error = engine
            .handle(PerformanceCommand::Tap {
                generation,
                input: input(2),
                at: time(20),
                velocity: Velocity::DEFAULT,
            })
            .unwrap_err();
        assert_eq!(
            error,
            EngineError::ActiveGroupCapacityExceeded { maximum: 1 }
        );
        assert_eq!(engine.cursor_index(), 1);
        assert_eq!(
            engine
                .active_groups()
                .next()
                .unwrap()
                .first_later_trigger_at,
            None
        );
    }

    #[test]
    fn end_of_score_down_is_latched_but_does_not_create_audio() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        for id in 1..=3 {
            tap(&mut engine, generation, id, id * 10);
            release(&mut engine, id, id * 10 + 1);
        }
        let end = tap(&mut engine, generation, 99, 100);
        assert!(commands(&end).is_empty());
        assert_eq!(
            end.event(),
            Some(&PerformanceEvent::Ignored {
                reason: IgnoreReason::EndOfScore,
            })
        );
        let repeated = tap(&mut engine, generation, 99, 101);
        assert_eq!(
            repeated.event(),
            Some(&PerformanceEvent::Ignored {
                reason: IgnoreReason::InputAlreadyHeld,
            })
        );
    }

    #[test]
    fn scheduled_groups_are_pruned_on_sample_clock_not_wall_time() {
        let mut engine = engine();
        let generation = load(&mut engine, 0);
        tap(&mut engine, generation, 1, 10);
        release(&mut engine, 1, 20);
        tap(&mut engine, generation, 2, 30);
        let deadline = 20 + MINIMUM;
        engine
            .handle(PerformanceCommand::AdvanceClock {
                to: time(deadline - 1),
            })
            .unwrap();
        assert_eq!(engine.active_group_count(), 2);
        engine
            .handle(PerformanceCommand::AdvanceClock { to: time(deadline) })
            .unwrap();
        assert_eq!(engine.active_group_count(), 1);
    }

    #[test]
    fn duplicate_score_event_ids_are_rejected() {
        let duplicate = ScoreSequence::new(vec![
            Slice::new(event(1), chord(&[60])),
            Slice::new(event(1), chord(&[61])),
        ]);
        assert_eq!(
            duplicate,
            Err(crate::ScoreSequenceError::DuplicateEventId(event(1)))
        );
    }

    #[test]
    fn exhaustive_gate_formula_matches_definition() {
        let gate = DefaultPianoGate;
        for release_at in [0, 1, 19_199, 50_000, 1_000_000] {
            for later_at in [0, 1, 100, 20_000, 2_000_000] {
                let actual = gate
                    .note_off_at(RATE, Some(time(release_at)), Some(time(later_at)))
                    .unwrap()
                    .unwrap();
                assert_eq!(actual.frame(), (release_at + MINIMUM).max(later_at));
                assert!(actual.frame() >= release_at + MINIMUM);
                assert!(actual.frame() >= later_at);
            }
        }
    }
}

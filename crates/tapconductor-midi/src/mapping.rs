// Copyright (c) 2026 Michael Saunders
use crate::{MidiChannel, MidiMessage, MidiNote, MidiTimestamp, TimestampedMidiMessage, Velocity};

/// Unique token paired across one MIDI Note On/Note Off gesture.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiInputToken(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VelocityCurve {
    Linear,
    /// Expands quieter controller velocities.
    Soft,
    /// Gives more control near the loud end.
    Hard,
    Fixed(Velocity),
}

impl VelocityCurve {
    pub fn apply(self, velocity: Velocity) -> Velocity {
        match self {
            Self::Linear => velocity,
            Self::Soft => {
                let normalized = velocity.get() as f32 / u16::MAX as f32;
                Velocity::new((normalized.sqrt() * u16::MAX as f32).round() as u16)
            }
            Self::Hard => {
                let value = velocity.get() as u32;
                Velocity::new(((value * value) / u16::MAX as u32) as u16)
            }
            Self::Fixed(value) => value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiInputConfig {
    /// Bit zero enables channel 1; all bits set accepts every MIDI 1 channel.
    pub channel_mask: u16,
    pub minimum_note: MidiNote,
    pub maximum_note: MidiNote,
    pub minimum_velocity: Velocity,
    pub velocity_curve: VelocityCurve,
    pub respect_sustain_pedal: bool,
}

impl Default for MidiInputConfig {
    fn default() -> Self {
        Self {
            channel_mask: u16::MAX,
            minimum_note: MidiNote::new(0).expect("valid constant"),
            maximum_note: MidiNote::new(127).expect("valid constant"),
            minimum_velocity: Velocity::new(1),
            velocity_curve: VelocityCurve::Linear,
            respect_sustain_pedal: false,
        }
    }
}

impl MidiInputConfig {
    fn accepts(&self, channel: MidiChannel, note: MidiNote, velocity: Velocity) -> bool {
        self.channel_mask & (1 << channel.zero_based()) != 0
            && note >= self.minimum_note
            && note <= self.maximum_note
            && velocity >= self.minimum_velocity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiTapEvent {
    /// Conductor tap. The source pitch is retained for diagnostics/mappings but
    /// normal rhythm mode uses only token, timestamp, and velocity.
    Down {
        token: MidiInputToken,
        timestamp: MidiTimestamp,
        channel: MidiChannel,
        source_note: MidiNote,
        velocity: Velocity,
    },
    Up {
        token: MidiInputToken,
        timestamp: MidiTimestamp,
    },
    /// Safety reset from All Sound Off, All Notes Off, Reset, or disconnect.
    Panic { timestamp: MidiTimestamp },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MapResult {
    pub emitted: usize,
    pub ignored: bool,
    /// True when the fixed held-key table was full. In that case no Down event
    /// is emitted, avoiding a gesture that could never be matched safely.
    pub tracker_full: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HeldInput {
    token: MidiInputToken,
    channel: MidiChannel,
    note: MidiNote,
    key_released: bool,
}

/// Allocation-free Note On -> tap mapper with paired release tokens.
pub struct MidiInputMapper<const HELD: usize = 128> {
    config: MidiInputConfig,
    held: [Option<HeldInput>; HELD],
    sustain: [bool; 16],
    next_token: u64,
}

impl<const HELD: usize> MidiInputMapper<HELD> {
    pub fn new(config: MidiInputConfig) -> Self {
        assert!(HELD > 0, "MIDI held-input capacity must be non-zero");
        Self {
            config,
            held: [None; HELD],
            sustain: [false; 16],
            next_token: 1,
        }
    }

    pub const fn config(&self) -> &MidiInputConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: MidiInputConfig) {
        self.config = config;
    }

    pub fn held_count(&self) -> usize {
        self.held.iter().filter(|entry| entry.is_some()).count()
    }

    /// Maps one raw MIDI message and synchronously emits zero or more events.
    /// The supplied callback is invoked directly; this method never allocates.
    pub fn process(
        &mut self,
        input: TimestampedMidiMessage,
        mut emit: impl FnMut(MidiTapEvent),
    ) -> MapResult {
        match input.message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => self.note_on(input.timestamp, channel, note, velocity, &mut emit),
            MidiMessage::NoteOff { channel, note, .. } => {
                self.note_off(input.timestamp, channel, note, &mut emit)
            }
            MidiMessage::ControlChange {
                channel,
                controller: 64,
                value,
            } if self.config.respect_sustain_pedal => {
                self.sustain(input.timestamp, channel, value >= 64, &mut emit)
            }
            MidiMessage::ControlChange {
                channel,
                controller: 120 | 123,
                ..
            } => self.panic_channel(input.timestamp, channel, &mut emit),
            MidiMessage::Reset => self.panic_all(input.timestamp, &mut emit),
            _ => MapResult {
                ignored: true,
                ..MapResult::default()
            },
        }
    }

    /// Clears all matching state after a device disconnect and requests an
    /// unconditional audio/MIDI panic.
    pub fn disconnect(&mut self, timestamp: MidiTimestamp, mut emit: impl FnMut(MidiTapEvent)) {
        self.held.fill(None);
        self.sustain.fill(false);
        emit(MidiTapEvent::Panic { timestamp });
    }

    fn note_on(
        &mut self,
        timestamp: MidiTimestamp,
        channel: MidiChannel,
        note: MidiNote,
        velocity: Velocity,
        emit: &mut impl FnMut(MidiTapEvent),
    ) -> MapResult {
        if velocity == Velocity::ZERO || !self.config.accepts(channel, note, velocity) {
            return MapResult {
                ignored: true,
                ..MapResult::default()
            };
        }
        let Some(slot) = self.held.iter().position(Option::is_none) else {
            return MapResult {
                tracker_full: true,
                ..MapResult::default()
            };
        };
        let token = MidiInputToken(self.next_token);
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.held[slot] = Some(HeldInput {
            token,
            channel,
            note,
            key_released: false,
        });
        emit(MidiTapEvent::Down {
            token,
            timestamp,
            channel,
            source_note: note,
            velocity: self.config.velocity_curve.apply(velocity),
        });
        MapResult {
            emitted: 1,
            ..MapResult::default()
        }
    }

    fn note_off(
        &mut self,
        timestamp: MidiTimestamp,
        channel: MidiChannel,
        note: MidiNote,
        emit: &mut impl FnMut(MidiTapEvent),
    ) -> MapResult {
        // FIFO matching makes repeated Note Ons for the same channel/note
        // deterministic under MIDI 1.0, which carries no per-note identifier.
        let selected = self
            .held
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.map(|held| (index, held)))
            .filter(|(_, held)| held.channel == channel && held.note == note && !held.key_released)
            .min_by_key(|(_, held)| held.token)
            .map(|(index, _)| index);
        let Some(index) = selected else {
            return MapResult {
                ignored: true,
                ..MapResult::default()
            };
        };

        if self.config.respect_sustain_pedal && self.sustain[channel.zero_based() as usize] {
            self.held[index]
                .as_mut()
                .expect("selected held key")
                .key_released = true;
            return MapResult::default();
        }

        let token = self.held[index].take().expect("selected held key").token;
        emit(MidiTapEvent::Up { token, timestamp });
        MapResult {
            emitted: 1,
            ..MapResult::default()
        }
    }

    fn sustain(
        &mut self,
        timestamp: MidiTimestamp,
        channel: MidiChannel,
        down: bool,
        emit: &mut impl FnMut(MidiTapEvent),
    ) -> MapResult {
        let channel_index = channel.zero_based() as usize;
        let was_down = self.sustain[channel_index];
        self.sustain[channel_index] = down;
        if down || !was_down {
            return MapResult::default();
        }

        let mut emitted = 0usize;
        for entry in &mut self.held {
            if entry
                .as_ref()
                .is_some_and(|held| held.channel == channel && held.key_released)
            {
                let token = entry.take().expect("matched held key").token;
                emit(MidiTapEvent::Up { token, timestamp });
                emitted += 1;
            }
        }
        MapResult {
            emitted,
            ..MapResult::default()
        }
    }

    fn panic_channel(
        &mut self,
        timestamp: MidiTimestamp,
        channel: MidiChannel,
        emit: &mut impl FnMut(MidiTapEvent),
    ) -> MapResult {
        for entry in &mut self.held {
            if entry.as_ref().is_some_and(|held| held.channel == channel) {
                *entry = None;
            }
        }
        self.sustain[channel.zero_based() as usize] = false;
        emit(MidiTapEvent::Panic { timestamp });
        MapResult {
            emitted: 1,
            ..MapResult::default()
        }
    }

    fn panic_all(
        &mut self,
        timestamp: MidiTimestamp,
        emit: &mut impl FnMut(MidiTapEvent),
    ) -> MapResult {
        self.held.fill(None);
        self.sustain.fill(false);
        emit(MidiTapEvent::Panic { timestamp });
        MapResult {
            emitted: 1,
            ..MapResult::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(timestamp: u64, message: MidiMessage) -> TimestampedMidiMessage {
        TimestampedMidiMessage {
            timestamp: MidiTimestamp(timestamp),
            message,
        }
    }

    fn note_on(timestamp: u64, note: u8, velocity: u8) -> TimestampedMidiMessage {
        input(
            timestamp,
            MidiMessage::NoteOn {
                channel: MidiChannel::new(0).unwrap(),
                note: MidiNote::new(note).unwrap(),
                velocity: Velocity::from_midi1(velocity).unwrap(),
            },
        )
    }

    fn note_off(timestamp: u64, note: u8) -> TimestampedMidiMessage {
        input(
            timestamp,
            MidiMessage::NoteOff {
                channel: MidiChannel::new(0).unwrap(),
                note: MidiNote::new(note).unwrap(),
                velocity: Velocity::ZERO,
            },
        )
    }

    #[test]
    fn note_on_is_velocity_tap_and_note_off_matches_token() {
        let mut mapper = MidiInputMapper::<8>::new(MidiInputConfig::default());
        let mut events = Vec::new();
        mapper.process(note_on(10, 72, 96), |event| events.push(event));
        mapper.process(note_off(20, 72), |event| events.push(event));
        let MidiTapEvent::Down {
            token, velocity, ..
        } = events[0]
        else {
            panic!("expected down")
        };
        assert_eq!(velocity, Velocity::from_midi1(96).unwrap());
        assert_eq!(
            events[1],
            MidiTapEvent::Up {
                token,
                timestamp: MidiTimestamp(20)
            }
        );
    }

    #[test]
    fn repeated_pitch_releases_tokens_fifo() {
        let mut mapper = MidiInputMapper::<8>::new(MidiInputConfig::default());
        let mut events = Vec::new();
        mapper.process(note_on(1, 60, 80), |event| events.push(event));
        mapper.process(note_on(2, 60, 90), |event| events.push(event));
        mapper.process(note_off(3, 60), |event| events.push(event));
        mapper.process(note_off(4, 60), |event| events.push(event));
        let first = match events[0] {
            MidiTapEvent::Down { token, .. } => token,
            _ => unreachable!(),
        };
        let second = match events[1] {
            MidiTapEvent::Down { token, .. } => token,
            _ => unreachable!(),
        };
        assert_eq!(
            events[2],
            MidiTapEvent::Up {
                token: first,
                timestamp: MidiTimestamp(3)
            }
        );
        assert_eq!(
            events[3],
            MidiTapEvent::Up {
                token: second,
                timestamp: MidiTimestamp(4)
            }
        );
    }

    #[test]
    fn sustain_defers_physical_release_until_pedal_up() {
        let config = MidiInputConfig {
            respect_sustain_pedal: true,
            ..MidiInputConfig::default()
        };
        let mut mapper = MidiInputMapper::<8>::new(config);
        let channel = MidiChannel::new(0).unwrap();
        let mut events = Vec::new();
        mapper.process(note_on(1, 60, 100), |event| events.push(event));
        mapper.process(
            input(
                2,
                MidiMessage::ControlChange {
                    channel,
                    controller: 64,
                    value: 127,
                },
            ),
            |event| events.push(event),
        );
        mapper.process(note_off(3, 60), |event| events.push(event));
        assert_eq!(events.len(), 1);
        mapper.process(
            input(
                4,
                MidiMessage::ControlChange {
                    channel,
                    controller: 64,
                    value: 0,
                },
            ),
            |event| events.push(event),
        );
        assert!(matches!(
            events[1],
            MidiTapEvent::Up {
                timestamp: MidiTimestamp(4),
                ..
            }
        ));
    }

    #[test]
    fn full_tracker_never_emits_unmatchable_down() {
        let mut mapper = MidiInputMapper::<1>::new(MidiInputConfig::default());
        mapper.process(note_on(1, 60, 100), |_| {});
        let result = mapper.process(note_on(2, 62, 100), |_| panic!("must not emit"));
        assert!(result.tracker_full);
    }
}

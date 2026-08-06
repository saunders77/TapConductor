// Copyright (c) 2026 Michael Saunders
use core::fmt;

/// Timestamp in microseconds on the backend's monotonic clock.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiTimestamp(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiChannel(u8);

impl MidiChannel {
    pub const fn new(zero_based: u8) -> Result<Self, MidiValueError> {
        if zero_based < 16 {
            Ok(Self(zero_based))
        } else {
            Err(MidiValueError::Channel(zero_based))
        }
    }

    pub const fn zero_based(self) -> u8 {
        self.0
    }

    pub const fn one_based(self) -> u8 {
        self.0 + 1
    }

    pub(crate) const fn from_status(status: u8) -> Self {
        Self(status & 0x0f)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct MidiNote(u8);

impl MidiNote {
    pub const MIN: Self = Self(0);
    pub const MAX: Self = Self(127);

    pub const fn new(note: u8) -> Result<Self, MidiValueError> {
        if note < 128 {
            Ok(Self(note))
        } else {
            Err(MidiValueError::Note(note))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    pub(crate) const fn from_data(note: u8) -> Self {
        Self(note)
    }
}

/// Internal 16-bit velocity, leaving room for MIDI 2.0 without an API change.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Velocity(u16);

impl Velocity {
    pub const ZERO: Self = Self(0);
    pub const MAX: Self = Self(u16::MAX);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn from_midi1(value: u8) -> Result<Self, MidiValueError> {
        if value < 128 {
            Ok(Self((value as u32 * u16::MAX as u32 / 127) as u16))
        } else {
            Err(MidiValueError::DataByte(value))
        }
    }

    pub const fn to_midi1(self) -> u8 {
        ((self.0 as u32 * 127 + (u16::MAX as u32 / 2)) / u16::MAX as u32) as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiValueError {
    Channel(u8),
    Note(u8),
    DataByte(u8),
}

impl fmt::Display for MidiValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid MIDI value: {self:?}")
    }
}

impl std::error::Error for MidiValueError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiMessage {
    NoteOn {
        channel: MidiChannel,
        note: MidiNote,
        velocity: Velocity,
    },
    NoteOff {
        channel: MidiChannel,
        note: MidiNote,
        velocity: Velocity,
    },
    PolyphonicPressure {
        channel: MidiChannel,
        note: MidiNote,
        pressure: Velocity,
    },
    ControlChange {
        channel: MidiChannel,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: MidiChannel,
        program: u8,
    },
    ChannelPressure {
        channel: MidiChannel,
        pressure: Velocity,
    },
    PitchBend {
        channel: MidiChannel,
        /// Fourteen-bit value, centered at 8192.
        value: u16,
    },
    TimingClock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampedMidiMessage {
    pub timestamp: MidiTimestamp,
    pub message: MidiMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Midi1Packet {
    bytes: [u8; 3],
    len: u8,
}

impl Midi1Packet {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl MidiMessage {
    /// Serializes channel voice and supported real-time messages without heap
    /// allocation. Every enum variant currently has a MIDI 1.0 representation.
    pub fn to_midi1(self) -> Midi1Packet {
        let (bytes, len) = match self {
            Self::NoteOff {
                channel,
                note,
                velocity,
            } => (
                [0x80 | channel.zero_based(), note.get(), velocity.to_midi1()],
                3,
            ),
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => (
                [0x90 | channel.zero_based(), note.get(), velocity.to_midi1()],
                3,
            ),
            Self::PolyphonicPressure {
                channel,
                note,
                pressure,
            } => (
                [0xa0 | channel.zero_based(), note.get(), pressure.to_midi1()],
                3,
            ),
            Self::ControlChange {
                channel,
                controller,
                value,
            } => ([0xb0 | channel.zero_based(), controller, value], 3),
            Self::ProgramChange { channel, program } => {
                ([0xc0 | channel.zero_based(), program, 0], 2)
            }
            Self::ChannelPressure { channel, pressure } => {
                ([0xd0 | channel.zero_based(), pressure.to_midi1(), 0], 2)
            }
            Self::PitchBend { channel, value } => (
                [
                    0xe0 | channel.zero_based(),
                    (value & 0x7f) as u8,
                    ((value >> 7) & 0x7f) as u8,
                ],
                3,
            ),
            Self::TimingClock => ([0xf8, 0, 0], 1),
            Self::Start => ([0xfa, 0, 0], 1),
            Self::Continue => ([0xfb, 0, 0], 1),
            Self::Stop => ([0xfc, 0, 0], 1),
            Self::ActiveSensing => ([0xfe, 0, 0], 1),
            Self::Reset => ([0xff, 0, 0], 1),
        };
        Midi1Packet { bytes, len }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MidiParseError {
    Empty,
    RunningStatusUnsupported,
    Truncated { expected: usize, actual: usize },
    InvalidDataByte(u8),
}

impl fmt::Display for MidiParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid MIDI 1.0 message: {self:?}")
    }
}

impl std::error::Error for MidiParseError {}

/// Parses one complete MIDI 1.0 message. SysEx and unsupported system-common
/// messages return `Ok(None)` so a backend can safely ignore them.
pub fn parse_midi1(bytes: &[u8]) -> Result<Option<MidiMessage>, MidiParseError> {
    let status = *bytes.first().ok_or(MidiParseError::Empty)?;
    if status < 0x80 {
        return Err(MidiParseError::RunningStatusUnsupported);
    }

    let channel = MidiChannel::from_status(status);
    let needed = match status & 0xf0 {
        0x80 | 0x90 | 0xa0 | 0xb0 | 0xe0 => 3,
        0xc0 | 0xd0 => 2,
        0xf0 => 1,
        _ => unreachable!(),
    };
    if bytes.len() < needed {
        return Err(MidiParseError::Truncated {
            expected: needed,
            actual: bytes.len(),
        });
    }
    for data in &bytes[1..needed] {
        if *data >= 0x80 {
            return Err(MidiParseError::InvalidDataByte(*data));
        }
    }

    let data1 = bytes.get(1).copied().unwrap_or(0);
    let data2 = bytes.get(2).copied().unwrap_or(0);
    let message = match status & 0xf0 {
        0x80 => Some(MidiMessage::NoteOff {
            channel,
            note: MidiNote::from_data(data1),
            velocity: Velocity::from_midi1(data2).expect("validated data byte"),
        }),
        0x90 if data2 == 0 => Some(MidiMessage::NoteOff {
            channel,
            note: MidiNote::from_data(data1),
            velocity: Velocity::ZERO,
        }),
        0x90 => Some(MidiMessage::NoteOn {
            channel,
            note: MidiNote::from_data(data1),
            velocity: Velocity::from_midi1(data2).expect("validated data byte"),
        }),
        0xa0 => Some(MidiMessage::PolyphonicPressure {
            channel,
            note: MidiNote::from_data(data1),
            pressure: Velocity::from_midi1(data2).expect("validated data byte"),
        }),
        0xb0 => Some(MidiMessage::ControlChange {
            channel,
            controller: data1,
            value: data2,
        }),
        0xc0 => Some(MidiMessage::ProgramChange {
            channel,
            program: data1,
        }),
        0xd0 => Some(MidiMessage::ChannelPressure {
            channel,
            pressure: Velocity::from_midi1(data1).expect("validated data byte"),
        }),
        0xe0 => Some(MidiMessage::PitchBend {
            channel,
            value: data1 as u16 | ((data2 as u16) << 7),
        }),
        0xf0 => match status {
            0xf8 => Some(MidiMessage::TimingClock),
            0xfa => Some(MidiMessage::Start),
            0xfb => Some(MidiMessage::Continue),
            0xfc => Some(MidiMessage::Stop),
            0xfe => Some(MidiMessage::ActiveSensing),
            0xff => Some(MidiMessage::Reset),
            _ => None,
        },
        _ => None,
    };
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_zero_is_canonical_note_off() {
        let message = parse_midi1(&[0x92, 60, 0]).unwrap().unwrap();
        assert_eq!(
            message,
            MidiMessage::NoteOff {
                channel: MidiChannel::new(2).unwrap(),
                note: MidiNote::new(60).unwrap(),
                velocity: Velocity::ZERO,
            }
        );
    }

    #[test]
    fn velocity_round_trip_reaches_both_endpoints() {
        assert_eq!(Velocity::from_midi1(0).unwrap(), Velocity::ZERO);
        assert_eq!(Velocity::from_midi1(127).unwrap(), Velocity::MAX);
        for value in 0..=127 {
            assert_eq!(Velocity::from_midi1(value).unwrap().to_midi1(), value);
        }
    }

    #[test]
    fn serializes_without_allocation() {
        let packet = MidiMessage::NoteOn {
            channel: MidiChannel::new(0).unwrap(),
            note: MidiNote::new(64).unwrap(),
            velocity: Velocity::MAX,
        }
        .to_midi1();
        assert_eq!(packet.bytes(), &[0x90, 64, 127]);
    }
}

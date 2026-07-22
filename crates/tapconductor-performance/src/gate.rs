use core::fmt;

use crate::{SampleRate, SampleTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GateError {
    SampleTimeOverflow,
}

impl fmt::Display for GateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SampleTimeOverflow => {
                formatter.write_str("gate deadline exceeds the sample clock")
            }
        }
    }
}

impl std::error::Error for GateError {}

/// Defines voice-group note-off timing without coupling it to a sampler.
pub trait GatePolicy {
    /// Returns `None` until the policy has enough information to release.
    fn note_off_at(
        &self,
        sample_rate: SampleRate,
        input_released_at: Option<SampleTime>,
        first_later_trigger_at: Option<SampleTime>,
    ) -> Result<Option<SampleTime>, GateError>;
}

/// The MVP piano gate: `max(first later trigger, input release + 100 ms)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DefaultPianoGate;

impl DefaultPianoGate {
    pub const MINIMUM_AFTER_RELEASE_MILLISECONDS: u64 = 100;

    /// Uses ceiling division, so the minimum is never rounded below 100 ms.
    #[must_use]
    pub const fn minimum_frames(sample_rate: SampleRate) -> u64 {
        let numerator = sample_rate.get() as u64 * Self::MINIMUM_AFTER_RELEASE_MILLISECONDS;
        numerator.div_ceil(1_000)
    }
}

impl GatePolicy for DefaultPianoGate {
    fn note_off_at(
        &self,
        sample_rate: SampleRate,
        input_released_at: Option<SampleTime>,
        first_later_trigger_at: Option<SampleTime>,
    ) -> Result<Option<SampleTime>, GateError> {
        let (Some(released), Some(later)) = (input_released_at, first_later_trigger_at) else {
            return Ok(None);
        };

        let minimum = released
            .checked_add(Self::minimum_frames(sample_rate))
            .ok_or(GateError::SampleTimeOverflow)?;
        Ok(Some(core::cmp::max(minimum, later)))
    }
}

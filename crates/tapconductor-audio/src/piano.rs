// Copyright (c) 2026 Michael Saunders
use crate::{Sampler, VoiceGroupId, VoiceStart};
use core::f32::consts::TAU;

const PARTIAL_COUNT: usize = 6;
const PARTIAL_RATIOS: [f32; PARTIAL_COUNT] = [1.0, 2.006, 3.018, 4.034, 5.056, 6.082];
const PARTIAL_GAINS: [f32; PARTIAL_COUNT] = [1.0, 0.48, 0.30, 0.21, 0.15, 0.11];
const PARTIAL_HALF_LIVES: [f32; PARTIAL_COUNT] = [10_000.0, 2.8, 1.9, 1.35, 1.0, 0.75];
// A short fade-in keeps a newly allocated oscillator from entering the mix at
// its non-zero starting phase. Eight milliseconds is long enough to remove
// the discontinuity without making the instrument feel sluggish.
const ATTACK_SECONDS: f32 = 0.008;

/// Controls the dependency-free fallback instrument.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PianoConfig {
    pub sample_rate: u32,
    /// Half-life of the naturally decaying held strike.
    pub held_half_life_seconds: f32,
    /// Half-life after the group receives note-off.
    pub release_half_life_seconds: f32,
    pub output_gain: f32,
}

impl PianoConfig {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            held_half_life_seconds: 0.85,
            release_half_life_seconds: 0.025,
            output_gain: 0.12,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    released: bool,
    group: VoiceGroupId,
    pitch: u8,
    oscillator: [f32; PARTIAL_COUNT],
    previous: [f32; PARTIAL_COUNT],
    partial_amplitudes: [f32; PARTIAL_COUNT],
    amplitude: f32,
    attack_gain: f32,
    age: u64,
}

impl Voice {
    const SILENT: Self = Self {
        active: false,
        released: false,
        group: VoiceGroupId(0),
        pitch: 0,
        oscillator: [0.0; PARTIAL_COUNT],
        previous: [0.0; PARTIAL_COUNT],
        partial_amplitudes: [0.0; PARTIAL_COUNT],
        amplitude: 0.0,
        attack_gain: 0.0,
        age: 0,
    };
}

/// A small, fixed-pool piano-like synthesizer available without sample assets.
///
/// It is intentionally an acceptable bootstrap/recovery instrument rather
/// than a replacement for a licensed multisampled piano. A strike combines
/// inharmonic partials and decays continuously even while held; therefore the
/// 400 ms gate policy never stretches or freezes its envelope.
pub struct PianoSynth<const VOICES: usize = 128> {
    voices: [Voice; VOICES],
    oscillator_coefficients: [[f32; PARTIAL_COUNT]; 128],
    oscillator_starts: [[[f32; 2]; PARTIAL_COUNT]; 128],
    partial_decay: [f32; PARTIAL_COUNT],
    held_decay: f32,
    release_decay: f32,
    attack_step: f32,
    output_gain: f32,
    next_age: u64,
}

impl<const VOICES: usize> PianoSynth<VOICES> {
    pub fn new(config: PianoConfig) -> Result<Self, PianoConfigError> {
        if config.sample_rate == 0 {
            return Err(PianoConfigError::ZeroSampleRate);
        }
        if !config.held_half_life_seconds.is_finite() || config.held_half_life_seconds <= 0.0 {
            return Err(PianoConfigError::InvalidHeldHalfLife);
        }
        if !config.release_half_life_seconds.is_finite() || config.release_half_life_seconds <= 0.0
        {
            return Err(PianoConfigError::InvalidReleaseHalfLife);
        }
        if !config.output_gain.is_finite() || config.output_gain < 0.0 {
            return Err(PianoConfigError::InvalidGain);
        }
        if VOICES == 0 {
            return Err(PianoConfigError::ZeroVoices);
        }

        let held_decay =
            0.5_f32.powf(1.0 / (config.held_half_life_seconds * config.sample_rate as f32));
        let release_decay =
            0.5_f32.powf(1.0 / (config.release_half_life_seconds * config.sample_rate as f32));
        let attack_step = 1.0 / (ATTACK_SECONDS * config.sample_rate as f32).max(1.0);
        let mut oscillator_coefficients = [[0.0; PARTIAL_COUNT]; 128];
        let mut oscillator_starts = [[[0.0; 2]; PARTIAL_COUNT]; 128];
        for pitch in 0..128 {
            let frequency = 440.0 * 2.0_f32.powf((pitch as f32 - 69.0) / 12.0);
            for (partial_index, partial) in PARTIAL_RATIOS.iter().copied().enumerate() {
                let step = TAU * frequency * partial / config.sample_rate as f32;
                // Do not synthesize a partial at/above Nyquist. Leaving both
                // recurrence states at zero prevents aliased brightness in
                // the upper register.
                if step >= core::f32::consts::PI * 0.98 {
                    continue;
                }
                let phase = 0.17 * partial;
                oscillator_coefficients[pitch][partial_index] = 2.0 * step.cos();
                oscillator_starts[pitch][partial_index] = [phase.sin(), (phase - step).sin()];
            }
        }
        let partial_decay = PARTIAL_HALF_LIVES
            .map(|half_life| 0.5_f32.powf(1.0 / (half_life * config.sample_rate as f32)));
        Ok(Self {
            voices: [Voice::SILENT; VOICES],
            oscillator_coefficients,
            oscillator_starts,
            partial_decay,
            held_decay,
            release_decay,
            attack_step,
            output_gain: config.output_gain,
            next_age: 1,
        })
    }

    fn allocate_voice(&mut self) -> (usize, bool) {
        if let Some(index) = self.voices.iter().position(|voice| !voice.active) {
            return (index, false);
        }

        // Prefer an already-released/quiet voice, then the oldest. This scan is
        // bounded by the compile-time voice count.
        let mut best = 0usize;
        for index in 1..VOICES {
            let candidate = &self.voices[index];
            let current = &self.voices[best];
            if (candidate.released && !current.released)
                || (candidate.released == current.released
                    && (candidate.amplitude < current.amplitude
                        || (candidate.amplitude == current.amplitude
                            && candidate.age < current.age)))
            {
                best = index;
            }
        }
        (best, true)
    }

    pub fn active_groups(&self) -> usize {
        let mut groups = [VoiceGroupId(0); VOICES];
        let mut count = 0usize;
        for voice in self.voices.iter().filter(|voice| voice.active) {
            if !groups[..count].contains(&voice.group) {
                groups[count] = voice.group;
                count += 1;
            }
        }
        count
    }
}

impl<const VOICES: usize> Sampler for PianoSynth<VOICES> {
    fn note_on(&mut self, group: VoiceGroupId, pitch: u8, velocity: u16) -> VoiceStart {
        if pitch > 127 || velocity == 0 {
            return VoiceStart::Rejected;
        }
        let (index, stole) = self.allocate_voice();
        let velocity = velocity as f32 / u16::MAX as f32;
        // A square-root curve keeps quiet rehearsal velocities audible.
        let amplitude = velocity.sqrt();
        // A firmer strike excites progressively more upper string modes. The
        // base profile is intentionally bright enough to remain distinct
        // against singers even at the default rehearsal velocity.
        let brightness = 0.9 + velocity * 0.22;
        let mut brightness_power = 1.0;
        let mut partial_amplitudes = [0.0; PARTIAL_COUNT];
        for partial in 0..PARTIAL_COUNT {
            partial_amplitudes[partial] = PARTIAL_GAINS[partial] * brightness_power;
            if self.oscillator_starts[pitch as usize][partial] == [0.0, 0.0] {
                partial_amplitudes[partial] = 0.0;
            }
            brightness_power *= brightness;
        }
        self.voices[index] = Voice {
            active: true,
            released: false,
            group,
            pitch,
            oscillator: self.oscillator_starts[pitch as usize].map(|state| state[0]),
            previous: self.oscillator_starts[pitch as usize].map(|state| state[1]),
            partial_amplitudes,
            amplitude,
            attack_gain: 0.0,
            age: self.next_age,
        };
        self.next_age = self.next_age.wrapping_add(1);
        if stole {
            VoiceStart::StoleOlderVoice
        } else {
            VoiceStart::Started
        }
    }

    fn release_group(&mut self, group: VoiceGroupId) {
        for voice in &mut self.voices {
            if voice.active && voice.group == group {
                voice.released = true;
            }
        }
    }

    fn panic(&mut self) {
        self.voices.fill(Voice::SILENT);
    }

    fn render(&mut self, interleaved: &mut [f32], channels: usize) {
        if channels == 0 {
            return;
        }
        for frame in interleaved.chunks_exact_mut(channels) {
            let mut mixed = 0.0_f32;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                // Stiff-string inharmonic partials create the bright hammer
                // attack. Their individual envelopes decay progressively
                // faster, as measured piano partials do.
                let mut voice_sample = 0.0;
                for partial in 0..PARTIAL_COUNT {
                    voice_sample += voice.oscillator[partial] * voice.partial_amplitudes[partial];
                    let next = self.oscillator_coefficients[voice.pitch as usize][partial]
                        * voice.oscillator[partial]
                        - voice.previous[partial];
                    voice.previous[partial] = voice.oscillator[partial];
                    voice.oscillator[partial] = next;
                    voice.partial_amplitudes[partial] *= self.partial_decay[partial];
                }
                mixed += voice_sample * voice.amplitude * voice.attack_gain;
                voice.attack_gain = (voice.attack_gain + self.attack_step).min(1.0);
                voice.amplitude *= if voice.released {
                    self.release_decay
                } else {
                    self.held_decay
                };
                if voice.amplitude < 0.00001 {
                    *voice = Voice::SILENT;
                }
            }
            // A cheap hard safety limiter avoids transcendental work in the
            // callback; ordinary gain keeps normal chords far below clipping.
            let sample = (mixed * self.output_gain).clamp(-1.0, 1.0);
            for channel in frame {
                *channel += sample;
            }
        }
    }

    fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PianoConfigError {
    ZeroSampleRate,
    ZeroVoices,
    InvalidHeldHalfLife,
    InvalidReleaseHalfLife,
    InvalidGain,
}

impl core::fmt::Display for PianoConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid fallback piano configuration: {self:?}")
    }
}

impl std::error::Error for PianoConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_piano_strike_naturally_decays_without_note_off() {
        let mut synth = PianoSynth::<8>::new(PianoConfig::new(1_000)).unwrap();
        synth.note_on(VoiceGroupId(1), 60, u16::MAX);
        let mut first = [0.0; 250];
        synth.render(&mut first, 1);
        let mut later = [0.0; 1_000];
        synth.render(&mut later, 1);
        let early_peak = first.iter().copied().map(f32::abs).fold(0.0, f32::max);
        let late_peak = later[750..]
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0, f32::max);
        assert!(late_peak < early_peak * 0.6);
    }

    #[test]
    fn note_attack_fades_in_from_silence() {
        let mut synth = PianoSynth::<8>::new(PianoConfig::new(48_000)).unwrap();
        synth.note_on(VoiceGroupId(1), 60, u16::MAX);
        synth.note_on(VoiceGroupId(1), 64, u16::MAX);
        synth.note_on(VoiceGroupId(1), 67, u16::MAX);

        let mut first_frame = [0.0; 2];
        synth.render(&mut first_frame, 2);
        assert_eq!(first_frame, [0.0, 0.0]);
        assert!(synth
            .voices
            .iter()
            .filter(|voice| voice.active)
            .all(|voice| voice.attack_gain > 0.0 && voice.attack_gain < 1.0));

        let attack_frames = (ATTACK_SECONDS * 48_000.0) as usize;
        let mut remainder = vec![0.0; attack_frames - 1];
        synth.render(&mut remainder, 1);
        assert!(synth
            .voices
            .iter()
            .filter(|voice| voice.active)
            .all(|voice| voice.attack_gain >= 0.999));
    }

    #[test]
    fn middle_register_strike_excites_bright_upper_partials() {
        let mut synth = PianoSynth::<1>::new(PianoConfig::new(48_000)).unwrap();
        synth.note_on(VoiceGroupId(1), 60, u16::MAX);
        let voice = &synth.voices[0];
        assert!(voice.partial_amplitudes[3] > 0.20);
        assert!(voice.partial_amplitudes[4] > 0.15);
        assert!(voice.partial_amplitudes[5] > 0.12);
        assert!(synth.partial_decay[5] < synth.partial_decay[1]);
    }

    #[test]
    fn release_is_scoped_to_voice_group_even_for_repeated_pitch() {
        let mut synth = PianoSynth::<8>::new(PianoConfig::new(48_000)).unwrap();
        synth.note_on(VoiceGroupId(1), 60, 50_000);
        synth.note_on(VoiceGroupId(2), 60, 50_000);
        synth.release_group(VoiceGroupId(1));
        assert_eq!(synth.active_groups(), 2);
        assert!(synth
            .voices
            .iter()
            .any(|voice| voice.active && voice.group == VoiceGroupId(2) && !voice.released));
    }

    #[test]
    fn fixed_pool_reports_voice_steal() {
        let mut synth = PianoSynth::<1>::new(PianoConfig::new(48_000)).unwrap();
        assert_eq!(
            synth.note_on(VoiceGroupId(1), 60, 50_000),
            VoiceStart::Started
        );
        assert_eq!(
            synth.note_on(VoiceGroupId(2), 64, 50_000),
            VoiceStart::StoleOlderVoice
        );
    }
}

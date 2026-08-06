// Copyright (c) 2026 Michael Saunders
use crate::{PianoConfig, PianoSynth, Sampler, VoiceGroupId, VoiceStart};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

const SOURCE_RATE: u32 = 44_100;
const VELOCITIES: usize = 128;
const PITCHES: usize = 128;
const LAYERS_PER_VELOCITY: usize = 2;
const SILENT_LAYER: LayerRegion = LayerRegion {
    sample: u16::MAX,
    key_center: 60,
    release_seconds: 1,
    gain: 0.0,
};

#[derive(Clone, Copy, Debug)]
struct LayerRegion {
    sample: u16,
    key_center: u8,
    release_seconds: u8,
    gain: f32,
}

#[derive(Clone, Copy, Debug)]
struct VelocityRegion {
    layers: [LayerRegion; LAYERS_PER_VELOCITY],
}

const SILENT_REGION: VelocityRegion = VelocityRegion {
    layers: [SILENT_LAYER; LAYERS_PER_VELOCITY],
};

#[derive(Clone, Copy, Debug)]
struct GroupSettings {
    release_seconds: u8,
    fade_in: Option<(u8, u8)>,
    fade_out: Option<(u8, u8)>,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            release_seconds: 1,
            fade_in: None,
            fade_out: None,
        }
    }
}

impl GroupSettings {
    fn update(&mut self, line: &str) {
        if let Some(value) = opcode(line, "ampeg_release") {
            self.release_seconds =
                value.parse::<f32>().unwrap_or(1.0).round().clamp(1.0, 10.0) as u8;
        }
        if let Some(low) = opcode(line, "xfin_lovel").and_then(|value| value.parse().ok()) {
            self.fade_in.get_or_insert((0, 127)).0 = low;
        }
        if let Some(high) = opcode(line, "xfin_hivel").and_then(|value| value.parse().ok()) {
            self.fade_in.get_or_insert((0, 127)).1 = high;
        }
        if let Some(low) = opcode(line, "xfout_lovel").and_then(|value| value.parse().ok()) {
            self.fade_out.get_or_insert((0, 127)).0 = low;
        }
        if let Some(high) = opcode(line, "xfout_hivel").and_then(|value| value.parse().ok()) {
            self.fade_out.get_or_insert((0, 127)).1 = high;
        }
    }

    fn gain_at(self, velocity: u8) -> f32 {
        fn rising(value: u8, low: u8, high: u8) -> f32 {
            if value <= low {
                0.0
            } else if value >= high || high <= low {
                1.0
            } else {
                f32::from(value - low) / f32::from(high - low)
            }
        }
        let mut gain = 1.0;
        if let Some((low, high)) = self.fade_in {
            gain *= rising(velocity, low, high);
        }
        if let Some((low, high)) = self.fade_out {
            gain *= 1.0 - rising(velocity, low, high);
        }
        gain
    }
}

#[derive(Debug)]
struct PcmSample {
    bytes: Box<[u8]>,
    data_offset: usize,
    frames: usize,
    channels: usize,
}

impl PcmSample {
    #[inline]
    fn value(&self, frame: usize, channel: usize) -> f32 {
        let channel = channel.min(self.channels - 1);
        let offset = self.data_offset + (frame * self.channels + channel) * 2;
        i16::from_le_bytes([self.bytes[offset], self.bytes[offset + 1]]) as f32 / 32_768.0
    }
}

/// Fully decoded/indexed Slender Salamander note bank. WAV bytes remain 16-bit
/// PCM and the phase-aligned velocity crossfades are compiled into lookup
/// tables before the audio callback starts.
#[derive(Debug)]
pub struct SalamanderBank {
    samples: Vec<PcmSample>,
    regions: Box<[VelocityRegion; PITCHES * VELOCITIES]>,
    pcm_bytes: u64,
}

impl SalamanderBank {
    pub fn load(directory: impl AsRef<Path>) -> Result<Self, SalamanderLoadError> {
        let directory = directory.as_ref();
        let sfz_path = directory.join("SlenderSalamanderGrandPiano.sfz");
        let sfz = fs::read_to_string(&sfz_path)
            .map_err(|error| SalamanderLoadError::io(&sfz_path, error))?;
        let mut regions = Box::new([SILENT_REGION; PITCHES * VELOCITIES]);
        let mut samples = Vec::with_capacity(90);
        let mut sample_indices = HashMap::<PathBuf, u16>::with_capacity(90);
        let mut group = GroupSettings::default();
        let mut default_path = PathBuf::new();

        for line in sfz.lines() {
            let line = line.trim();
            if line.starts_with("//Release string resonances") {
                break;
            }
            if line.starts_with("<group>") {
                group = GroupSettings::default();
                group.update(line);
                continue;
            }
            if !line.starts_with("<region>") {
                if let Some(path) = opcode(line, "default_path") {
                    default_path = PathBuf::from(path.replace('\\', "/"));
                }
                group.update(line);
                continue;
            }

            let relative = opcode(line, "sample").ok_or_else(|| {
                SalamanderLoadError::InvalidSfz("note region has no sample opcode".to_owned())
            })?;
            let sample_path = directory
                .join(&default_path)
                .join(relative.replace('\\', "/"));
            let sample_index = if let Some(index) = sample_indices.get(&sample_path) {
                *index
            } else {
                let index = u16::try_from(samples.len())
                    .map_err(|_| SalamanderLoadError::InvalidSfz("too many samples".to_owned()))?;
                samples.push(load_pcm16_wave(&sample_path)?);
                sample_indices.insert(sample_path, index);
                index
            };
            let low_key = parse_key(line, "lokey", 0)?;
            let high_key = parse_key(line, "hikey", 127)?;
            let low_velocity = parse_u8(line, "lovel", 1)?;
            let high_velocity = parse_u8(line, "hivel", 127)?;
            let key_center = parse_key(line, "pitch_keycenter", 60)?;
            for pitch in low_key..=high_key {
                for velocity in low_velocity..=high_velocity {
                    let gain = group.gain_at(velocity);
                    if gain <= 0.0 {
                        continue;
                    }
                    let region = &mut regions[pitch as usize * VELOCITIES + velocity as usize];
                    let slot = region
                        .layers
                        .iter_mut()
                        .find(|layer| layer.sample == u16::MAX)
                        .ok_or_else(|| {
                            SalamanderLoadError::InvalidSfz(format!(
                                "more than {LAYERS_PER_VELOCITY} layers at pitch {pitch}, velocity {velocity}"
                            ))
                        })?;
                    *slot = LayerRegion {
                        sample: sample_index,
                        key_center,
                        release_seconds: group.release_seconds,
                        gain,
                    };
                }
            }
        }

        for pitch in 21usize..=108 {
            if regions[pitch * VELOCITIES + 64].layers[0].sample == u16::MAX {
                return Err(SalamanderLoadError::InvalidSfz(format!(
                    "no playable region for MIDI pitch {pitch}"
                )));
            }
        }
        let pcm_bytes = samples
            .iter()
            .map(|sample| (sample.frames * sample.channels * 2) as u64)
            .sum();
        Ok(Self {
            samples,
            regions,
            pcm_bytes,
        })
    }

    pub const fn source_sample_rate(&self) -> u32 {
        SOURCE_RATE
    }

    pub const fn pcm_bytes(&self) -> u64 {
        self.pcm_bytes
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    fn region(&self, pitch: u8, velocity: u8) -> Option<VelocityRegion> {
        let pitch = pitch.clamp(21, 108) as usize;
        let velocity = velocity.max(1) as usize;
        let region = self.regions[pitch * VELOCITIES + velocity];
        (region.layers[0].sample != u16::MAX).then_some(region)
    }
}

fn opcode<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.split_ascii_whitespace()
        .find_map(|piece| piece.strip_prefix(name)?.strip_prefix('='))
}

fn parse_u8(line: &str, name: &str, default: u8) -> Result<u8, SalamanderLoadError> {
    opcode(line, name).map_or(Ok(default), |value| {
        value
            .parse::<u8>()
            .map_err(|_| SalamanderLoadError::InvalidSfz(format!("invalid {name} value `{value}`")))
    })
}

fn parse_key(line: &str, name: &str, default: u8) -> Result<u8, SalamanderLoadError> {
    let Some(value) = opcode(line, name) else {
        return Ok(default);
    };
    if let Ok(number) = value.parse::<u8>() {
        return Ok(number);
    }
    let split = value
        .char_indices()
        .find(|(_, character)| character.is_ascii_digit() || *character == '-')
        .map(|(index, _)| index)
        .ok_or_else(|| {
            SalamanderLoadError::InvalidSfz(format!("invalid {name} value `{value}`"))
        })?;
    let (note, octave) = value.split_at(split);
    let semitone = match note {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        _ => {
            return Err(SalamanderLoadError::InvalidSfz(format!(
                "invalid {name} value `{value}`"
            )));
        }
    };
    let octave = octave
        .parse::<i16>()
        .map_err(|_| SalamanderLoadError::InvalidSfz(format!("invalid {name} value `{value}`")))?;
    u8::try_from((octave + 1) * 12 + semitone).map_err(|_| {
        SalamanderLoadError::InvalidSfz(format!("out-of-range {name} value `{value}`"))
    })
}

fn load_pcm16_wave(path: &Path) -> Result<PcmSample, SalamanderLoadError> {
    let bytes = fs::read(path).map_err(|error| SalamanderLoadError::io(path, error))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SalamanderLoadError::InvalidWave(
            path.to_path_buf(),
            "not a RIFF/WAVE file",
        ));
    }
    let mut cursor = 12usize;
    let mut format = None;
    let mut data = None;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let start = cursor + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| {
                SalamanderLoadError::InvalidWave(path.to_path_buf(), "truncated chunk")
            })?;
        if id == b"fmt " && size >= 16 {
            let encoding = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
            let channels =
                u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()) as usize;
            let rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
            let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
            format = Some((encoding, channels, rate, bits));
        } else if id == b"data" {
            data = Some((start, size));
        }
        cursor = end + (size & 1);
    }
    let (encoding, channels, rate, bits) = format
        .ok_or_else(|| SalamanderLoadError::InvalidWave(path.to_path_buf(), "missing fmt chunk"))?;
    let (data_offset, data_bytes) = data.ok_or_else(|| {
        SalamanderLoadError::InvalidWave(path.to_path_buf(), "missing data chunk")
    })?;
    if encoding != 1 || !(channels == 1 || channels == 2) || rate != SOURCE_RATE || bits != 16 {
        return Err(SalamanderLoadError::InvalidWave(
            path.to_path_buf(),
            "expected 44.1 kHz, 16-bit PCM, mono or stereo",
        ));
    }
    let frame_bytes = channels * 2;
    if data_bytes == 0 || data_bytes % frame_bytes != 0 {
        return Err(SalamanderLoadError::InvalidWave(
            path.to_path_buf(),
            "invalid PCM data length",
        ));
    }
    Ok(PcmSample {
        bytes: bytes.into_boxed_slice(),
        data_offset,
        frames: data_bytes / frame_bytes,
        channels,
    })
}

#[derive(Clone, Copy, Debug)]
struct Voice {
    active: bool,
    released: bool,
    group: VoiceGroupId,
    sample: u16,
    position: f64,
    step: f64,
    gain: f32,
    release_decay: f32,
    age: u64,
}

impl Voice {
    const SILENT: Self = Self {
        active: false,
        released: false,
        group: VoiceGroupId(0),
        sample: 0,
        position: 0.0,
        step: 1.0,
        gain: 0.0,
        release_decay: 1.0,
        age: 0,
    };
}

/// Allocation-free, memory-resident player for Slender Salamander Grand Piano.
pub struct SampledPiano<const VOICES: usize = 128> {
    bank: Arc<SalamanderBank>,
    voices: [Voice; VOICES],
    pitch_steps: Box<[[f64; PITCHES]; PITCHES]>,
    release_decays: [f32; 11],
    output_gain: f32,
    next_age: u64,
}

impl<const VOICES: usize> SampledPiano<VOICES> {
    pub fn new(bank: Arc<SalamanderBank>, output_rate: u32) -> Result<Self, SalamanderLoadError> {
        if output_rate == 0 || VOICES == 0 {
            return Err(SalamanderLoadError::InvalidConfiguration);
        }
        let mut pitch_steps = Box::new([[0.0; PITCHES]; PITCHES]);
        for pitch in 0..PITCHES {
            for key_center in 0..PITCHES {
                pitch_steps[pitch][key_center] = 2.0_f64
                    .powf((pitch as f64 - key_center as f64) / 12.0)
                    * f64::from(SOURCE_RATE)
                    / f64::from(output_rate);
            }
        }
        let mut release_decays = [1.0; 11];
        for (seconds, decay) in release_decays.iter_mut().enumerate().skip(1) {
            let release_frames = output_rate as f32 * seconds as f32;
            *decay = 0.001_f32.powf(1.0 / release_frames);
        }
        Ok(Self {
            bank,
            voices: [Voice::SILENT; VOICES],
            pitch_steps,
            release_decays,
            output_gain: 0.24,
            next_age: 1,
        })
    }

    fn allocate_voice(&self) -> (usize, bool) {
        if let Some(index) = self.voices.iter().position(|voice| !voice.active) {
            return (index, false);
        }
        let mut best = 0;
        for index in 1..VOICES {
            let candidate = self.voices[index];
            let current = self.voices[best];
            if (candidate.released && !current.released)
                || (candidate.released == current.released
                    && (candidate.gain < current.gain
                        || (candidate.gain == current.gain && candidate.age < current.age)))
            {
                best = index;
            }
        }
        (best, true)
    }
}

impl<const VOICES: usize> Sampler for SampledPiano<VOICES> {
    fn note_on(&mut self, group: VoiceGroupId, pitch: u8, velocity: u16) -> VoiceStart {
        if pitch > 127 || velocity == 0 {
            return VoiceStart::Rejected;
        }
        let midi_velocity = ((u32::from(velocity) * 127 + 32_767) / 65_535).clamp(1, 127) as u8;
        let Some(region) = self.bank.region(pitch, midi_velocity) else {
            return VoiceStart::Rejected;
        };
        let velocity_gain = 0.55 + 0.45 * (f32::from(midi_velocity) / 127.0);
        let mut started = false;
        let mut stole_any = false;
        for layer in region
            .layers
            .into_iter()
            .filter(|layer| layer.sample != u16::MAX && layer.gain > 0.0)
        {
            let (index, stole) = self.allocate_voice();
            self.voices[index] = Voice {
                active: true,
                released: false,
                group,
                sample: layer.sample,
                position: 0.0,
                step: self.pitch_steps[pitch as usize][layer.key_center as usize],
                gain: velocity_gain * layer.gain,
                release_decay: self.release_decays[layer.release_seconds as usize],
                age: self.next_age,
            };
            self.next_age = self.next_age.wrapping_add(1);
            started = true;
            stole_any |= stole;
        }
        if stole_any {
            VoiceStart::StoleOlderVoice
        } else if started {
            VoiceStart::Started
        } else {
            VoiceStart::Rejected
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
            let mut left = 0.0;
            let mut right = 0.0;
            for voice in &mut self.voices {
                if !voice.active {
                    continue;
                }
                let sample = &self.bank.samples[voice.sample as usize];
                let first = voice.position as usize;
                if first + 1 >= sample.frames {
                    *voice = Voice::SILENT;
                    continue;
                }
                let fraction = (voice.position - first as f64) as f32;
                let l0 = sample.value(first, 0);
                let l1 = sample.value(first + 1, 0);
                let r0 = sample.value(first, sample.channels - 1);
                let r1 = sample.value(first + 1, sample.channels - 1);
                left += (l0 + (l1 - l0) * fraction) * voice.gain;
                right += (r0 + (r1 - r0) * fraction) * voice.gain;
                voice.position += voice.step;
                if voice.released {
                    voice.gain *= voice.release_decay;
                    if voice.gain < 0.0005 {
                        *voice = Voice::SILENT;
                    }
                }
            }
            let left = (left * self.output_gain).clamp(-1.0, 1.0);
            let right = (right * self.output_gain).clamp(-1.0, 1.0);
            if channels == 1 {
                frame[0] += (left + right) * 0.5;
            } else {
                frame[0] += left;
                frame[1] += right;
                for channel in &mut frame[2..] {
                    *channel += (left + right) * 0.5;
                }
            }
        }
    }

    fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }
}

/// Single concrete sampler type lets the native backend use Salamander when
/// available while retaining the dependency-free recovery instrument.
// The procedural variant intentionally owns its fixed voice pool inline. It is
// constructed outside the realtime callback, and avoiding a conditional heap
// indirection keeps both sampler variants predictable inside render().
#[allow(clippy::large_enum_variant)]
pub enum PianoInstrument<const VOICES: usize = 128> {
    Salamander(SampledPiano<VOICES>),
    Procedural(PianoSynth<VOICES>),
}

impl<const VOICES: usize> PianoInstrument<VOICES> {
    pub fn new(
        bank: Option<Arc<SalamanderBank>>,
        sample_rate: u32,
    ) -> Result<Self, SalamanderLoadError> {
        match bank {
            Some(bank) => Ok(Self::Salamander(SampledPiano::new(bank, sample_rate)?)),
            None => PianoSynth::new(PianoConfig::new(sample_rate))
                .map(Self::Procedural)
                .map_err(|_| SalamanderLoadError::InvalidConfiguration),
        }
    }
}

impl<const VOICES: usize> Sampler for PianoInstrument<VOICES> {
    fn note_on(&mut self, group: VoiceGroupId, pitch: u8, velocity: u16) -> VoiceStart {
        match self {
            Self::Salamander(piano) => piano.note_on(group, pitch, velocity),
            Self::Procedural(piano) => piano.note_on(group, pitch, velocity),
        }
    }
    fn release_group(&mut self, group: VoiceGroupId) {
        match self {
            Self::Salamander(piano) => piano.release_group(group),
            Self::Procedural(piano) => piano.release_group(group),
        }
    }
    fn panic(&mut self) {
        match self {
            Self::Salamander(piano) => piano.panic(),
            Self::Procedural(piano) => piano.panic(),
        }
    }
    fn render(&mut self, output: &mut [f32], channels: usize) {
        match self {
            Self::Salamander(piano) => piano.render(output, channels),
            Self::Procedural(piano) => piano.render(output, channels),
        }
    }
    fn active_voice_count(&self) -> usize {
        match self {
            Self::Salamander(piano) => piano.active_voice_count(),
            Self::Procedural(piano) => piano.active_voice_count(),
        }
    }
}

#[derive(Debug)]
pub enum SalamanderLoadError {
    Io(PathBuf, std::io::Error),
    InvalidSfz(String),
    InvalidWave(PathBuf, &'static str),
    InvalidConfiguration,
}

impl SalamanderLoadError {
    fn io(path: &Path, error: std::io::Error) -> Self {
        Self::Io(path.to_path_buf(), error)
    }
}

impl std::fmt::Display for SalamanderLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(path, error) => {
                write!(formatter, "unable to load {}: {error}", path.display())
            }
            Self::InvalidSfz(message) => write!(formatter, "invalid Salamander SFZ: {message}"),
            Self::InvalidWave(path, message) => {
                write!(formatter, "invalid WAV {}: {message}", path.display())
            }
            Self::InvalidConfiguration => write!(formatter, "invalid sampled-piano configuration"),
        }
    }
}

impl std::error::Error for SalamanderLoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcodes_are_read_without_confusing_prefixes() {
        let line = "<region> sample=audio\\C4v8.wav lokey=59 hikey=61 lovel=57 hivel=64";
        assert_eq!(opcode(line, "sample"), Some("audio\\C4v8.wav"));
        assert_eq!(opcode(line, "hikey"), Some("61"));
        assert_eq!(opcode(line, "key"), None);
    }

    #[test]
    #[ignore = "loads the 238 MB licensed sample asset"]
    fn bundled_salamander_bank_loads_and_renders() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("assets/SlenderSalamander44khz16bit");
        let bank = Arc::new(SalamanderBank::load(directory).unwrap());
        assert_eq!(bank.sample_count(), 90);
        assert!(bank.pcm_bytes() > 200_000_000);
        assert!(bank.pcm_bytes() < 220_000_000);
        let mut piano = SampledPiano::<16>::new(bank, 44_100).unwrap();
        assert_eq!(
            piano.note_on(VoiceGroupId(1), 60, u16::MAX),
            VoiceStart::Started
        );
        let mut output = [0.0; 512];
        piano.render(&mut output, 2);
        assert!(output.iter().any(|sample| *sample != 0.0));

        piano.panic();
        piano.note_on(VoiceGroupId(1), 60, 15_500);
        assert_eq!(piano.active_voice_count(), 2);
        piano.panic();

        for (index, pitch) in [48, 52, 55, 60, 64, 67, 72, 76, 79, 84]
            .into_iter()
            .enumerate()
        {
            piano.note_on(VoiceGroupId(index as u64 + 2), pitch, 52_000);
        }
        let mut dense_output = vec![0.0; 44_100 * 2 * 5];
        let started = std::time::Instant::now();
        piano.render(&mut dense_output, 2);
        let elapsed = started.elapsed();
        eprintln!("rendered five seconds of an 11-note texture in {elapsed:?}");
        assert!(elapsed.as_secs_f32() < 5.0);
    }

    #[test]
    fn parses_named_sfz_keys_and_crossfade_gains() {
        assert_eq!(parse_key("lokey=C4", "lokey", 0).unwrap(), 60);
        assert_eq!(parse_key("lokey=A0", "lokey", 0).unwrap(), 21);
        assert_eq!(parse_key("lokey=C-1", "lokey", 0).unwrap(), 0);
        let settings = GroupSettings {
            release_seconds: 1,
            fade_in: Some((8, 59)),
            fade_out: None,
        };
        assert_eq!(settings.gain_at(8), 0.0);
        assert_eq!(settings.gain_at(59), 1.0);
        assert!(settings.gain_at(30) > 0.0 && settings.gain_at(30) < 1.0);
    }
}

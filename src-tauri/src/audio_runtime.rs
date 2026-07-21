use crate::dto::{DeviceDto, DiagnosticsDto, WasapiPeriodsDto};
use std::sync::Arc;
use tapconductor_audio::{
    AudioCommand, AudioCommandSender, AudioDiagnostics, Chord, Note, PianoConfig, PianoSynth,
    VoiceGroupId, audio_engine,
    backend::{AudioBackend, RunningAudioStream},
};
use tapconductor_performance as performance;

#[cfg(not(windows))]
use tapconductor_audio::backend::CpalBackend as PlatformAudioBackend;
#[cfg(windows)]
use tapconductor_audio::backend::WasapiLowLatencyBackend as PlatformAudioBackend;

const DEFAULT_SAMPLE_RATE: u32 = 48_000;
const REQUESTED_BUFFER_FRAMES: u32 = 128;
const MAX_ACCEPTED_BUFFER_FRAMES: u32 = 256;
const COMMAND_QUEUE: usize = 2_048;
const SCHEDULE_CAPACITY: usize = 2_048;
const VOICES: usize = 256;

struct AudioRuntime {
    sender: AudioCommandSender<COMMAND_QUEUE>,
    diagnostics: Arc<AudioDiagnostics>,
    _stream: Box<dyn RunningAudioStream>,
}

pub struct AudioManager {
    backend: PlatformAudioBackend,
    runtime: Option<AudioRuntime>,
    selected_device: Option<String>,
    selected_device_name: String,
    last_error: Option<String>,
    /// Total frames rendered by streams that were replaced. Performance time
    /// never moves backwards when an output device is rebuilt.
    clock_epoch: u64,
    master_gain: f32,
    wasapi_periods: Option<WasapiPeriodsDto>,
    sample_rate: u32,
}

impl AudioManager {
    pub fn new() -> Self {
        let mut manager = Self {
            backend: PlatformAudioBackend::default(),
            runtime: None,
            selected_device: None,
            selected_device_name: "System default".to_owned(),
            last_error: None,
            clock_epoch: 0,
            master_gain: 1.0,
            wasapi_periods: None,
            // This is used only if endpoint discovery fails. A successful
            // restart immediately replaces it with the device's native rate.
            sample_rate: DEFAULT_SAMPLE_RATE,
        };
        if let Err(error) = manager.restart(None) {
            manager.last_error = Some(error);
        }
        manager
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn now_sample(&self) -> u64 {
        self.clock_epoch.saturating_add(
            self.runtime
                .as_ref()
                .map(|runtime| runtime.diagnostics.snapshot().rendered_frames)
                .unwrap_or(0),
        )
    }

    pub fn ensure_ready(&self) -> Result<(), String> {
        match self.runtime.as_ref() {
            Some(runtime) if runtime.diagnostics.snapshot().backend_errors == 0 => Ok(()),
            Some(_) => Err(
                "The audio backend reported an output-stream error; reselect the device before performing."
                    .to_owned(),
            ),
            None => Err(self
                .last_error
                .clone()
                .unwrap_or_else(|| "Audio is not ready.".to_owned())),
        }
    }

    pub fn devices(&self) -> Result<Vec<DeviceDto>, String> {
        self.backend
            .output_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|device| DeviceDto {
                        id: device.id,
                        name: device.name,
                        is_default: device.is_default,
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    pub fn restart(&mut self, selected_device: Option<String>) -> Result<(), String> {
        let result = self.try_restart(selected_device);
        if let Err(error) = &result {
            self.last_error = Some(error.clone());
        }
        result
    }

    fn try_restart(&mut self, selected_device: Option<String>) -> Result<(), String> {
        let selected_name = self
            .backend
            .output_devices()
            .ok()
            .and_then(|devices| {
                selected_device
                    .as_deref()
                    .and_then(|id| devices.iter().find(|device| device.id == id))
                    .or_else(|| devices.iter().find(|device| device.is_default))
                    .map(|device| device.name.clone())
            })
            .unwrap_or_else(|| "System default".to_owned());

        let mut output_config = self
            .backend
            .preferred_output_config(selected_device.as_deref())
            .map_err(|error| error.to_string())?;
        let sample_rate = output_config.sample_rate;
        if output_config.buffer_frames.is_none() {
            output_config.buffer_frames = Some(REQUESTED_BUFFER_FRAMES);
        }
        if output_config
            .buffer_frames
            .is_some_and(|frames| frames > MAX_ACCEPTED_BUFFER_FRAMES)
        {
            return Err(format!(
                "The selected output's smallest advertised buffer is {} frames; TapConductor requires at most {MAX_ACCEPTED_BUFFER_FRAMES} frames for live use.",
                output_config.buffer_frames.expect("checked as Some")
            ));
        }

        let wasapi_periods = self.probe_periods(selected_device.as_deref());
        let sampler = PianoSynth::<VOICES>::new(PianoConfig::new(sample_rate))
            .map_err(|error| error.to_string())?;
        let (mut sender, engine, diagnostics) = audio_engine::<_, COMMAND_QUEUE, SCHEDULE_CAPACITY>(
            sampler,
            sample_rate,
            output_config.channels,
        );
        sender
            .try_send(AudioCommand::SetMasterGain {
                gain: self.master_gain,
                at: 0,
            })
            .map_err(|_| "Unable to initialize the audio gain command.".to_owned())?;
        let stream = self
            .backend
            .start_output(&output_config, Box::new(engine))
            .map_err(|error| error.to_string())?;

        // The old and newly warmed streams may overlap during setup. Anchor
        // the new stream at the exact handoff so the performance clock neither
        // jumps backward nor double-counts those parallel render frames.
        let new_rendered_frames = diagnostics.snapshot().rendered_frames;
        let handoff_sample = self.now_sample();
        self.runtime = Some(AudioRuntime {
            sender,
            diagnostics,
            _stream: stream,
        });
        self.clock_epoch = handoff_sample.saturating_sub(new_rendered_frames);
        self.selected_device = selected_device;
        self.selected_device_name = selected_name;
        self.wasapi_periods = wasapi_periods;
        self.sample_rate = sample_rate;
        self.last_error = None;
        Ok(())
    }

    #[cfg(windows)]
    fn probe_periods(&self, selected_device: Option<&str>) -> Option<WasapiPeriodsDto> {
        if selected_device.is_some() {
            return None;
        }
        self.backend
            .probe_default_periods()
            .ok()
            .map(|periods| WasapiPeriodsDto {
                sample_rate: periods.sample_rate,
                channels: periods.channels,
                default_frames: periods.default_frames,
                fundamental_frames: periods.fundamental_frames,
                minimum_frames: periods.minimum_frames,
                maximum_frames: periods.maximum_frames,
            })
    }

    #[cfg(not(windows))]
    fn probe_periods(&self, _selected_device: Option<&str>) -> Option<WasapiPeriodsDto> {
        None
    }

    pub fn send_performance_command(
        &mut self,
        command: performance::AudioCommand,
    ) -> Result<(), String> {
        let local_time = |absolute: u64| absolute.saturating_sub(self.clock_epoch);
        if let performance::AudioCommand::Panic { at, .. } = command {
            // A missing stream is already silent. Treat safety-stop commands
            // as delivered so score import/display is independent of audio
            // endpoint availability.
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.sender.panic_at(local_time(at.frame()));
            }
            return Ok(());
        }
        let translated = match command {
            performance::AudioCommand::PlaySlice {
                at,
                group,
                chord,
                velocity,
            } => {
                let mut audio_chord = Chord::empty();
                for pitch in chord.pitches() {
                    audio_chord
                        .push(Note::new(pitch.get(), velocity.get()))
                        .map_err(|error| error.to_string())?;
                }
                AudioCommand::PlaySlice {
                    at: local_time(at.frame()),
                    group: VoiceGroupId(group.get()),
                    chord: audio_chord,
                }
            }
            performance::AudioCommand::ReleaseGroup { at, group } => AudioCommand::ReleaseGroup {
                at: local_time(at.frame()),
                group: VoiceGroupId(group.get()),
            },
            performance::AudioCommand::Panic { .. } => unreachable!("panic handled above"),
        };
        let runtime = self.runtime.as_mut().ok_or_else(|| {
            self.last_error
                .clone()
                .unwrap_or_else(|| "Audio is not ready.".to_owned())
        })?;
        runtime
            .sender
            .try_send(translated)
            .map_err(|_| "The real-time audio command queue is full.".to_owned())
    }

    pub fn set_volume(&mut self, gain: f32) -> Result<(), String> {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 2.0)
        } else {
            return Err("Volume must be a finite number.".to_owned());
        };
        let at = self.now_sample().saturating_sub(self.clock_epoch);
        let runtime = self
            .runtime
            .as_mut()
            .ok_or_else(|| "Audio is not ready.".to_owned())?;
        runtime
            .sender
            .try_send(AudioCommand::SetMasterGain { gain, at })
            .map_err(|_| "The real-time audio command queue is full.".to_owned())?;
        self.master_gain = gain;
        Ok(())
    }

    pub fn diagnostics(
        &self,
        midi_input: Option<String>,
        midi_output: Option<String>,
        midi_output_error: Option<String>,
    ) -> DiagnosticsDto {
        let snapshot = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.diagnostics.snapshot())
            .unwrap_or_default();
        let estimated_latency_ms = if snapshot.sample_rate == 0 {
            0.0
        } else {
            f64::from(snapshot.estimated_output_latency_frames) * 1_000.0
                / f64::from(snapshot.sample_rate)
        };
        DiagnosticsDto {
            audio_backend: self.backend_label().to_owned(),
            output_device: self.selected_device_name.clone(),
            sample_rate: snapshot.sample_rate,
            buffer_frames: snapshot.latest_buffer_frames,
            estimated_latency_ms,
            callback_underruns: 0,
            backend_errors: snapshot.backend_errors,
            late_commands: snapshot.late_commands,
            invalid_audio_buffers: snapshot.invalid_buffers,
            voice_steals: snapshot.voice_steals,
            queue_overflows: snapshot.queue_overflows + snapshot.schedule_overflows,
            active_voices: snapshot.active_voices,
            direct_wasapi_stream: self.uses_direct_wasapi_stream(),
            wasapi_periods: self.wasapi_periods,
            midi_input,
            midi_output,
            midi_output_error,
            ready: self.runtime.is_some() && snapshot.backend_errors == 0,
            message: self.last_error.clone().or_else(|| {
                (snapshot.backend_errors > 0).then(|| {
                    "The audio backend reported an output-stream error; reselect the device before performing."
                        .to_owned()
                })
            }),
        }
    }

    #[cfg(windows)]
    fn backend_label(&self) -> &'static str {
        self.backend.backend_label()
    }

    #[cfg(not(windows))]
    fn backend_label(&self) -> &'static str {
        "CPAL platform audio"
    }

    #[cfg(windows)]
    fn uses_direct_wasapi_stream(&self) -> bool {
        self.backend.uses_direct_stream()
    }

    #[cfg(not(windows))]
    fn uses_direct_wasapi_stream(&self) -> bool {
        false
    }
}

impl Default for AudioManager {
    fn default() -> Self {
        Self::new()
    }
}

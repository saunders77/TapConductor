//! Platform output abstraction and an optional CPAL prototype backend.

use crate::AudioRenderCallback;
use core::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputStreamConfig {
    pub device_id: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub buffer_frames: Option<u32>,
}

pub trait RunningAudioStream: Send {
    fn pause(&self) -> Result<(), BackendError>;
    fn resume(&self) -> Result<(), BackendError>;
}

/// Backend methods run only during setup/recovery, never in the callback.
pub trait AudioBackend {
    fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError>;

    /// Returns a supported low-latency float configuration for the selected
    /// device. Callers should construct the sampler with this actual sample
    /// rate rather than assuming 48 kHz.
    fn preferred_output_config(
        &self,
        device_id: Option<&str>,
    ) -> Result<OutputStreamConfig, BackendError>;

    fn start_output(
        &self,
        config: &OutputStreamConfig,
        renderer: Box<dyn AudioRenderCallback>,
    ) -> Result<Box<dyn RunningAudioStream>, BackendError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendError {
    pub operation: &'static str,
    pub detail: String,
}

impl BackendError {
    pub fn new(operation: &'static str, detail: impl Into<String>) -> Self {
        Self {
            operation,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "audio {} failed: {}", self.operation, self.detail)
    }
}

impl std::error::Error for BackendError {}

#[cfg(feature = "cpal-backend")]
mod cpal_impl {
    use super::{
        AudioBackend, AudioRenderCallback, BackendError, OutputDeviceInfo, OutputStreamConfig,
        RunningAudioStream,
    };
    use crate::RenderCallbackInfo;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
    use std::thread::{self, JoinHandle};

    #[derive(Clone, Copy, Debug, Default)]
    pub struct CpalBackend;

    type SupportedF32Config = (u32, u16, Option<(u32, u32)>);

    impl CpalBackend {
        fn select_device(&self, id: Option<&str>) -> Result<cpal::Device, BackendError> {
            select_device(&cpal::default_host(), id)
        }

        fn supported_f32_config(
            &self,
            device: &cpal::Device,
            requested_rate: Option<u32>,
            requested_channels: Option<u16>,
        ) -> Result<SupportedF32Config, BackendError> {
            supported_f32_config(device, requested_rate, requested_channels)
        }
    }

    fn select_device(host: &cpal::Host, id: Option<&str>) -> Result<cpal::Device, BackendError> {
        if let Some(id) = id {
            for (index, device) in host
                .output_devices()
                .map_err(|error| BackendError::new("device enumeration", error.to_string()))?
                .enumerate()
            {
                let name = device.name().unwrap_or_else(|_| "Unknown output".into());
                if make_id(index, &name) == id {
                    return Ok(device);
                }
            }
            return Err(BackendError::new(
                "device selection",
                "output device is unavailable",
            ));
        }
        host.default_output_device()
            .ok_or_else(|| BackendError::new("device selection", "no default output device"))
    }

    fn supported_f32_config(
        device: &cpal::Device,
        requested_rate: Option<u32>,
        requested_channels: Option<u16>,
    ) -> Result<SupportedF32Config, BackendError> {
        let ranges = device
            .supported_output_configs()
            .map_err(|error| BackendError::new("format query", error.to_string()))?;
        let mut best = None;
        for range in ranges {
            if range.sample_format() != cpal::SampleFormat::F32 {
                continue;
            }
            if requested_channels.is_some_and(|channels| channels != range.channels()) {
                continue;
            }
            let minimum_rate = range.min_sample_rate().0;
            let maximum_rate = range.max_sample_rate().0;
            let rate = requested_rate
                .unwrap_or(48_000)
                .clamp(minimum_rate, maximum_rate);
            let exact_rate = requested_rate.is_none_or(|requested| requested == rate);
            let rate_distance = rate.abs_diff(requested_rate.unwrap_or(48_000));
            let buffer_range = match range.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                cpal::SupportedBufferSize::Unknown => None,
            };
            let candidate = (rate, range.channels(), buffer_range);
            let score = (
                !exact_rate,
                requested_channels.is_none_or(|channels| channels != range.channels()),
                rate_distance,
            );
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score < *best_score)
            {
                best = Some((score, candidate));
            }
        }
        best.map(|(_, candidate)| candidate).ok_or_else(|| {
            BackendError::new(
                "format negotiation",
                "device exposes no compatible 32-bit float output stream",
            )
        })
    }

    fn make_id(index: usize, name: &str) -> String {
        format!("cpal:{index}:{name}")
    }

    impl AudioBackend for CpalBackend {
        fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError> {
            let host = cpal::default_host();
            let default_name = host
                .default_output_device()
                .and_then(|device| device.name().ok());
            let devices = host
                .output_devices()
                .map_err(|error| BackendError::new("device enumeration", error.to_string()))?;
            let mut result = Vec::new();
            for (index, device) in devices.enumerate() {
                let name = device.name().unwrap_or_else(|_| "Unknown output".into());
                result.push(OutputDeviceInfo {
                    id: make_id(index, &name),
                    is_default: default_name.as_deref() == Some(name.as_str()),
                    name,
                });
            }
            Ok(result)
        }

        fn preferred_output_config(
            &self,
            device_id: Option<&str>,
        ) -> Result<OutputStreamConfig, BackendError> {
            let device = self.select_device(device_id)?;
            let default = device.default_output_config().ok();
            let requested_rate = default.as_ref().map(|config| config.sample_rate().0);
            let requested_channels = default.as_ref().map(|config| config.channels());
            let (sample_rate, channels, buffer_range) =
                self.supported_f32_config(&device, requested_rate, requested_channels)?;
            let buffer_frames =
                buffer_range.map(|(minimum, maximum)| 128_u32.clamp(minimum, maximum));
            Ok(OutputStreamConfig {
                device_id: device_id.map(str::to_owned),
                sample_rate,
                channels,
                buffer_frames,
            })
        }

        fn start_output(
            &self,
            config: &OutputStreamConfig,
            renderer: Box<dyn AudioRenderCallback>,
        ) -> Result<Box<dyn RunningAudioStream>, BackendError> {
            if config.channels == 0 || config.sample_rate == 0 {
                return Err(BackendError::new(
                    "stream setup",
                    "sample rate and channel count must be non-zero",
                ));
            }
            let config = config.clone();
            let (startup_tx, startup_rx) = sync_channel(1);
            let (control_tx, control_rx) = sync_channel(8);
            let thread = thread::Builder::new()
                .name("tapconductor-cpal-owner".into())
                .spawn(move || run_stream_thread(config, renderer, startup_tx, control_rx))
                .map_err(|error| BackendError::new("stream thread", error.to_string()))?;
            match startup_rx.recv() {
                Ok(Ok(())) => Ok(Box::new(CpalRunningStream {
                    control: control_tx,
                    thread: Some(thread),
                })),
                Ok(Err(error)) => {
                    let _ = thread.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = thread.join();
                    Err(BackendError::new(
                        "stream thread",
                        "stream owner exited during startup",
                    ))
                }
            }
        }
    }

    fn build_stream(
        config: &OutputStreamConfig,
        mut renderer: Box<dyn AudioRenderCallback>,
    ) -> Result<cpal::Stream, BackendError> {
        let host = cpal::default_host();
        let device = select_device(&host, config.device_id.as_deref())?;
        let (sample_rate, channels, buffer_range) =
            supported_f32_config(&device, Some(config.sample_rate), Some(config.channels))?;
        if sample_rate != config.sample_rate || channels != config.channels {
            return Err(BackendError::new(
                "stream setup",
                "requested sample rate/channel count is not supported as float output",
            ));
        }
        if let (Some(requested), Some((minimum, maximum))) = (config.buffer_frames, buffer_range) {
            if requested < minimum || requested > maximum {
                return Err(BackendError::new(
                    "stream setup",
                    format!(
                        "requested {requested}-frame buffer is outside supported {minimum}..={maximum}"
                    ),
                ));
            }
        }

        let stream_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: config
                .buffer_frames
                .map(cpal::BufferSize::Fixed)
                .unwrap_or(cpal::BufferSize::Default),
        };
        let channels = config.channels as usize;
        let diagnostics = renderer.audio_diagnostics();
        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    let _ = renderer.render_audio(
                        data,
                        RenderCallbackInfo {
                            estimated_output_latency_frames: Some(
                                (data.len() / channels).min(u32::MAX as usize) as u32,
                            ),
                        },
                    );
                },
                move |_error| {
                    if let Some(diagnostics) = diagnostics.as_ref() {
                        diagnostics.note_backend_error();
                    }
                    // Never perform logging or UI work on CPAL's error thread.
                },
                None,
            )
            .map_err(|error| BackendError::new("stream construction", error.to_string()))?;
        stream
            .play()
            .map_err(|error| BackendError::new("stream start", error.to_string()))?;
        Ok(stream)
    }

    enum StreamControl {
        Pause(SyncSender<Result<(), BackendError>>),
        Resume(SyncSender<Result<(), BackendError>>),
        Shutdown,
    }

    fn run_stream_thread(
        config: OutputStreamConfig,
        renderer: Box<dyn AudioRenderCallback>,
        startup: SyncSender<Result<(), BackendError>>,
        controls: Receiver<StreamControl>,
    ) {
        let stream = match build_stream(&config, renderer) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = startup.send(Err(error));
                return;
            }
        };
        if startup.send(Ok(())).is_err() {
            return;
        }
        while let Ok(control) = controls.recv() {
            match control {
                StreamControl::Pause(reply) => {
                    let result = stream
                        .pause()
                        .map_err(|error| BackendError::new("stream pause", error.to_string()));
                    let _ = reply.send(result);
                }
                StreamControl::Resume(reply) => {
                    let result = stream
                        .play()
                        .map_err(|error| BackendError::new("stream resume", error.to_string()));
                    let _ = reply.send(result);
                }
                StreamControl::Shutdown => break,
            }
        }
    }

    struct CpalRunningStream {
        control: SyncSender<StreamControl>,
        thread: Option<JoinHandle<()>>,
    }

    impl RunningAudioStream for CpalRunningStream {
        fn pause(&self) -> Result<(), BackendError> {
            let (reply, result) = sync_channel(1);
            self.control
                .send(StreamControl::Pause(reply))
                .map_err(|_| BackendError::new("stream pause", "stream owner has stopped"))?;
            result
                .recv()
                .map_err(|_| BackendError::new("stream pause", "stream owner did not reply"))?
        }

        fn resume(&self) -> Result<(), BackendError> {
            let (reply, result) = sync_channel(1);
            self.control
                .send(StreamControl::Resume(reply))
                .map_err(|_| BackendError::new("stream resume", "stream owner has stopped"))?;
            result
                .recv()
                .map_err(|_| BackendError::new("stream resume", "stream owner did not reply"))?
        }
    }

    impl Drop for CpalRunningStream {
        fn drop(&mut self) {
            let _ = self.control.send(StreamControl::Shutdown);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub use self::CpalBackend as PublicCpalBackend;
}

#[cfg(feature = "cpal-backend")]
pub use cpal_impl::PublicCpalBackend as CpalBackend;

/// Windows `IAudioClient3` engine-period measurement. Stream output remains
/// the CPAL/WASAPI fallback until the direct event-driven renderer is promoted
/// after hardware latency validation.
#[cfg(all(windows, feature = "wasapi-cpal-fallback"))]
mod wasapi_probe {
    use super::{
        AudioBackend, AudioRenderCallback, BackendError, OutputDeviceInfo, OutputStreamConfig,
        RunningAudioStream,
    };
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioClient3, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    /// Shared-mode periods reported by `IAudioClient3`, in sample frames.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WasapiEnginePeriods {
        pub sample_rate: u32,
        pub channels: u16,
        pub default_frames: u32,
        pub fundamental_frames: u32,
        pub minimum_frames: u32,
        pub maximum_frames: u32,
    }

    /// Phase-zero backend: probes the direct low-latency WASAPI contract and
    /// uses CPAL's WASAPI stream for output. `uses_direct_stream()` deliberately
    /// reports false so diagnostics never imply the direct renderer is active.
    #[derive(Default)]
    pub struct WasapiLowLatencyBackend {
        fallback: super::CpalBackend,
    }

    impl WasapiLowLatencyBackend {
        pub const fn uses_direct_stream(&self) -> bool {
            false
        }

        pub const fn backend_label(&self) -> &'static str {
            "CPAL/WASAPI fallback (IAudioClient3 period probe available)"
        }

        pub fn probe_default_periods(&self) -> Result<WasapiEnginePeriods, BackendError> {
            // SAFETY: COM is initialized and balanced on this thread. Every
            // interface and format pointer is used only before uninitializing;
            // the format is released with its documented COM allocator.
            unsafe {
                let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
                initialized.ok().map_err(|error| {
                    BackendError::new("WASAPI COM initialization", error.to_string())
                })?;

                let result = (|| {
                    let enumerator: IMMDeviceEnumerator =
                        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(
                            |error| {
                                BackendError::new("WASAPI endpoint enumerator", error.to_string())
                            },
                        )?;
                    let device = enumerator
                        .GetDefaultAudioEndpoint(eRender, eConsole)
                        .map_err(|error| {
                            BackendError::new("WASAPI default endpoint", error.to_string())
                        })?;
                    let client: IAudioClient3 =
                        device.Activate(CLSCTX_ALL, None).map_err(|error| {
                            BackendError::new("IAudioClient3 activation", error.to_string())
                        })?;
                    let format = client.GetMixFormat().map_err(|error| {
                        BackendError::new("WASAPI mix format", error.to_string())
                    })?;
                    if format.is_null() {
                        return Err(BackendError::new(
                            "WASAPI mix format",
                            "endpoint returned a null format",
                        ));
                    }

                    let mut default_frames = 0;
                    let mut fundamental_frames = 0;
                    let mut minimum_frames = 0;
                    let mut maximum_frames = 0;
                    let period_result = client.GetSharedModeEnginePeriod(
                        format,
                        &mut default_frames,
                        &mut fundamental_frames,
                        &mut minimum_frames,
                        &mut maximum_frames,
                    );
                    let sample_rate = (*format).nSamplesPerSec;
                    let channels = (*format).nChannels;
                    CoTaskMemFree(Some(format.cast()));
                    period_result.map_err(|error| {
                        BackendError::new("IAudioClient3 period query", error.to_string())
                    })?;
                    Ok(WasapiEnginePeriods {
                        sample_rate,
                        channels,
                        default_frames,
                        fundamental_frames,
                        minimum_frames,
                        maximum_frames,
                    })
                })();
                CoUninitialize();
                result
            }
        }
    }

    impl AudioBackend for WasapiLowLatencyBackend {
        fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError> {
            self.fallback.output_devices()
        }

        fn preferred_output_config(
            &self,
            device_id: Option<&str>,
        ) -> Result<OutputStreamConfig, BackendError> {
            let mut config = self.fallback.preferred_output_config(device_id)?;
            if device_id.is_none() {
                if let Ok(periods) = self.probe_default_periods() {
                    if periods.sample_rate == config.sample_rate
                        && periods.channels == config.channels
                    {
                        config.buffer_frames = Some(periods.minimum_frames.max(1));
                    }
                }
            }
            Ok(config)
        }

        fn start_output(
            &self,
            config: &OutputStreamConfig,
            renderer: Box<dyn AudioRenderCallback>,
        ) -> Result<Box<dyn RunningAudioStream>, BackendError> {
            self.fallback.start_output(config, renderer)
        }
    }

    pub use self::WasapiEnginePeriods as PublicWasapiEnginePeriods;
    pub use self::WasapiLowLatencyBackend as PublicWasapiLowLatencyBackend;
}

#[cfg(all(windows, feature = "wasapi-cpal-fallback"))]
pub use wasapi_probe::{
    PublicWasapiEnginePeriods as WasapiEnginePeriods,
    PublicWasapiLowLatencyBackend as WasapiLowLatencyBackend,
};

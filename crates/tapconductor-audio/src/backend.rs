//! Platform output abstraction with native Windows ASIO and WASAPI hosts.

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
    /// Sample representation expected by the physical endpoint. The engine
    /// always renders f32 and the platform backend performs any conversion.
    pub sample_format: OutputSampleFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputSampleFormat {
    F32,
    I16,
    I24,
    I32,
}

pub trait RunningAudioStream: Send {
    fn pause(&self) -> Result<(), BackendError>;
    fn resume(&self) -> Result<(), BackendError>;
}

/// Backend methods run only during setup/recovery, never in the callback.
pub trait AudioBackend {
    fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError>;

    /// Returns a supported low-latency configuration for the selected device.
    /// The engine renders float samples; a backend may convert them to the
    /// driver's native representation. Callers must use the returned sample
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
        AudioBackend, AudioRenderCallback, BackendError, OutputDeviceInfo, OutputSampleFormat,
        OutputStreamConfig, RunningAudioStream,
    };
    use crate::RenderCallbackInfo;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const DEVICE_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

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
            // CPAL 0.15's iOS backend reports a placeholder 0..=0 range and
            // rejects every BufferSize::Fixed value. AVAudioSession owns the
            // physical I/O duration on iPad, so leave the stream at the
            // negotiated system default there. macOS and Windows retain the
            // existing low-latency fixed-buffer preference.
            #[cfg(target_os = "ios")]
            let buffer_frames = None;
            #[cfg(not(target_os = "ios"))]
            let buffer_frames =
                buffer_range.map(|(minimum, maximum)| 128_u32.clamp(minimum, maximum));
            Ok(OutputStreamConfig {
                device_id: device_id.map(str::to_owned),
                sample_rate,
                channels,
                buffer_frames,
                sample_format: OutputSampleFormat::F32,
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
            match startup_rx.recv_timeout(DEVICE_OPERATION_TIMEOUT) {
                Ok(Ok(())) => Ok(Box::new(CpalRunningStream {
                    control: control_tx,
                    thread: Some(thread),
                })),
                Ok(Err(error)) => {
                    let _ = thread.join();
                    Err(error)
                }
                Err(RecvTimeoutError::Disconnected) => Err(BackendError::new(
                    "stream thread",
                    "stream owner exited during startup",
                )),
                Err(RecvTimeoutError::Timeout) => Err(BackendError::new(
                    "stream startup",
                    "the audio device did not respond within 5 seconds",
                )),
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
                .try_send(StreamControl::Pause(reply))
                .map_err(|error| {
                    let detail = match error {
                        TrySendError::Full(_) => "stream owner is not responding",
                        TrySendError::Disconnected(_) => "stream owner has stopped",
                    };
                    BackendError::new("stream pause", detail)
                })?;
            result
                .recv_timeout(DEVICE_OPERATION_TIMEOUT)
                .map_err(|error| match error {
                    RecvTimeoutError::Timeout => BackendError::new(
                        "stream pause",
                        "the audio device did not respond within 5 seconds",
                    ),
                    RecvTimeoutError::Disconnected => {
                        BackendError::new("stream pause", "stream owner did not reply")
                    }
                })?
        }

        fn resume(&self) -> Result<(), BackendError> {
            let (reply, result) = sync_channel(1);
            self.control
                .try_send(StreamControl::Resume(reply))
                .map_err(|error| {
                    let detail = match error {
                        TrySendError::Full(_) => "stream owner is not responding",
                        TrySendError::Disconnected(_) => "stream owner has stopped",
                    };
                    BackendError::new("stream resume", detail)
                })?;
            result
                .recv_timeout(DEVICE_OPERATION_TIMEOUT)
                .map_err(|error| match error {
                    RecvTimeoutError::Timeout => BackendError::new(
                        "stream resume",
                        "the audio device did not respond within 5 seconds",
                    ),
                    RecvTimeoutError::Disconnected => {
                        BackendError::new("stream resume", "stream owner did not reply")
                    }
                })?
        }
    }

    impl Drop for CpalRunningStream {
        fn drop(&mut self) {
            let _ = self.control.try_send(StreamControl::Shutdown);
            // A faulty native driver can leave its owner thread inside an
            // operating-system call forever. Dropping the JoinHandle detaches
            // that thread so endpoint recovery and the UI are never held
            // hostage by driver teardown.
            self.thread.take();
        }
    }

    pub use self::CpalBackend as PublicCpalBackend;
}

#[cfg(feature = "cpal-backend")]
pub use cpal_impl::PublicCpalBackend as CpalBackend;

/// Native ASIO host backed by Steinberg's SDK through CPAL. ASIO drivers are
/// identified separately from Windows endpoints so both kinds can appear in
/// one device picker without ambiguous names.
#[cfg(all(windows, feature = "asio-backend"))]
mod asio_impl {
    use super::{
        AudioBackend, AudioRenderCallback, BackendError, OutputDeviceInfo, OutputSampleFormat,
        OutputStreamConfig, RunningAudioStream,
    };
    use crate::RenderCallbackInfo;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError},
        Mutex,
    };
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    const ID_PREFIX: &str = "asio:";
    const DRIVER_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

    pub struct AsioBackend {
        control: SyncSender<OwnerRequest>,
        owner: Mutex<Option<JoinHandle<()>>>,
    }

    impl Default for AsioBackend {
        fn default() -> Self {
            let (control, receiver) = sync_channel(8);
            let owner = thread::Builder::new()
                .name("tapconductor-asio-owner".into())
                .spawn(move || run_owner_thread(receiver))
                .expect("failed to create the ASIO owner thread");
            Self {
                control,
                owner: Mutex::new(Some(owner)),
            }
        }
    }

    impl AsioBackend {
        fn host() -> Result<cpal::Host, BackendError> {
            cpal::host_from_id(cpal::HostId::Asio)
                .map_err(|error| BackendError::new("ASIO host initialization", error.to_string()))
        }

        fn select_device(
            host: &cpal::Host,
            id: Option<&str>,
        ) -> Result<cpal::Device, BackendError> {
            let requested = id.and_then(|value| value.strip_prefix(ID_PREFIX));
            let mut devices = host
                .output_devices()
                .map_err(|error| BackendError::new("ASIO driver enumeration", error.to_string()))?;
            if let Some(requested) = requested {
                return devices
                    .find(|device| device.name().ok().as_deref() == Some(requested))
                    .ok_or_else(|| {
                        BackendError::new(
                            "ASIO driver selection",
                            "the selected ASIO driver is no longer available",
                        )
                    });
            }
            devices.next().ok_or_else(|| {
                BackendError::new(
                    "ASIO driver selection",
                    "no ASIO output driver is installed",
                )
            })
        }
    }

    impl AudioBackend for AsioBackend {
        fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError> {
            request_owner(
                &self.control,
                OwnerRequest::Devices,
                "ASIO driver enumeration",
            )
        }

        fn preferred_output_config(
            &self,
            device_id: Option<&str>,
        ) -> Result<OutputStreamConfig, BackendError> {
            let device_id = device_id.map(str::to_owned);
            request_owner(
                &self.control,
                move |reply| OwnerRequest::Preferred(device_id, reply),
                "ASIO output configuration",
            )
        }

        fn start_output(
            &self,
            config: &OutputStreamConfig,
            renderer: Box<dyn AudioRenderCallback>,
        ) -> Result<Box<dyn RunningAudioStream>, BackendError> {
            let config = config.clone();
            request_owner(
                &self.control,
                move |reply| OwnerRequest::Start(config, renderer, reply),
                "ASIO stream startup",
            )?;
            Ok(Box::new(AsioRunningStream {
                control: self.control.clone(),
            }))
        }
    }

    fn stream_config(config: &OutputStreamConfig) -> cpal::StreamConfig {
        cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: config
                .buffer_frames
                .map(cpal::BufferSize::Fixed)
                .unwrap_or(cpal::BufferSize::Default),
        }
    }

    fn build_stream(
        config: &OutputStreamConfig,
        mut renderer: Box<dyn AudioRenderCallback>,
        device: cpal::Device,
    ) -> Result<cpal::Stream, BackendError> {
        if config.channels == 0 || config.sample_rate == 0 {
            return Err(BackendError::new(
                "ASIO stream setup",
                "sample rate and channel count must be non-zero",
            ));
        }
        let native = device
            .default_output_config()
            .map_err(|error| BackendError::new("ASIO output configuration", error.to_string()))?;
        if native.sample_rate().0 != config.sample_rate || native.channels() < config.channels {
            return Err(BackendError::new(
                "ASIO stream setup",
                "the driver configuration changed while the stream was opening",
            ));
        }

        let cpal_config = stream_config(config);
        let channels = usize::from(config.channels);
        let capacity = usize::try_from(config.buffer_frames.unwrap_or(2048))
            .unwrap_or(2048)
            .saturating_mul(channels);
        let diagnostics = renderer.audio_diagnostics();
        let error_diagnostics = diagnostics.clone();
        let error_callback = move |_error| {
            if let Some(diagnostics) = error_diagnostics.as_ref() {
                diagnostics.note_backend_error();
            }
        };
        let callback_info = move |samples: usize| RenderCallbackInfo {
            estimated_output_latency_frames: Some(
                (samples / channels).min(u32::MAX as usize) as u32
            ),
        };

        let stream = match config.sample_format {
            OutputSampleFormat::F32 => device.build_output_stream(
                &cpal_config,
                move |data: &mut [f32], _| {
                    let _ = renderer.render_audio(data, callback_info(data.len()));
                },
                error_callback,
                None,
            ),
            OutputSampleFormat::I16 => {
                let mut scratch = vec![0.0_f32; capacity];
                device.build_output_stream(
                    &cpal_config,
                    move |data: &mut [i16], _| {
                        if let Some(render) = scratch.get_mut(..data.len()) {
                            let _ = renderer.render_audio(render, callback_info(data.len()));
                            for (output, sample) in data.iter_mut().zip(render.iter().copied()) {
                                *output = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                            }
                        } else {
                            data.fill(0);
                        }
                    },
                    error_callback,
                    None,
                )
            }
            OutputSampleFormat::I32 => {
                let mut scratch = vec![0.0_f32; capacity];
                device.build_output_stream(
                    &cpal_config,
                    move |data: &mut [i32], _| {
                        if let Some(render) = scratch.get_mut(..data.len()) {
                            let _ = renderer.render_audio(render, callback_info(data.len()));
                            for (output, sample) in data.iter_mut().zip(render.iter().copied()) {
                                *output = (sample.clamp(-1.0, 1.0) * i32::MAX as f32) as i32;
                            }
                        } else {
                            data.fill(0);
                        }
                    },
                    error_callback,
                    None,
                )
            }
            OutputSampleFormat::I24 => {
                return Err(BackendError::new(
                    "ASIO sample format",
                    "packed 24-bit ASIO output is not supported by CPAL",
                ));
            }
        }
        .map_err(|error| BackendError::new("ASIO stream construction", error.to_string()))?;
        stream
            .play()
            .map_err(|error| BackendError::new("ASIO stream start", error.to_string()))?;
        Ok(stream)
    }

    enum OwnerRequest {
        Devices(SyncSender<Result<Vec<OutputDeviceInfo>, BackendError>>),
        Preferred(
            Option<String>,
            SyncSender<Result<OutputStreamConfig, BackendError>>,
        ),
        Start(
            OutputStreamConfig,
            Box<dyn AudioRenderCallback>,
            SyncSender<Result<(), BackendError>>,
        ),
        Pause(SyncSender<Result<(), BackendError>>),
        Resume(SyncSender<Result<(), BackendError>>),
        Close(SyncSender<()>),
        Terminate,
    }

    fn request_owner<T>(
        control: &SyncSender<OwnerRequest>,
        request: impl FnOnce(SyncSender<Result<T, BackendError>>) -> OwnerRequest,
        operation: &'static str,
    ) -> Result<T, BackendError> {
        let (reply, result) = sync_channel(1);
        control.try_send(request(reply)).map_err(|error| {
            let detail = match error {
                TrySendError::Full(_) => "the ASIO owner thread is not responding",
                TrySendError::Disconnected(_) => "the ASIO owner thread has stopped",
            };
            BackendError::new(operation, detail)
        })?;
        result
            .recv_timeout(DRIVER_OPERATION_TIMEOUT)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => BackendError::new(
                    operation,
                    "the ASIO driver did not respond within 5 seconds",
                ),
                RecvTimeoutError::Disconnected => {
                    BackendError::new(operation, "the ASIO owner thread did not reply")
                }
            })?
    }

    fn output_config(
        device_id: Option<String>,
        device: &cpal::Device,
    ) -> Result<OutputStreamConfig, BackendError> {
        let config = device
            .default_output_config()
            .map_err(|error| BackendError::new("ASIO output configuration", error.to_string()))?;
        let sample_format = match config.sample_format() {
            cpal::SampleFormat::F32 => OutputSampleFormat::F32,
            cpal::SampleFormat::I16 => OutputSampleFormat::I16,
            cpal::SampleFormat::I32 => OutputSampleFormat::I32,
            format => {
                return Err(BackendError::new(
                    "ASIO sample format",
                    format!("the driver exposes unsupported sample format {format:?}"),
                ));
            }
        };
        let buffer_frames = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, .. } => Some(*min),
            cpal::SupportedBufferSize::Unknown => None,
        };
        Ok(OutputStreamConfig {
            device_id,
            sample_rate: config.sample_rate().0,
            // ASIO exposes physical channels in order. TapConductor uses the
            // main stereo pair (or the sole mono channel), not every output
            // on a multi-channel studio interface.
            channels: config.channels().min(2),
            buffer_frames,
            sample_format,
        })
    }

    fn enumerate_devices(host: &cpal::Host) -> Result<Vec<OutputDeviceInfo>, BackendError> {
        let devices = host
            .output_devices()
            .map_err(|error| BackendError::new("ASIO driver enumeration", error.to_string()))?;
        let mut result = Vec::new();
        for device in devices {
            let Ok(name) = device.name() else { continue };
            // Enumerating the menu must not initialize every installed driver.
            // Some ASIO drivers cannot answer configuration queries while
            // another application owns them, and wrapper drivers may need to
            // show their control panel before they can report a format.  Keep
            // every driver exposed by the ASIO host visible and perform the
            // definitive compatibility check only when the user selects it.
            result.push(OutputDeviceInfo {
                id: format!("{ID_PREFIX}{name}"),
                name: format!("{name} (ASIO)"),
                is_default: false,
            });
        }
        Ok(result)
    }

    fn run_owner_thread(receiver: Receiver<OwnerRequest>) {
        let host = AsioBackend::host();
        let mut cached_devices: Option<Vec<OutputDeviceInfo>> = None;
        let mut prepared_device: Option<(Option<String>, cpal::Device)> = None;
        let mut stream: Option<cpal::Stream> = None;

        while let Ok(request) = receiver.recv() {
            match request {
                OwnerRequest::Devices(reply) => {
                    let result = match (&host, &cached_devices) {
                        (_, Some(devices)) => Ok(devices.clone()),
                        (Ok(host), None) => enumerate_devices(host),
                        (Err(error), None) => Err(error.clone()),
                    };
                    if let Ok(devices) = &result {
                        cached_devices = Some(devices.clone());
                    }
                    let _ = reply.send(result);
                }
                OwnerRequest::Preferred(device_id, reply) => {
                    stream.take();
                    prepared_device = None;
                    let result = match &host {
                        Ok(host) => AsioBackend::select_device(host, device_id.as_deref())
                            .and_then(|device| {
                                let config = output_config(device_id.clone(), &device)?;
                                prepared_device = Some((device_id, device));
                                Ok(config)
                            }),
                        Err(error) => Err(error.clone()),
                    };
                    let _ = reply.send(result);
                }
                OwnerRequest::Start(config, renderer, reply) => {
                    stream.take();
                    let device = prepared_device
                        .take()
                        .filter(|(id, _)| id.as_deref() == config.device_id.as_deref())
                        .map(|(_, device)| Ok(device))
                        .unwrap_or_else(|| match &host {
                            Ok(host) => {
                                AsioBackend::select_device(host, config.device_id.as_deref())
                            }
                            Err(error) => Err(error.clone()),
                        });
                    match device.and_then(|device| build_stream(&config, renderer, device)) {
                        Ok(new_stream) => {
                            stream = Some(new_stream);
                            let _ = reply.send(Ok(()));
                        }
                        Err(error) => {
                            let _ = reply.send(Err(error));
                        }
                    }
                }
                OwnerRequest::Pause(reply) => {
                    let result = stream
                        .as_ref()
                        .ok_or_else(|| {
                            BackendError::new("ASIO stream pause", "no ASIO stream is active")
                        })
                        .and_then(|stream| {
                            stream.pause().map_err(|error| {
                                BackendError::new("ASIO stream pause", error.to_string())
                            })
                        });
                    let _ = reply.send(result);
                }
                OwnerRequest::Resume(reply) => {
                    let result = stream
                        .as_ref()
                        .ok_or_else(|| {
                            BackendError::new("ASIO stream resume", "no ASIO stream is active")
                        })
                        .and_then(|stream| {
                            stream.play().map_err(|error| {
                                BackendError::new("ASIO stream resume", error.to_string())
                            })
                        });
                    let _ = reply.send(result);
                }
                OwnerRequest::Close(reply) => {
                    stream.take();
                    prepared_device = None;
                    let _ = reply.send(());
                }
                OwnerRequest::Terminate => break,
            }
        }
    }

    struct AsioRunningStream {
        control: SyncSender<OwnerRequest>,
    }

    impl RunningAudioStream for AsioRunningStream {
        fn pause(&self) -> Result<(), BackendError> {
            request_owner(&self.control, OwnerRequest::Pause, "ASIO stream pause")
        }

        fn resume(&self) -> Result<(), BackendError> {
            request_owner(&self.control, OwnerRequest::Resume, "ASIO stream resume")
        }
    }

    impl Drop for AsioRunningStream {
        fn drop(&mut self) {
            let (reply, result) = sync_channel(1);
            if self.control.try_send(OwnerRequest::Close(reply)).is_ok() {
                let _ = result.recv_timeout(DRIVER_OPERATION_TIMEOUT);
            }
        }
    }

    impl Drop for AsioBackend {
        fn drop(&mut self) {
            let _ = self.control.try_send(OwnerRequest::Terminate);
            if let Ok(mut owner) = self.owner.lock() {
                // Never join a thread that may be stuck inside a third-party
                // driver. Detaching it allows a fresh backend to be created by
                // the in-app reload action.
                owner.take();
            }
        }
    }

    pub use self::AsioBackend as PublicAsioBackend;
}

#[cfg(all(windows, feature = "asio-backend"))]
pub use asio_impl::PublicAsioBackend as AsioBackend;

#[cfg(all(windows, feature = "wasapi-cpal-fallback"))]
mod wasapi_direct;

/// Windows direct event-driven `IAudioClient3` renderer and engine-period
/// diagnostics, with CPAL retained for endpoint enumeration.
#[cfg(all(windows, feature = "wasapi-cpal-fallback"))]
mod wasapi_probe {
    use super::{
        AudioBackend, AudioRenderCallback, BackendError, OutputDeviceInfo, OutputStreamConfig,
        RunningAudioStream,
    };
    use std::sync::atomic::{AtomicU8, Ordering};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
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

    /// Uses a direct event-driven IAudioClient3 stream for both the default
    /// endpoint and explicitly selected Windows endpoints.
    pub struct WindowsLowLatencyBackend {
        fallback: super::CpalBackend,
        #[cfg(feature = "asio-backend")]
        asio: super::AsioBackend,
        active_backend: AtomicU8,
    }

    const BACKEND_NONE: u8 = 0;
    const BACKEND_WASAPI: u8 = 1;
    const BACKEND_WASAPI_RAW: u8 = 2;
    const BACKEND_ASIO: u8 = 3;

    impl Default for WindowsLowLatencyBackend {
        fn default() -> Self {
            Self {
                fallback: super::CpalBackend,
                #[cfg(feature = "asio-backend")]
                asio: super::AsioBackend::default(),
                active_backend: AtomicU8::new(BACKEND_NONE),
            }
        }
    }

    impl WindowsLowLatencyBackend {
        pub fn uses_direct_stream(&self) -> bool {
            matches!(
                self.active_backend.load(Ordering::Relaxed),
                BACKEND_WASAPI | BACKEND_WASAPI_RAW
            )
        }

        pub fn uses_asio_stream(&self) -> bool {
            self.active_backend.load(Ordering::Relaxed) == BACKEND_ASIO
        }

        pub fn backend_label(&self) -> &'static str {
            match self.active_backend.load(Ordering::Relaxed) {
                BACKEND_ASIO => "Native ASIO",
                BACKEND_WASAPI_RAW => "Direct event-driven WASAPI (raw IAudioClient3)",
                BACKEND_WASAPI => "Direct event-driven WASAPI (IAudioClient3)",
                _ => "Windows audio",
            }
        }

        pub fn probe_default_periods(&self) -> Result<WasapiEnginePeriods, BackendError> {
            // SAFETY: COM is initialized and balanced on this thread. Every
            // interface and format pointer is used only before uninitializing;
            // the format is released with its documented COM allocator.
            unsafe {
                let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
                let should_uninitialize = match initialized.ok() {
                    Ok(()) => true,
                    Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
                    Err(error) => {
                        return Err(BackendError::new(
                            "WASAPI COM initialization",
                            error.to_string(),
                        ));
                    }
                };

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
                if should_uninitialize {
                    CoUninitialize();
                }
                result
            }
        }
    }

    impl AudioBackend for WindowsLowLatencyBackend {
        fn output_devices(&self) -> Result<Vec<OutputDeviceInfo>, BackendError> {
            let mut devices = Vec::new();
            #[cfg(feature = "asio-backend")]
            if let Ok(mut asio_devices) = self.asio.output_devices() {
                devices.append(&mut asio_devices);
            }
            devices.extend(self.fallback.output_devices()?);
            Ok(devices)
        }

        fn preferred_output_config(
            &self,
            device_id: Option<&str>,
        ) -> Result<OutputStreamConfig, BackendError> {
            #[cfg(feature = "asio-backend")]
            if device_id.is_some_and(|id| id.starts_with("asio:")) {
                return self.asio.preferred_output_config(device_id);
            }
            super::wasapi_direct::preferred_output_config(device_id)
        }

        fn start_output(
            &self,
            config: &OutputStreamConfig,
            renderer: Box<dyn AudioRenderCallback>,
        ) -> Result<Box<dyn RunningAudioStream>, BackendError> {
            self.active_backend.store(BACKEND_NONE, Ordering::Relaxed);
            #[cfg(feature = "asio-backend")]
            if config
                .device_id
                .as_deref()
                .is_some_and(|id| id.starts_with("asio:"))
            {
                let stream = self.asio.start_output(config, renderer)?;
                self.active_backend.store(BACKEND_ASIO, Ordering::Relaxed);
                return Ok(stream);
            }
            let (stream, raw_mode) = super::wasapi_direct::start_output(config, renderer)?;
            self.active_backend.store(
                if raw_mode {
                    BACKEND_WASAPI_RAW
                } else {
                    BACKEND_WASAPI
                },
                Ordering::Relaxed,
            );
            Ok(stream)
        }
    }

    pub use self::WasapiEnginePeriods as PublicWasapiEnginePeriods;
    pub use self::WindowsLowLatencyBackend as PublicWindowsLowLatencyBackend;
}

#[cfg(all(windows, feature = "wasapi-cpal-fallback"))]
pub use wasapi_probe::{
    PublicWasapiEnginePeriods as WasapiEnginePeriods,
    PublicWindowsLowLatencyBackend as WindowsLowLatencyBackend,
};

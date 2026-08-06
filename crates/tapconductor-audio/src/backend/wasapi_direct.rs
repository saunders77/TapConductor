// Copyright (c) 2026 Michael Saunders
use super::{
    AudioRenderCallback, BackendError, OutputSampleFormat, OutputStreamConfig, RunningAudioStream,
};
use crate::RenderCallbackInfo;
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    slice,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread::{self, JoinHandle},
};
use windows::{
    core::w,
    Win32::{
        Devices::Properties::DEVPKEY_Device_FriendlyName,
        Foundation::{CloseHandle, HANDLE, RPC_E_CHANGED_MODE, WAIT_OBJECT_0},
        Media::{
            Audio::{
                eConsole, eRender, AudioCategory_Media, AudioClientProperties, IAudioClient3,
                IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator,
                AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                AUDCLNT_STREAMOPTIONS_NONE, DEVICE_STATE_ACTIVE,
            },
            KernelStreaming::WAVE_FORMAT_EXTENSIBLE,
            Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT},
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                StructuredStorage::PropVariantClear, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
            },
            Threading::{
                AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW,
                SetEvent, WaitForMultipleObjects, INFINITE,
            },
            Variant::VT_LPWSTR,
        },
    },
};

pub(super) fn preferred_output_config(
    device_id: Option<&str>,
) -> Result<OutputStreamConfig, BackendError> {
    let selected_name = device_id.and_then(cpal_device_name);
    with_com("direct WASAPI format query", || unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|error| {
                BackendError::new("direct WASAPI endpoint enumerator", error.to_string())
            })?;
        let device = select_device(&enumerator, selected_name)?;
        let client: IAudioClient3 = device.Activate(CLSCTX_ALL, None).map_err(|error| {
            BackendError::new("direct IAudioClient3 activation", error.to_string())
        })?;
        let format = client
            .GetMixFormat()
            .map_err(|error| BackendError::new("direct WASAPI mix format", error.to_string()))?;
        if format.is_null() {
            return Err(BackendError::new(
                "direct WASAPI mix format",
                "endpoint returned a null format",
            ));
        }
        let result = (|| {
            ensure_float_mix_format(format)?;
            let sample_rate = (*format).nSamplesPerSec;
            let channels = (*format).nChannels;
            let sample_format = OutputSampleFormat::F32;
            let mut default_period = 0;
            let mut fundamental_period = 0;
            let mut minimum_period = 0;
            let mut maximum_period = 0;
            client
                .GetSharedModeEnginePeriod(
                    format,
                    &mut default_period,
                    &mut fundamental_period,
                    &mut minimum_period,
                    &mut maximum_period,
                )
                .map_err(|error| {
                    BackendError::new("direct WASAPI period query", error.to_string())
                })?;
            Ok(OutputStreamConfig {
                device_id: device_id.map(str::to_owned),
                sample_rate,
                channels,
                buffer_frames: Some(minimum_period.max(1)),
                sample_format,
            })
        })();
        CoTaskMemFree(Some(format.cast()));
        result
    })
}

fn with_com<T>(
    operation: &'static str,
    work: impl FnOnce() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        let should_uninitialize = match initialized.ok() {
            Ok(()) => true,
            Err(error) if error.code() == RPC_E_CHANGED_MODE => false,
            Err(error) => return Err(BackendError::new(operation, error.to_string())),
        };
        let result = work();
        if should_uninitialize {
            CoUninitialize();
        }
        result
    }
}

fn cpal_device_name(device_id: &str) -> Option<&str> {
    device_id
        .strip_prefix("cpal:")?
        .split_once(':')
        .map(|(_, name)| name)
}

fn initialization_error(operation: &'static str, error: windows::core::Error) -> BackendError {
    BackendError::new(operation, error.to_string())
}

unsafe fn select_device(
    enumerator: &IMMDeviceEnumerator,
    selected_name: Option<&str>,
) -> Result<windows::Win32::Media::Audio::IMMDevice, BackendError> {
    // SAFETY: the returned COM interfaces remain owned by the caller's
    // COM-initialized thread.
    unsafe {
        let Some(selected_name) = selected_name else {
            return enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|error| {
                    BackendError::new("direct WASAPI default endpoint", error.to_string())
                });
        };

        let devices = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|error| {
                BackendError::new("direct WASAPI endpoint enumeration", error.to_string())
            })?;
        let count = devices.GetCount().map_err(|error| {
            BackendError::new("direct WASAPI endpoint count", error.to_string())
        })?;
        for index in 0..count {
            let device = devices.Item(index).map_err(|error| {
                BackendError::new("direct WASAPI endpoint selection", error.to_string())
            })?;
            if device_name(&device)? == selected_name {
                return Ok(device);
            }
        }
        Err(BackendError::new(
            "direct WASAPI device selection",
            format!("output device is unavailable: {selected_name}"),
        ))
    }
}

unsafe fn device_name(
    device: &windows::Win32::Media::Audio::IMMDevice,
) -> Result<String, BackendError> {
    // SAFETY: the property value is checked for VT_LPWSTR before its union is
    // read and is always cleared after a successful GetValue call.
    unsafe {
        let property_store = device.OpenPropertyStore(STGM_READ).map_err(|error| {
            BackendError::new("direct WASAPI device properties", error.to_string())
        })?;
        let mut property_value = property_store
            .GetValue(
                &DEVPKEY_Device_FriendlyName as *const _
                    as *const windows::Win32::Foundation::PROPERTYKEY,
            )
            .map_err(|error| BackendError::new("direct WASAPI device name", error.to_string()))?;
        let variant = &property_value.Anonymous.Anonymous;
        let result = if variant.vt != VT_LPWSTR {
            Err(BackendError::new(
                "direct WASAPI device name",
                "friendly-name property is not a Unicode string",
            ))
        } else {
            let pointer = variant.Anonymous.pwszVal.0;
            if pointer.is_null() {
                Err(BackendError::new(
                    "direct WASAPI device name",
                    "friendly-name property is null",
                ))
            } else {
                let mut length = 0_isize;
                while *pointer.offset(length) != 0 {
                    length += 1;
                }
                let wide = slice::from_raw_parts(pointer, length as usize);
                Ok(OsString::from_wide(wide).to_string_lossy().into_owned())
            }
        };
        let _ = PropVariantClear(&mut property_value);
        result
    }
}

unsafe fn ensure_float_mix_format(
    format: *const windows::Win32::Media::Audio::WAVEFORMATEX,
) -> Result<(), BackendError> {
    // SAFETY: caller supplies a non-null WAVEFORMATEX allocated by WASAPI.
    unsafe {
        let format_ref = &*format;
        let is_float = if u32::from(format_ref.wFormatTag) == WAVE_FORMAT_IEEE_FLOAT {
            format_ref.wBitsPerSample == 32
        } else if u32::from(format_ref.wFormatTag) == WAVE_FORMAT_EXTENSIBLE
            && usize::from(format_ref.cbSize)
                >= size_of::<windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE>()
                    - size_of::<windows::Win32::Media::Audio::WAVEFORMATEX>()
        {
            let extensible = format.cast::<windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE>();
            format_ref.wBitsPerSample == 32
                && core::ptr::addr_of!((*extensible).SubFormat).read_unaligned()
                    == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
        } else {
            false
        };
        if is_float {
            Ok(())
        } else {
            Err(BackendError::new(
                "direct WASAPI format",
                "endpoint mix format is not compatible 32-bit float audio",
            ))
        }
    }
}

enum StreamControl {
    Pause(SyncSender<Result<(), BackendError>>),
    Resume(SyncSender<Result<(), BackendError>>),
    Shutdown,
}

pub(super) fn start_output(
    config: &OutputStreamConfig,
    renderer: Box<dyn AudioRenderCallback>,
) -> Result<(Box<dyn RunningAudioStream>, bool), BackendError> {
    let config = config.clone();
    let (startup_tx, startup_rx) = sync_channel(1);
    let (control_tx, control_rx) = sync_channel(8);
    let thread = thread::Builder::new()
        .name("tapconductor-wasapi-render".into())
        .spawn(move || run_stream(config, renderer, startup_tx, control_rx))
        .map_err(|error| BackendError::new("direct WASAPI thread", error.to_string()))?;

    match startup_rx.recv() {
        Ok(Ok((control_event, raw_mode))) => Ok((
            Box::new(DirectWasapiStream {
                control: control_tx,
                control_event: HANDLE(control_event as *mut _),
                thread: Some(thread),
            }),
            raw_mode,
        )),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err(BackendError::new(
                "direct WASAPI startup",
                "render thread exited before initialization completed",
            ))
        }
    }
}

fn run_stream(
    config: OutputStreamConfig,
    mut renderer: Box<dyn AudioRenderCallback>,
    startup: SyncSender<Result<(usize, bool), BackendError>>,
    controls: Receiver<StreamControl>,
) {
    // SAFETY: every COM interface and Win32 handle is confined to this owner
    // thread. COM initialization and every successful handle creation are
    // balanced on all normal exit paths.
    unsafe {
        if let Err(error) = CoInitializeEx(None, COINIT_MULTITHREADED).ok() {
            let _ = startup.send(Err(BackendError::new(
                "direct WASAPI COM initialization",
                error.to_string(),
            )));
            return;
        }

        let result = initialize_stream(&config, &mut *renderer);
        let (
            client,
            render_client,
            audio_event,
            control_event,
            buffer_frames,
            stream_latency_frames,
            raw_mode,
            mut scratch,
        ) = match result {
            Ok(values) => values,
            Err(error) => {
                let _ = startup.send(Err(error));
                CoUninitialize();
                return;
            }
        };

        let mut task_index = 0;
        let mmcss = AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index).ok();
        if let Err(error) = client.Start() {
            let _ = startup.send(Err(BackendError::new(
                "direct WASAPI stream start",
                error.to_string(),
            )));
            let _ = CloseHandle(audio_event);
            let _ = CloseHandle(control_event);
            if let Some(handle) = mmcss {
                let _ = AvRevertMmThreadCharacteristics(handle);
            }
            CoUninitialize();
            return;
        }
        if startup
            .send(Ok((control_event.0 as usize, raw_mode)))
            .is_err()
        {
            let _ = client.Stop();
            let _ = CloseHandle(audio_event);
            let _ = CloseHandle(control_event);
            if let Some(handle) = mmcss {
                let _ = AvRevertMmThreadCharacteristics(handle);
            }
            CoUninitialize();
            return;
        }

        let diagnostics = renderer.audio_diagnostics();
        let handles = [audio_event, control_event];
        let endpoint = RenderEndpoint {
            buffer_frames,
            stream_latency_frames,
            channels: config.channels,
            sample_format: config.sample_format,
        };
        let mut running = true;
        while running {
            let signaled = WaitForMultipleObjects(&handles, false, INFINITE);
            if signaled == WAIT_OBJECT_0 {
                if let Err(error) = render_available(
                    &client,
                    &render_client,
                    endpoint,
                    &mut *renderer,
                    &mut scratch,
                ) {
                    if let Some(diagnostics) = diagnostics.as_ref() {
                        diagnostics.note_backend_error();
                    }
                    let _ = error;
                    break;
                }
            } else if signaled.0 == WAIT_OBJECT_0.0 + 1 {
                while let Ok(control) = controls.try_recv() {
                    match control {
                        StreamControl::Pause(reply) => {
                            let response = client.Stop().map_err(|error| {
                                BackendError::new("direct WASAPI pause", error.to_string())
                            });
                            let _ = reply.send(response);
                        }
                        StreamControl::Resume(reply) => {
                            let response = client.Start().map_err(|error| {
                                BackendError::new("direct WASAPI resume", error.to_string())
                            });
                            let _ = reply.send(response);
                        }
                        StreamControl::Shutdown => {
                            running = false;
                            break;
                        }
                    }
                }
            } else {
                if let Some(diagnostics) = diagnostics.as_ref() {
                    diagnostics.note_backend_error();
                }
                break;
            }
        }

        let _ = client.Stop();
        if let Some(handle) = mmcss {
            let _ = AvRevertMmThreadCharacteristics(handle);
        }
        let _ = CloseHandle(audio_event);
        let _ = CloseHandle(control_event);
        CoUninitialize();
    }
}

type InitializedStream = (
    IAudioClient3,
    IAudioRenderClient,
    HANDLE,
    HANDLE,
    u32,
    u32,
    bool,
    Vec<f32>,
);

unsafe fn initialize_stream(
    config: &OutputStreamConfig,
    renderer: &mut dyn AudioRenderCallback,
) -> Result<InitializedStream, BackendError> {
    // SAFETY: called on a COM-initialized owner thread. The mix-format pointer
    // is released with CoTaskMemFree after stream initialization has copied it.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|error| {
                BackendError::new("direct WASAPI endpoint enumerator", error.to_string())
            })?;
        let selected_name = config.device_id.as_deref().and_then(cpal_device_name);
        let device = select_device(&enumerator, selected_name)?;
        let mut client: IAudioClient3 = device.Activate(CLSCTX_ALL, None).map_err(|error| {
            BackendError::new("direct IAudioClient3 activation", error.to_string())
        })?;
        let properties = AudioClientProperties {
            cbSize: size_of::<AudioClientProperties>() as u32,
            bIsOffload: false.into(),
            eCategory: AudioCategory_Media,
            Options: AUDCLNT_STREAMOPTIONS_NONE,
        };
        // Normal shared mode avoids endpoint-specific RAW-mode failures. The
        // event callback and IAudioClient3 engine period still provide the
        // low-latency path; RAW only controls Windows audio effects.
        let raw_mode = false;
        client.SetClientProperties(&properties).map_err(|error| {
            BackendError::new("direct WASAPI client properties", error.to_string())
        })?;
        let mix_format = client
            .GetMixFormat()
            .map_err(|error| BackendError::new("direct WASAPI mix format", error.to_string()))?;
        if mix_format.is_null() {
            return Err(BackendError::new(
                "direct WASAPI mix format",
                "endpoint returned a null format",
            ));
        }

        let stream_format = &*mix_format;
        let format_matches = stream_format.nSamplesPerSec == config.sample_rate
            && stream_format.nChannels == config.channels;
        let float_format = ensure_float_mix_format(mix_format).is_ok();
        if !format_matches || !float_format {
            CoTaskMemFree(Some(mix_format.cast()));
            return Err(BackendError::new(
                "direct WASAPI format",
                "the selected endpoint format does not match the negotiated audio format",
            ));
        }

        let mut default_period = 0;
        let mut fundamental_period = 0;
        let mut minimum_period = 0;
        let mut maximum_period = 0;
        if let Err(error) = client.GetSharedModeEnginePeriod(
            stream_format,
            &mut default_period,
            &mut fundamental_period,
            &mut minimum_period,
            &mut maximum_period,
        ) {
            CoTaskMemFree(Some(mix_format.cast()));
            return Err(BackendError::new(
                "direct WASAPI period query",
                error.to_string(),
            ));
        }
        let requested = config.buffer_frames.unwrap_or(minimum_period);
        let fundamental = fundamental_period.max(1);
        let period = requested
            .max(minimum_period)
            .min(maximum_period)
            .div_ceil(fundamental)
            .saturating_mul(fundamental)
            .min(maximum_period);
        let low_latency_result = client.InitializeSharedAudioStream(
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            period,
            stream_format,
            None,
        );
        if low_latency_result.is_err() && config.device_id.is_some() {
            // Some consumer endpoints cannot join an already-running audio
            // engine through IAudioClient3's period API. A fresh traditional
            // shared-mode client remains event driven and lets Windows choose
            // the compatible engine buffer instead of failing the device
            // switch outright.
            client = device.Activate(CLSCTX_ALL, None).map_err(|error| {
                CoTaskMemFree(Some(mix_format.cast()));
                BackendError::new("direct WASAPI compatibility activation", error.to_string())
            })?;
            if let Err(error) = client.SetClientProperties(&properties) {
                CoTaskMemFree(Some(mix_format.cast()));
                return Err(BackendError::new(
                    "direct WASAPI compatibility properties",
                    error.to_string(),
                ));
            }
            let rate = i64::from(config.sample_rate.max(1));
            let scaled = i64::from(period).saturating_mul(10_000_000);
            let duration = scaled.saturating_add(rate - 1) / rate;
            if let Err(error) = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                duration,
                0,
                stream_format,
                None,
            ) {
                CoTaskMemFree(Some(mix_format.cast()));
                return Err(initialization_error(
                    "direct WASAPI compatibility initialization",
                    error,
                ));
            }
        } else if let Err(error) = low_latency_result {
            CoTaskMemFree(Some(mix_format.cast()));
            return Err(initialization_error(
                "direct WASAPI shared-stream initialization",
                error,
            ));
        }
        CoTaskMemFree(Some(mix_format.cast()));

        let audio_event = CreateEventW(None, false, false, None)
            .map_err(|error| BackendError::new("direct WASAPI audio event", error.to_string()))?;
        if let Err(error) = client.SetEventHandle(audio_event) {
            let _ = CloseHandle(audio_event);
            return Err(BackendError::new(
                "direct WASAPI event registration",
                error.to_string(),
            ));
        }
        let control_event = match CreateEventW(None, false, false, None) {
            Ok(event) => event,
            Err(error) => {
                let _ = CloseHandle(audio_event);
                return Err(BackendError::new(
                    "direct WASAPI control event",
                    error.to_string(),
                ));
            }
        };
        let render_client: IAudioRenderClient = client.GetService().map_err(|error| {
            let _ = CloseHandle(audio_event);
            let _ = CloseHandle(control_event);
            BackendError::new("direct WASAPI render service", error.to_string())
        })?;
        let buffer_frames = client.GetBufferSize().map_err(|error| {
            let _ = CloseHandle(audio_event);
            let _ = CloseHandle(control_event);
            BackendError::new("direct WASAPI buffer size", error.to_string())
        })?;
        let stream_latency_frames = client
            .GetStreamLatency()
            .ok()
            .and_then(|hundred_nanoseconds| {
                u64::try_from(hundred_nanoseconds)
                    .ok()
                    .and_then(|duration| {
                        duration
                            .saturating_mul(u64::from(config.sample_rate))
                            .checked_add(9_999_999)
                            .map(|scaled| scaled / 10_000_000)
                    })
                    .and_then(|frames| u32::try_from(frames).ok())
            })
            .unwrap_or(period);

        // Prime the endpoint before Start, as required by event-driven WASAPI.
        let sample_count = usize::try_from(buffer_frames)
            .ok()
            .and_then(|frames| frames.checked_mul(usize::from(config.channels)))
            .ok_or_else(|| BackendError::new("direct WASAPI render", "buffer size overflow"))?;
        let mut scratch = vec![0.0; sample_count];
        render_frames(
            &render_client,
            buffer_frames,
            config.channels,
            config.sample_format,
            0,
            renderer,
            &mut scratch,
        )?;
        Ok((
            client,
            render_client,
            audio_event,
            control_event,
            buffer_frames,
            stream_latency_frames,
            raw_mode,
            scratch,
        ))
    }
}

unsafe fn render_available(
    client: &IAudioClient3,
    render_client: &IAudioRenderClient,
    endpoint: RenderEndpoint,
    renderer: &mut dyn AudioRenderCallback,
    scratch: &mut [f32],
) -> Result<(), BackendError> {
    // SAFETY: the render client owns the returned buffer until ReleaseBuffer.
    unsafe {
        let padding = client
            .GetCurrentPadding()
            .map_err(|error| BackendError::new("direct WASAPI padding query", error.to_string()))?;
        let frames = endpoint.buffer_frames.saturating_sub(padding);
        if frames == 0 {
            return Ok(());
        }
        render_frames(
            render_client,
            frames,
            endpoint.channels,
            endpoint.sample_format,
            padding.saturating_add(endpoint.stream_latency_frames),
            renderer,
            scratch,
        )
    }
}

#[derive(Clone, Copy)]
struct RenderEndpoint {
    buffer_frames: u32,
    stream_latency_frames: u32,
    channels: u16,
    sample_format: OutputSampleFormat,
}

unsafe fn render_frames(
    render_client: &IAudioRenderClient,
    frames: u32,
    channels: u16,
    sample_format: OutputSampleFormat,
    queued_frames: u32,
    renderer: &mut dyn AudioRenderCallback,
    scratch: &mut [f32],
) -> Result<(), BackendError> {
    // SAFETY: WASAPI provides frames * channels writable f32 samples for the
    // negotiated mix format. ReleaseBuffer ends the temporary slice lifetime.
    unsafe {
        let buffer = render_client.GetBuffer(frames).map_err(|error| {
            BackendError::new("direct WASAPI acquire buffer", error.to_string())
        })?;
        let sample_count = usize::try_from(frames)
            .ok()
            .and_then(|value| value.checked_mul(usize::from(channels)))
            .ok_or_else(|| BackendError::new("direct WASAPI render", "buffer size overflow"))?;
        let info = RenderCallbackInfo {
            estimated_output_latency_frames: Some(queued_frames),
        };
        match sample_format {
            OutputSampleFormat::F32 => {
                let samples = slice::from_raw_parts_mut(buffer.cast::<f32>(), sample_count);
                samples.fill(0.0);
                let _ = renderer.render_audio(samples, info);
            }
            integer_format => {
                let samples = scratch.get_mut(..sample_count).ok_or_else(|| {
                    BackendError::new("direct WASAPI render", "conversion buffer is too small")
                })?;
                samples.fill(0.0);
                let _ = renderer.render_audio(samples, info);
                convert_integer_samples(buffer, samples, integer_format);
            }
        }
        render_client
            .ReleaseBuffer(frames, 0)
            .map_err(|error| BackendError::new("direct WASAPI release buffer", error.to_string()))
    }
}

unsafe fn convert_integer_samples(
    output: *mut u8,
    samples: &[f32],
    sample_format: OutputSampleFormat,
) {
    unsafe {
        match sample_format {
            OutputSampleFormat::I16 => {
                let target = slice::from_raw_parts_mut(output.cast::<i16>(), samples.len());
                for (target, sample) in target.iter_mut().zip(samples) {
                    *target = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
                }
            }
            OutputSampleFormat::I24 => {
                let target = slice::from_raw_parts_mut(output, samples.len().saturating_mul(3));
                for (bytes, sample) in target.chunks_exact_mut(3).zip(samples) {
                    let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
                    let bytes32 = value.to_le_bytes();
                    bytes.copy_from_slice(&bytes32[..3]);
                }
            }
            OutputSampleFormat::I32 => {
                let target = slice::from_raw_parts_mut(output.cast::<i32>(), samples.len());
                for (target, sample) in target.iter_mut().zip(samples) {
                    *target = (sample.clamp(-1.0, 1.0) * i32::MAX as f32).round() as i32;
                }
            }
            OutputSampleFormat::F32 => unreachable!("float output requires no conversion"),
        }
    }
}

struct DirectWasapiStream {
    control: SyncSender<StreamControl>,
    control_event: HANDLE,
    thread: Option<JoinHandle<()>>,
}

// HANDLE is a process-local opaque event handle. Access is synchronized by
// the kernel and its lifetime is bounded by joining the owner thread.
unsafe impl Send for DirectWasapiStream {}

impl DirectWasapiStream {
    fn request(
        &self,
        make_control: impl FnOnce(SyncSender<Result<(), BackendError>>) -> StreamControl,
        operation: &'static str,
    ) -> Result<(), BackendError> {
        let (reply_tx, reply_rx) = sync_channel(1);
        self.control
            .send(make_control(reply_tx))
            .map_err(|_| BackendError::new(operation, "render thread has stopped"))?;
        unsafe { SetEvent(self.control_event) }
            .map_err(|error| BackendError::new(operation, error.to_string()))?;
        reply_rx
            .recv()
            .map_err(|_| BackendError::new(operation, "render thread did not reply"))?
    }
}

impl RunningAudioStream for DirectWasapiStream {
    fn pause(&self) -> Result<(), BackendError> {
        self.request(StreamControl::Pause, "direct WASAPI pause")
    }

    fn resume(&self) -> Result<(), BackendError> {
        self.request(StreamControl::Resume, "direct WASAPI resume")
    }
}

impl Drop for DirectWasapiStream {
    fn drop(&mut self) {
        let _ = self.control.send(StreamControl::Shutdown);
        unsafe {
            let _ = SetEvent(self.control_event);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// Copyright (c) 2026 Michael Saunders
use std::time::Instant;
#[cfg(windows)]
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tapconductor_audio::{
    AudioCommand, Chord, Note, PianoConfig, PianoSynth, RenderCallbackInfo, VoiceGroupId,
    audio_engine,
};
#[cfg(windows)]
use tapconductor_audio::{
    AudioDiagnosticSnapshot, AudioDiagnostics, AudioRenderCallback, RenderStatus,
};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BUFFER_FRAMES: usize = 128;
const ITERATIONS: usize = 2_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    if std::env::args().any(|argument| argument == "--live") {
        return run_live_output_probe();
    }

    let synth = PianoSynth::<256>::new(PianoConfig::new(SAMPLE_RATE))?;
    let (mut sender, mut engine, diagnostics) =
        audio_engine::<_, 256, 256>(synth, SAMPLE_RATE, CHANNELS);
    let chord = Chord::try_from_slice(&[
        Note::from_midi1(36, 96),
        Note::from_midi1(48, 96),
        Note::from_midi1(55, 96),
        Note::from_midi1(60, 96),
        Note::from_midi1(64, 96),
        Note::from_midi1(67, 96),
        Note::from_midi1(72, 96),
        Note::from_midi1(76, 96),
        Note::from_midi1(79, 96),
        Note::from_midi1(84, 96),
    ])?;
    let mut buffer = [0.0_f32; BUFFER_FRAMES * CHANNELS as usize];
    let mut dispatch_nanoseconds = Vec::with_capacity(ITERATIONS);
    let mut latest_first_audible_frame = None;

    // Warm every code path and voice allocation before measurement.
    sender
        .try_send(AudioCommand::PlaySlice {
            group: VoiceGroupId(1),
            at: engine.sample_clock(),
            chord,
        })
        .map_err(|_| "warm-up queue overflow")?;
    engine.render_block(&mut buffer, RenderCallbackInfo::default());
    sender.panic_at(engine.sample_clock());
    engine.render_block(&mut buffer, RenderCallbackInfo::default());

    for iteration in 0..ITERATIONS {
        let group = VoiceGroupId(iteration as u64 + 2);
        let at = engine.sample_clock();
        let started = Instant::now();
        sender
            .try_send(AudioCommand::PlaySlice { group, at, chord })
            .map_err(|_| "measurement queue overflow")?;
        engine.render_block(&mut buffer, RenderCallbackInfo::default());
        dispatch_nanoseconds.push(started.elapsed().as_nanos() as u64);
        latest_first_audible_frame = buffer
            .chunks_exact(CHANNELS as usize)
            .position(|frame| frame.iter().any(|sample| sample.abs() > 1.0e-7));
        sender.panic_at(engine.sample_clock());
        engine.render_block(&mut buffer, RenderCallbackInfo::default());
    }

    dispatch_nanoseconds.sort_unstable();
    let percentile = |percent: usize| -> f64 {
        let index = ((ITERATIONS - 1) * percent) / 100;
        dispatch_nanoseconds[index] as f64 / 1_000_000.0
    };
    let snapshot = diagnostics.snapshot();
    println!(
        concat!(
            "{{\n",
            "  \"kind\": \"offline-command-to-render\",\n",
            "  \"iterations\": {},\n",
            "  \"sampleRate\": {},\n",
            "  \"bufferFrames\": {},\n",
            "  \"firstAudibleFrame\": {},\n",
            "  \"p50Milliseconds\": {:.4},\n",
            "  \"p95Milliseconds\": {:.4},\n",
            "  \"p99Milliseconds\": {:.4},\n",
            "  \"queueOverflows\": {},\n",
            "  \"lateCommands\": {}\n",
            "}}"
        ),
        ITERATIONS,
        SAMPLE_RATE,
        BUFFER_FRAMES,
        latest_first_audible_frame.unwrap_or(usize::MAX),
        percentile(50),
        percentile(95),
        percentile(99),
        snapshot.queue_overflows,
        snapshot.late_commands,
    );
    Ok(())
}

#[cfg(windows)]
struct OnsetProbe<R> {
    renderer: R,
    epoch: Instant,
    command_nanoseconds: Arc<AtomicU64>,
    first_render_nanoseconds: Arc<AtomicU64>,
}

#[cfg(windows)]
impl<R: AudioRenderCallback> AudioRenderCallback for OnsetProbe<R> {
    fn render_audio(&mut self, output: &mut [f32], info: RenderCallbackInfo) -> RenderStatus {
        let result = self.renderer.render_audio(output, info);
        let command = self.command_nanoseconds.load(Ordering::Acquire);
        if command != 0
            && self.first_render_nanoseconds.load(Ordering::Relaxed) == 0
            && output.iter().any(|sample| sample.abs() > 1.0e-7)
        {
            let now = self.epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            let _ = self.first_render_nanoseconds.compare_exchange(
                0,
                now.max(command),
                Ordering::Release,
                Ordering::Relaxed,
            );
        }
        result
    }

    fn audio_diagnostics(&self) -> Option<Arc<AudioDiagnostics>> {
        self.renderer.audio_diagnostics()
    }
}

#[cfg(windows)]
fn run_live_output_probe() -> Result<(), Box<dyn std::error::Error>> {
    use tapconductor_audio::backend::{AudioBackend, WindowsLowLatencyBackend};

    let backend = WindowsLowLatencyBackend::default();
    let requested_device = std::env::args()
        .skip_while(|argument| argument != "--device")
        .nth(1);
    let devices = backend.output_devices()?;
    let selected = if let Some(requested) = requested_device.as_deref() {
        Some(
            devices
                .iter()
                .find(|device| {
                    device
                        .name
                        .to_lowercase()
                        .contains(&requested.to_lowercase())
                })
                .ok_or_else(|| format!("no output device name contains {requested:?}"))?,
        )
    } else {
        None
    };
    let device_id = selected.map(|device| device.id.as_str());
    let output_device = selected
        .map(|device| device.name.clone())
        .or_else(|| {
            devices
                .iter()
                .find(|device| device.is_default)
                .map(|device| device.name.clone())
        })
        .unwrap_or_else(|| "System default".to_owned());
    let config = backend.preferred_output_config(device_id)?;
    if std::env::args().any(|argument| argument == "--query-only") {
        println!(
            "output={output_device:?} rate={} channels={} format={:?} period_frames={}",
            config.sample_rate,
            config.channels,
            config.sample_format,
            config.buffer_frames.unwrap_or_default()
        );
        return Ok(());
    }
    let periods = if selected.is_none() {
        backend.probe_default_periods()?
    } else {
        tapconductor_audio::backend::WasapiEnginePeriods {
            sample_rate: config.sample_rate,
            channels: config.channels,
            default_frames: 0,
            fundamental_frames: 0,
            minimum_frames: config.buffer_frames.unwrap_or(0),
            maximum_frames: 0,
        }
    };
    let synth = PianoSynth::<32>::new(PianoConfig::new(config.sample_rate))?;
    let (mut sender, engine, diagnostics) =
        audio_engine::<_, 64, 64>(synth, config.sample_rate, config.channels);
    let epoch = Instant::now();
    let command_nanoseconds = Arc::new(AtomicU64::new(0));
    let first_render_nanoseconds = Arc::new(AtomicU64::new(0));
    let renderer = OnsetProbe {
        renderer: engine,
        epoch,
        command_nanoseconds: Arc::clone(&command_nanoseconds),
        first_render_nanoseconds: Arc::clone(&first_render_nanoseconds),
    };
    let stream = backend.start_output(&config, Box::new(renderer))?;
    if std::env::args().any(|argument| argument == "--open-only") {
        drop(stream);
        println!(
            "opened output={output_device:?} backend={} rate={} channels={} period_frames={}",
            backend.backend_label(),
            config.sample_rate,
            config.channels,
            config.buffer_frames.unwrap_or_default(),
        );
        return Ok(());
    }
    std::thread::sleep(Duration::from_millis(250));

    let chord = Chord::try_from_slice(&[Note::from_midi1(72, 64)])?;
    let at = diagnostics.snapshot().rendered_frames;
    let command_time = epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    command_nanoseconds.store(command_time.max(1), Ordering::Release);
    sender
        .try_send(AudioCommand::PlaySlice {
            group: VoiceGroupId(1),
            at,
            chord,
        })
        .map_err(|_| "live probe command queue overflow")?;

    let deadline = Instant::now() + Duration::from_secs(2);
    while first_render_nanoseconds.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let first_render = first_render_nanoseconds.load(Ordering::Acquire);
    let snapshot = diagnostics.snapshot();
    sender.panic_at(snapshot.rendered_frames);
    std::thread::sleep(Duration::from_millis(50));
    drop(stream);
    if first_render == 0 {
        return Err("live output probe did not observe an audible sample".into());
    }

    print_live_result(
        &config,
        periods,
        snapshot,
        first_render.saturating_sub(command_time) as f64 / 1_000_000.0,
        backend.backend_label(),
        &output_device,
    );
    Ok(())
}

#[cfg(windows)]
fn print_live_result(
    config: &tapconductor_audio::backend::OutputStreamConfig,
    periods: tapconductor_audio::backend::WasapiEnginePeriods,
    snapshot: AudioDiagnosticSnapshot,
    queue_to_render_ms: f64,
    backend: &str,
    output_device: &str,
) {
    let endpoint_ms = f64::from(snapshot.estimated_output_latency_frames) * 1_000.0
        / f64::from(snapshot.sample_rate.max(1));
    println!(
        concat!(
            "{{\n",
            "  \"kind\": \"live-default-output\",\n",
            "  \"backend\": \"{}\",\n",
            "  \"outputDevice\": \"{}\",\n",
            "  \"sampleRate\": {},\n",
            "  \"channels\": {},\n",
            "  \"requestedPeriodFrames\": {},\n",
            "  \"defaultPeriodFrames\": {},\n",
            "  \"minimumPeriodFrames\": {},\n",
            "  \"actualCallbackFrames\": {},\n",
            "  \"nativeQueueToFirstRenderMs\": {:.3},\n",
            "  \"reportedStreamLatencyMs\": {:.3},\n",
            "  \"nativeToEndpointEstimateMs\": {:.3},\n",
            "  \"lateCommands\": {},\n",
            "  \"queueOverflows\": {}\n",
            "}}"
        ),
        backend,
        output_device.replace('"', "\\\""),
        config.sample_rate,
        config.channels,
        config.buffer_frames.unwrap_or(0),
        periods.default_frames,
        periods.minimum_frames,
        snapshot.latest_buffer_frames,
        queue_to_render_ms,
        endpoint_ms,
        queue_to_render_ms + endpoint_ms,
        snapshot.late_commands,
        snapshot.queue_overflows,
    );
}

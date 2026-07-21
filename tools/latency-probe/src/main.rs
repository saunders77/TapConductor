use std::time::Instant;
use tapconductor_audio::{
    AudioCommand, Chord, Note, PianoConfig, PianoSynth, RenderCallbackInfo, VoiceGroupId,
    audio_engine,
};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
const BUFFER_FRAMES: usize = 128;
const ITERATIONS: usize = 2_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

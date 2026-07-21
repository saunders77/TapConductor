use crate::command::{AudioCommand, SampleTime, VoiceGroupId};
use crate::diagnostics::AudioDiagnostics;
use crate::queue::{spsc_channel, Consumer, Producer, QueueFull};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Result of allocating one voice in a sampler's preallocated pool.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceStart {
    Started,
    StoleOlderVoice,
    Rejected,
}

/// Real-time sampler contract.
///
/// Implementations must preallocate all voices and resources before playback.
/// None of these methods may allocate, lock, block, log, decode, or perform
/// file/network/device I/O. `render` adds interleaved samples to a buffer that
/// the engine has already cleared.
pub trait Sampler: Send + 'static {
    fn note_on(&mut self, group: VoiceGroupId, pitch: u8, velocity: u16) -> VoiceStart;
    fn release_group(&mut self, group: VoiceGroupId);
    fn panic(&mut self);
    fn render(&mut self, interleaved: &mut [f32], channels: usize);

    fn active_voice_count(&self) -> usize {
        0
    }
}

/// Per-callback information supplied by a platform backend.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderCallbackInfo {
    /// Backend estimate, if it can be obtained without work in the callback.
    pub estimated_output_latency_frames: Option<u32>,
}

/// Object-safe interface accepted by audio backends.
pub trait AudioRenderCallback: Send + 'static {
    fn render_audio(&mut self, output: &mut [f32], info: RenderCallbackInfo) -> RenderStatus;

    /// Gives a backend an atomic-only error reporting path. Called once during
    /// stream setup, never from the audio callback.
    fn audio_diagnostics(&self) -> Option<Arc<AudioDiagnostics>> {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderStatus {
    Rendered { frames: usize },
    InvalidBuffer,
}

#[derive(Debug)]
struct PanicSignal {
    at: AtomicU64,
    pending: AtomicBool,
}

impl PanicSignal {
    fn new() -> Self {
        Self {
            at: AtomicU64::new(0),
            pending: AtomicBool::new(false),
        }
    }

    fn publish(&self, at: SampleTime) {
        self.at.store(at, Ordering::Relaxed);
        self.pending.store(true, Ordering::Release);
    }

    fn take(&self) -> Option<SampleTime> {
        if self.pending.swap(false, Ordering::Acquire) {
            Some(self.at.load(Ordering::Relaxed))
        } else {
            None
        }
    }
}

/// The sole non-real-time producer handle.
pub struct AudioCommandSender<const QUEUE: usize> {
    producer: Producer<AudioCommand, QUEUE>,
    diagnostics: Arc<AudioDiagnostics>,
    panic_signal: Arc<PanicSignal>,
}

impl<const QUEUE: usize> AudioCommandSender<QUEUE> {
    /// Enqueues a command without waiting. Panic uses a dedicated atomic path,
    /// so a full ordinary-command queue can never prevent emergency silence.
    #[allow(clippy::result_large_err)] // Returning the inline command avoids an allocation.
    pub fn try_send(&mut self, command: AudioCommand) -> Result<(), QueueFull<AudioCommand>> {
        if let AudioCommand::Panic { at } = command {
            self.panic_signal.publish(at);
            return Ok(());
        }

        self.producer.try_push(command).inspect_err(|_| {
            self.diagnostics.note_queue_overflow();
        })
    }

    pub fn panic_at(&mut self, at: SampleTime) {
        self.panic_signal.publish(at);
    }

    pub fn diagnostics(&self) -> &Arc<AudioDiagnostics> {
        &self.diagnostics
    }
}

/// Creates the command path and engine. All heap allocation occurs here,
/// before the returned engine is installed in an audio callback.
pub fn audio_engine<S, const QUEUE: usize, const SCHEDULED: usize>(
    sampler: S,
    sample_rate: u32,
    channels: u16,
) -> (
    AudioCommandSender<QUEUE>,
    AudioEngine<S, QUEUE, SCHEDULED>,
    Arc<AudioDiagnostics>,
)
where
    S: Sampler,
{
    assert!(sample_rate > 0, "sample rate must be non-zero");
    assert!(channels > 0, "channel count must be non-zero");
    assert!(SCHEDULED > 0, "scheduled command capacity must be non-zero");

    let (producer, consumer) = spsc_channel::<AudioCommand, QUEUE>();
    let diagnostics = Arc::new(AudioDiagnostics::default());
    diagnostics.configure(sample_rate, channels);
    let panic_signal = Arc::new(PanicSignal::new());

    let sender = AudioCommandSender {
        producer,
        diagnostics: Arc::clone(&diagnostics),
        panic_signal: Arc::clone(&panic_signal),
    };
    let engine = AudioEngine {
        sampler,
        receiver: consumer,
        diagnostics: Arc::clone(&diagnostics),
        panic_signal,
        channels: channels as usize,
        sample_clock: 0,
        master_gain: 1.0,
        pending: vec![None; SCHEDULED].into_boxed_slice(),
        pending_len: 0,
    };
    (sender, engine, diagnostics)
}

/// Callback-owned scheduler and sampler.
pub struct AudioEngine<S, const QUEUE: usize, const SCHEDULED: usize>
where
    S: Sampler,
{
    sampler: S,
    receiver: Consumer<AudioCommand, QUEUE>,
    diagnostics: Arc<AudioDiagnostics>,
    panic_signal: Arc<PanicSignal>,
    channels: usize,
    sample_clock: SampleTime,
    master_gain: f32,
    // One setup-time allocation keeps the callback fixed-capacity while
    // avoiding a multi-hundred-kilobyte inline stack value at app startup.
    pending: Box<[Option<AudioCommand>]>,
    pending_len: usize,
}

impl<S, const QUEUE: usize, const SCHEDULED: usize> AudioEngine<S, QUEUE, SCHEDULED>
where
    S: Sampler,
{
    pub const fn sample_clock(&self) -> SampleTime {
        self.sample_clock
    }

    pub const fn channels(&self) -> usize {
        self.channels
    }

    pub fn diagnostics(&self) -> &Arc<AudioDiagnostics> {
        &self.diagnostics
    }

    pub fn sampler(&self) -> &S {
        &self.sampler
    }

    /// Intended for setup/tests only, before the engine enters a callback.
    pub fn sampler_mut(&mut self) -> &mut S {
        &mut self.sampler
    }

    pub fn render_block(&mut self, output: &mut [f32], info: RenderCallbackInfo) -> RenderStatus {
        output.fill(0.0);
        if self.channels == 0 || !output.len().is_multiple_of(self.channels) {
            self.diagnostics.note_invalid_buffer();
            return RenderStatus::InvalidBuffer;
        }

        let frames = output.len() / self.channels;
        self.collect_commands();

        let block_start = self.sample_clock;
        let block_end = block_start.saturating_add(frames as u64);
        let mut frame_cursor = 0usize;
        let mut consumed = 0usize;
        let mut panic_applied = false;

        while consumed < self.pending_len {
            let command = self.pending[consumed].expect("occupied scheduled prefix");
            let at = command.at();
            if at >= block_end {
                break;
            }

            if at < block_start {
                self.diagnostics.note_late_command();
            }
            let target = at.saturating_sub(block_start).min(frames as u64) as usize;
            if target > frame_cursor {
                self.render_segment(output, frame_cursor, target);
                frame_cursor = target;
            }

            consumed += 1;
            if self.apply_command(command) {
                // Panic is authoritative over every command that was already
                // queued, including future releases/attacks. Commands enqueued
                // after this callback begins are observed on the next block.
                self.pending_len = 0;
                panic_applied = true;
                break;
            }
        }

        if !panic_applied && consumed > 0 {
            let remaining = self.pending_len - consumed;
            self.pending.copy_within(consumed..self.pending_len, 0);
            for slot in &mut self.pending[remaining..self.pending_len] {
                *slot = None;
            }
            self.pending_len = remaining;
        }

        if frame_cursor < frames {
            self.render_segment(output, frame_cursor, frames);
        }

        self.sample_clock = block_end;
        self.diagnostics.note_callback(
            frames.min(u32::MAX as usize) as u32,
            info.estimated_output_latency_frames,
        );
        self.diagnostics
            .set_active_voices(self.sampler.active_voice_count());
        RenderStatus::Rendered { frames }
    }

    fn collect_commands(&mut self) {
        if let Some(at) = self.panic_signal.take() {
            self.insert_priority_panic(AudioCommand::Panic { at });
        }
        while let Some(command) = self.receiver.try_pop() {
            if self.insert_sorted(command).is_err() {
                self.diagnostics.note_schedule_overflow();
            }
        }
    }

    fn insert_priority_panic(&mut self, command: AudioCommand) {
        if self.pending_len == SCHEDULED {
            // Safety commands may evict the furthest-future ordinary command.
            self.pending_len -= 1;
            self.pending[self.pending_len] = None;
            self.diagnostics.note_schedule_overflow();
        }
        let _ = self.insert_sorted(command);
    }

    #[allow(clippy::result_large_err)] // Scheduled commands remain inline and allocation-free.
    fn insert_sorted(&mut self, command: AudioCommand) -> Result<(), AudioCommand> {
        if self.pending_len == SCHEDULED {
            return Err(command);
        }
        let at = command.at();
        let mut index = self.pending_len;
        // Strict comparison preserves FIFO order for equal sample times.
        while index > 0
            && self.pending[index - 1]
                .expect("occupied scheduled prefix")
                .at()
                > at
        {
            self.pending[index] = self.pending[index - 1];
            index -= 1;
        }
        self.pending[index] = Some(command);
        self.pending_len += 1;
        Ok(())
    }

    /// Returns true when a Panic invalidated the rest of the schedule.
    fn apply_command(&mut self, command: AudioCommand) -> bool {
        match command {
            AudioCommand::PlaySlice { group, chord, .. } => {
                // No sample is rendered between these calls, so every note in
                // the chord begins at exactly the same output frame.
                for note in chord.as_slice() {
                    if self.sampler.note_on(group, note.pitch, note.velocity)
                        == VoiceStart::StoleOlderVoice
                    {
                        self.diagnostics.note_voice_steal();
                    }
                }
                false
            }
            AudioCommand::ReleaseGroup { group, .. } => {
                self.sampler.release_group(group);
                false
            }
            AudioCommand::Panic { .. } => {
                self.sampler.panic();
                true
            }
            AudioCommand::SetMasterGain { gain, .. } => {
                self.master_gain = if gain.is_finite() {
                    gain.clamp(0.0, 2.0)
                } else {
                    1.0
                };
                false
            }
        }
    }

    fn render_segment(&mut self, output: &mut [f32], first_frame: usize, end_frame: usize) {
        let first = first_frame * self.channels;
        let end = end_frame * self.channels;
        let segment = &mut output[first..end];
        self.sampler.render(segment, self.channels);
        if self.master_gain != 1.0 {
            for sample in segment {
                *sample *= self.master_gain;
            }
        }
    }
}

impl<S, const QUEUE: usize, const SCHEDULED: usize> AudioRenderCallback
    for AudioEngine<S, QUEUE, SCHEDULED>
where
    S: Sampler,
{
    fn render_audio(&mut self, output: &mut [f32], info: RenderCallbackInfo) -> RenderStatus {
        self.render_block(output, info)
    }

    fn audio_diagnostics(&self) -> Option<Arc<AudioDiagnostics>> {
        Some(Arc::clone(&self.diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Chord, Note};

    #[derive(Default)]
    struct TraceSampler {
        rendered_frames: u64,
        starts: Vec<(u64, VoiceGroupId, u8)>,
        releases: Vec<(u64, VoiceGroupId)>,
        panics: Vec<u64>,
    }

    impl Sampler for TraceSampler {
        fn note_on(&mut self, group: VoiceGroupId, pitch: u8, _velocity: u16) -> VoiceStart {
            self.starts.push((self.rendered_frames, group, pitch));
            VoiceStart::Started
        }

        fn release_group(&mut self, group: VoiceGroupId) {
            self.releases.push((self.rendered_frames, group));
        }

        fn panic(&mut self) {
            self.panics.push(self.rendered_frames);
        }

        fn render(&mut self, interleaved: &mut [f32], channels: usize) {
            let frames = interleaved.len() / channels;
            self.rendered_frames += frames as u64;
            interleaved.fill(0.25);
        }
    }

    fn chord(pitches: &[u8]) -> Chord {
        let mut chord = Chord::empty();
        for pitch in pitches {
            chord.push(Note::new(*pitch, 50_000)).unwrap();
        }
        chord
    }

    #[test]
    fn every_chord_note_starts_on_exactly_the_same_frame() {
        let (mut tx, mut engine, _) =
            audio_engine::<TraceSampler, 16, 16>(TraceSampler::default(), 48_000, 2);
        tx.try_send(AudioCommand::PlaySlice {
            group: VoiceGroupId(7),
            at: 11,
            chord: chord(&[48, 60, 64, 67, 72]),
        })
        .unwrap();
        let mut output = [0.0; 64];
        engine.render_block(&mut output, RenderCallbackInfo::default());

        assert_eq!(engine.sampler().starts.len(), 5);
        assert!(engine.sampler().starts.iter().all(|start| start.0 == 11));
    }

    #[test]
    fn future_commands_are_sorted_and_groups_release_independently() {
        let (mut tx, mut engine, _) =
            audio_engine::<TraceSampler, 16, 16>(TraceSampler::default(), 48_000, 1);
        tx.try_send(AudioCommand::ReleaseGroup {
            group: VoiceGroupId(1),
            at: 25,
        })
        .unwrap();
        tx.try_send(AudioCommand::PlaySlice {
            group: VoiceGroupId(2),
            at: 8,
            chord: chord(&[60]),
        })
        .unwrap();
        tx.try_send(AudioCommand::ReleaseGroup {
            group: VoiceGroupId(2),
            at: 12,
        })
        .unwrap();

        let mut first = [0.0; 16];
        engine.render_block(&mut first, RenderCallbackInfo::default());
        assert_eq!(engine.sampler().starts[0].0, 8);
        assert_eq!(engine.sampler().releases, vec![(12, VoiceGroupId(2))]);

        let mut second = [0.0; 16];
        engine.render_block(&mut second, RenderCallbackInfo::default());
        assert_eq!(engine.sampler().releases[1], (25, VoiceGroupId(1)));
    }

    #[test]
    fn panic_cannot_be_lost_to_a_full_queue_and_cancels_future_work() {
        let (mut tx, mut engine, diagnostics) =
            audio_engine::<TraceSampler, 1, 8>(TraceSampler::default(), 48_000, 1);
        tx.try_send(AudioCommand::PlaySlice {
            group: VoiceGroupId(1),
            at: 100,
            chord: chord(&[60]),
        })
        .unwrap();
        assert!(tx
            .try_send(AudioCommand::ReleaseGroup {
                group: VoiceGroupId(1),
                at: 200,
            })
            .is_err());
        tx.panic_at(5);

        let mut output = [0.0; 16];
        engine.render_block(&mut output, RenderCallbackInfo::default());
        assert_eq!(engine.sampler().panics, vec![5]);
        assert_eq!(diagnostics.snapshot().queue_overflows, 1);

        for _ in 0..16 {
            engine.render_block(&mut output, RenderCallbackInfo::default());
        }
        assert!(engine.sampler().starts.is_empty());
    }

    #[test]
    fn gain_change_applies_at_its_sample_boundary() {
        let (mut tx, mut engine, _) =
            audio_engine::<TraceSampler, 4, 4>(TraceSampler::default(), 48_000, 1);
        tx.try_send(AudioCommand::SetMasterGain { gain: 0.5, at: 3 })
            .unwrap();
        let mut output = [0.0; 8];
        engine.render_block(&mut output, RenderCallbackInfo::default());
        assert_eq!(&output[..3], &[0.25; 3]);
        assert_eq!(&output[3..], &[0.125; 5]);
    }

    #[test]
    fn production_capacities_construct_on_a_small_windows_sized_stack() {
        std::thread::Builder::new()
            .stack_size(256 * 1_024)
            .spawn(|| {
                let (tx, engine, _) =
                    audio_engine::<TraceSampler, 2_048, 2_048>(TraceSampler::default(), 48_000, 2);
                assert_eq!(tx.producer.capacity(), 2_048);
                assert_eq!(engine.pending.len(), 2_048);
            })
            .expect("small-stack test thread starts")
            .join()
            .expect("large capacities do not overflow the setup stack");
    }
}

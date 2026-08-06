// Copyright (c) 2026 Michael Saunders
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Lock-free counters written on the real-time path and sampled by the UI.
#[derive(Debug, Default)]
pub struct AudioDiagnostics {
    sample_rate: AtomicU32,
    channels: AtomicU32,
    latest_buffer_frames: AtomicU32,
    estimated_output_latency_frames: AtomicU32,
    callbacks: AtomicU64,
    rendered_frames: AtomicU64,
    late_commands: AtomicU64,
    queue_overflows: AtomicU64,
    schedule_overflows: AtomicU64,
    invalid_buffers: AtomicU64,
    backend_errors: AtomicU64,
    voice_steals: AtomicU64,
    active_voices: AtomicU32,
}

impl AudioDiagnostics {
    pub fn configure(&self, sample_rate: u32, channels: u16) {
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        self.channels.store(channels as u32, Ordering::Relaxed);
    }

    pub fn note_backend_error(&self) {
        self.backend_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> AudioDiagnosticSnapshot {
        AudioDiagnosticSnapshot {
            sample_rate: self.sample_rate.load(Ordering::Relaxed),
            channels: self.channels.load(Ordering::Relaxed) as u16,
            latest_buffer_frames: self.latest_buffer_frames.load(Ordering::Relaxed),
            estimated_output_latency_frames: self
                .estimated_output_latency_frames
                .load(Ordering::Relaxed),
            callbacks: self.callbacks.load(Ordering::Relaxed),
            rendered_frames: self.rendered_frames.load(Ordering::Relaxed),
            late_commands: self.late_commands.load(Ordering::Relaxed),
            queue_overflows: self.queue_overflows.load(Ordering::Relaxed),
            schedule_overflows: self.schedule_overflows.load(Ordering::Relaxed),
            invalid_buffers: self.invalid_buffers.load(Ordering::Relaxed),
            backend_errors: self.backend_errors.load(Ordering::Relaxed),
            voice_steals: self.voice_steals.load(Ordering::Relaxed),
            active_voices: self.active_voices.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn note_callback(&self, frames: u32, latency_frames: Option<u32>) {
        self.latest_buffer_frames.store(frames, Ordering::Relaxed);
        if let Some(latency) = latency_frames {
            self.estimated_output_latency_frames
                .store(latency, Ordering::Relaxed);
        }
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        self.rendered_frames
            .fetch_add(frames as u64, Ordering::Relaxed);
    }

    pub(crate) fn note_late_command(&self) {
        self.late_commands.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_queue_overflow(&self) {
        self.queue_overflows.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_schedule_overflow(&self) {
        self.schedule_overflows.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_invalid_buffer(&self) {
        self.invalid_buffers.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn note_voice_steal(&self) {
        self.voice_steals.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn set_active_voices(&self, voices: usize) {
        self.active_voices
            .store(voices.min(u32::MAX as usize) as u32, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AudioDiagnosticSnapshot {
    pub sample_rate: u32,
    pub channels: u16,
    pub latest_buffer_frames: u32,
    pub estimated_output_latency_frames: u32,
    pub callbacks: u64,
    pub rendered_frames: u64,
    pub late_commands: u64,
    pub queue_overflows: u64,
    pub schedule_overflows: u64,
    pub invalid_buffers: u64,
    pub backend_errors: u64,
    pub voice_steals: u64,
    pub active_voices: u32,
}

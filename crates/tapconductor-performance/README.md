# tapconductor-performance

Pure, device-independent performance state for TapConductor. The crate owns the
authoritative score cursor, physical input pairing, per-gesture voice groups,
generation checks, and sample-clock release scheduling. It does not open audio
or MIDI devices and it performs no wall-clock timing.

The default piano gate releases a voice group only after both conditions are
known:

1. another slice has been triggered; and
2. 100 ms have elapsed after the physical input that created the group was
   released.

The resulting release sample is
`max(first_later_trigger, input_release + 100 ms)`. Each trigger receives a
globally unique group ID, so overlapping occurrences of the same MIDI pitch are
never coupled.

`PerformanceEngine::handle` returns a fixed-capacity `Transition`. Enqueue its
audio commands without waiting for UI work. Release commands can target a
future sample and can arrive before a newer immediate play command, so the audio
side must use its bounded sample scheduler rather than FIFO execution time.

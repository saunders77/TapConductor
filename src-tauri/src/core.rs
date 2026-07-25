use crate::{
    audio_runtime::AudioManager,
    dto::{CoreEventDto, LoadedScoreDto},
    midi_runtime::{MidiInputAction, MidiManager},
    session::ScoreSession,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
use tapconductor_performance::{
    Chord, EngineConfig, EventId, Generation, IgnoreReason, InputId, MidiPitch, PerformanceCommand,
    PerformanceEngine, PerformanceEvent, SampleRate, SampleTime, ScoreSequence, Slice, StaffSlice,
    Transition, TriggerKind, Velocity,
};

const MIN_TAP_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Default)]
struct TapInputGate {
    last_accepted: Option<Instant>,
}

impl TapInputGate {
    fn accept(&mut self, received_at: Instant) -> bool {
        if self
            .last_accepted
            .is_some_and(|last| received_at.duration_since(last) < MIN_TAP_INTERVAL)
        {
            return false;
        }
        self.last_accepted = Some(received_at);
        true
    }
}

pub struct AppCore {
    pub audio: AudioManager,
    pub midi: MidiManager,
    performance: PerformanceEngine,
    direct_midi_performance: PerformanceEngine,
    score: Option<ScoreSession>,
    input_ids: HashMap<String, InputId>,
    next_input_id: u64,
    tap_input_gate: TapInputGate,
    beat_tap_mode: bool,
    midi_free_play: bool,
    direct_midi_tokens: HashSet<String>,
}

impl AppCore {
    pub fn new(
        midi_action_sender: Sender<MidiInputAction>,
        salamander_directory: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        let audio = AudioManager::new(salamander_directory);
        let rate = SampleRate::new(audio.sample_rate())
            .ok_or_else(|| "Invalid audio sample rate.".to_owned())?;
        let performance = PerformanceEngine::with_default_gate(rate, EngineConfig::default())
            .map_err(|error| error.to_string())?;
        let mut direct_midi_performance =
            PerformanceEngine::with_default_gate(rate, EngineConfig::default())
                .map_err(|error| error.to_string())?;
        direct_midi_performance
            .load_score(direct_midi_sequence(), SampleTime::ZERO)
            .map_err(|error| error.to_string())?;
        Ok(Self {
            midi: MidiManager::new(midi_action_sender, audio.sample_rate()),
            audio,
            performance,
            direct_midi_performance,
            score: None,
            input_ids: HashMap::new(),
            next_input_id: 1,
            tap_input_gate: TapInputGate::default(),
            beat_tap_mode: false,
            midi_free_play: false,
            direct_midi_tokens: HashSet::new(),
        })
    }

    pub fn set_beat_tap_mode(&mut self, enabled: bool) -> Result<Option<CoreEventDto>, String> {
        self.beat_tap_mode = enabled;
        self.panic()
    }

    pub const fn beat_tap_mode(&self) -> bool {
        self.beat_tap_mode
    }

    pub fn set_midi_free_play(&mut self, enabled: bool) -> Result<Option<CoreEventDto>, String> {
        if self.midi_free_play == enabled {
            return Ok(None);
        }
        self.midi_free_play = enabled;
        self.direct_midi_tokens.clear();
        self.panic()
    }

    pub const fn midi_free_play(&self) -> bool {
        self.midi_free_play
    }

    pub fn load_score(
        &mut self,
        path: PathBuf,
    ) -> Result<(LoadedScoreDto, Option<CoreEventDto>), String> {
        let mut session = ScoreSession::load(path, 0)?;
        let sequence = sequence_from_events(session.events())?;
        let at = self.now();
        let transition = self
            .performance
            .load_score(sequence, at)
            .map_err(|error| error.to_string())?;
        let generation = self
            .performance
            .generation()
            .ok_or_else(|| "Performance engine did not accept the score.".to_owned())?;
        session.set_generation(generation.get());
        let event = self.apply_transition(transition)?;
        let dto = session.dto();
        self.score = Some(session);
        Ok((dto, event))
    }

    pub fn set_audio_device(
        &mut self,
        selected_device: Option<String>,
    ) -> Result<Option<CoreEventDto>, String> {
        // A rate change is only legal across a safety boundary. Panic first so
        // no deadline expressed in the old device's frames survives.
        let event = self.panic()?;
        self.audio.restart(selected_device)?;
        self.apply_audio_sample_rate()?;
        Ok(event)
    }

    /// Silence all held voices and release the mobile output stream before the
    /// operating system suspends the application.
    #[cfg(mobile)]
    pub fn suspend_audio(&mut self) -> Result<Option<CoreEventDto>, String> {
        let event = self.panic()?;
        self.audio.suspend();
        self.input_ids.clear();
        self.direct_midi_tokens.clear();
        self.tap_input_gate = TapInputGate::default();
        Ok(event)
    }

    /// Rebuild the mobile stream because its route, channel count, or sample
    /// rate may have changed while the application was inactive.
    #[cfg(mobile)]
    pub fn resume_audio(&mut self) -> Result<(), String> {
        self.audio.resume()?;
        self.apply_audio_sample_rate()
    }

    fn apply_audio_sample_rate(&mut self) -> Result<(), String> {
        let sample_rate = SampleRate::new(self.audio.sample_rate())
            .ok_or_else(|| "Invalid audio sample rate.".to_owned())?;
        self.performance
            .set_sample_rate(sample_rate)
            .map_err(|error| error.to_string())?;
        self.direct_midi_performance
            .set_sample_rate(sample_rate)
            .map_err(|error| error.to_string())?;
        self.midi.set_sample_rate(sample_rate.get());
        Ok(())
    }

    pub fn set_instrument(&mut self, instrument: &str) -> Result<Option<CoreEventDto>, String> {
        let event = self.panic()?;
        self.audio.set_instrument(instrument)?;
        Ok(event)
    }

    pub fn set_roll_delays(&mut self, regular_ms: u16, audition_ms: u16) -> Result<(), String> {
        self.performance.set_roll_delays(regular_ms, audition_ms);
        Ok(())
    }

    pub fn set_part_enabled(
        &mut self,
        generation: u64,
        part_id: &str,
        enabled: bool,
    ) -> Result<(LoadedScoreDto, Option<CoreEventDto>), String> {
        self.ensure_audio_ready()?;
        self.require_ui_generation(generation)?;
        let session = self
            .score
            .as_mut()
            .ok_or_else(|| "No score is loaded.".to_owned())?;
        session.set_part_enabled(part_id, enabled)?;
        let sequence = sequence_from_events(session.events())?;
        let transition = self
            .performance
            .load_score(sequence, SampleTime::new(self.audio.now_sample()))
            .map_err(|error| error.to_string())?;
        let new_generation = self
            .performance
            .generation()
            .expect("load_score sets generation");
        session.set_generation(new_generation.get());
        let dto = session.dto();
        let event = self.apply_transition(transition)?;
        Ok((dto, event))
    }

    pub fn input_down(
        &mut self,
        token: String,
        midi_velocity: u8,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let generation = self.current_generation()?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        // A repeated down for an already-held physical input remains the
        // performance engine's responsibility. It is ignored there and must
        // not restart the receiver's suppression window.
        if !self.input_ids.contains_key(&token) && !self.tap_input_gate.accept(Instant::now()) {
            return Ok(None);
        }
        let (input, inserted) = self.input_for_down(token.clone())?;
        let result = self
            .performance
            .handle(PerformanceCommand::Tap {
                generation,
                input,
                at: self.now(),
                velocity,
            })
            .map_err(|error| error.to_string());
        if result.is_err() && inserted {
            self.input_ids.remove(&token);
        }
        self.apply_transition(result?)
    }

    /// Play incoming MIDI keys directly, without reading or advancing the score.
    pub fn direct_midi_down(
        &mut self,
        token: String,
        midi_pitch: u8,
        midi_velocity: u8,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let pitch = MidiPitch::new(midi_pitch)
            .ok_or_else(|| format!("Invalid MIDI pitch {midi_pitch}."))?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        let (input, inserted) = self.input_for_down(token.clone())?;
        let generation = self
            .direct_midi_performance
            .generation()
            .expect("direct MIDI engine has a permanent one-note sequence");
        let result = self
            .direct_midi_performance
            .handle(PerformanceCommand::AuditionNote {
                generation,
                event: EventId::new(1),
                pitch,
                input,
                at: self.now(),
                velocity,
            });
        if result.is_err() && inserted {
            self.input_ids.remove(&token);
        } else {
            self.direct_midi_tokens.insert(token);
        }
        self.apply_direct_transition(result.map_err(|error| error.to_string())?)
    }

    pub fn audition(
        &mut self,
        generation: u64,
        index: usize,
        token: String,
        midi_velocity: u8,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let engine_generation = self.require_ui_generation(generation)?;
        let event = self.event_id(index)?;
        let sounding_pitches = self
            .score
            .as_ref()
            .ok_or_else(|| "No score is loaded.".to_owned())?
            .sounding_pitches_at(index)?;
        let chord =
            Chord::from_midi_numbers(&sounding_pitches).map_err(|error| error.to_string())?;
        let (input, inserted) = self.input_for_down(token.clone())?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        let result = self
            .performance
            .handle(PerformanceCommand::AuditionChord {
                generation: engine_generation,
                event,
                chord,
                input,
                at: self.now(),
                velocity,
            })
            .map_err(|error| error.to_string());
        if result.is_err() && inserted {
            self.input_ids.remove(&token);
        }
        self.apply_transition(result?)
    }

    pub fn audition_note(
        &mut self,
        generation: u64,
        index: usize,
        midi_pitch: u8,
        token: String,
        midi_velocity: u8,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let engine_generation = self.require_ui_generation(generation)?;
        let event = self.event_id(index)?;
        let pitch = MidiPitch::new(midi_pitch)
            .ok_or_else(|| format!("Invalid MIDI pitch {midi_pitch}."))?;
        let (input, inserted) = self.input_for_down(token.clone())?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        let result = self
            .performance
            .handle(PerformanceCommand::AuditionNote {
                generation: engine_generation,
                event,
                pitch,
                input,
                at: self.now(),
                velocity,
            })
            .map_err(|error| error.to_string());
        if result.is_err() && inserted {
            self.input_ids.remove(&token);
        }
        self.apply_transition(result?)
    }

    pub fn audition_chord(
        &mut self,
        generation: u64,
        index: usize,
        midi_pitches: Vec<u8>,
        token: String,
        midi_velocity: u8,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let engine_generation = self.require_ui_generation(generation)?;
        let event = self.event_id(index)?;
        let chord = Chord::from_midi_numbers(&midi_pitches).map_err(|error| error.to_string())?;
        let (input, inserted) = self.input_for_down(token.clone())?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        let result = self
            .performance
            .handle(PerformanceCommand::AuditionChord {
                generation: engine_generation,
                event,
                chord,
                input,
                at: self.now(),
                velocity,
            })
            .map_err(|error| error.to_string());
        if result.is_err() && inserted {
            self.input_ids.remove(&token);
        }
        self.apply_transition(result?)
    }

    pub fn release_input(&mut self, token: &str) -> Result<Option<CoreEventDto>, String> {
        let Some(input) = self.input_ids.get(token).copied() else {
            return Ok(None);
        };
        let direct = self.direct_midi_tokens.remove(token);
        let at = self.now();
        let engine = if direct {
            &mut self.direct_midi_performance
        } else {
            &mut self.performance
        };
        let transition = engine
            .handle(PerformanceCommand::InputReleased { input, at })
            .map_err(|error| error.to_string())?;
        // The engine has now accepted the physical release and removed its
        // latch. Keep the token if handle() rejects so a later release can retry.
        self.input_ids.remove(token);
        if direct {
            self.apply_direct_transition(transition)
        } else {
            self.apply_transition(transition)
        }
    }

    pub fn reposition(
        &mut self,
        generation: u64,
        index: usize,
    ) -> Result<Option<CoreEventDto>, String> {
        self.ensure_audio_ready()?;
        let generation = self.require_ui_generation(generation)?;
        let event = self.event_id(index)?;
        let transition = self
            .performance
            .handle(PerformanceCommand::Reposition {
                generation,
                event,
                at: self.now(),
            })
            .map_err(|error| error.to_string())?;
        self.apply_transition(transition)
    }

    pub fn panic(&mut self) -> Result<Option<CoreEventDto>, String> {
        let transition = self
            .performance
            .handle(PerformanceCommand::Panic { at: self.now() })
            .map_err(|error| error.to_string())?;
        let event = self.apply_transition(transition)?;
        let direct_transition = self
            .direct_midi_performance
            .handle(PerformanceCommand::Panic { at: self.now() })
            .map_err(|error| error.to_string())?;
        self.apply_direct_transition(direct_transition)?;
        Ok(event)
    }

    /// A MIDI mapper panic means its token tracker has already been cleared,
    /// so no later Note Offs can release those engine latches. Explicitly pair
    /// every native MIDI input before the safety stop. Keyboard/pointer tokens
    /// remain latched until their real matching up event.
    pub fn panic_midi_inputs(&mut self) -> Result<Option<CoreEventDto>, String> {
        let tokens: Vec<String> = self
            .input_ids
            .keys()
            .filter(|token| token.starts_with("midi:"))
            .cloned()
            .collect();
        for token in tokens {
            self.release_input(&token)?;
        }
        self.panic()
    }

    fn input_for_down(&mut self, token: String) -> Result<(InputId, bool), String> {
        if let Some(existing) = self.input_ids.get(&token) {
            return Ok((*existing, false));
        }
        let id = InputId::new(self.next_input_id);
        self.next_input_id = self
            .next_input_id
            .checked_add(1)
            .ok_or_else(|| "Input identifier space was exhausted.".to_owned())?;
        self.input_ids.insert(token, id);
        Ok((id, true))
    }

    fn event_id(&self, index: usize) -> Result<EventId, String> {
        let session = self
            .score
            .as_ref()
            .ok_or_else(|| "No score is loaded.".to_owned())?;
        if index >= session.events().len() {
            return Err(format!("Score event index {index} is out of range."));
        }
        Ok(EventId::new(index as u64 + 1))
    }

    fn current_generation(&self) -> Result<Generation, String> {
        self.performance
            .generation()
            .ok_or_else(|| "No score is loaded.".to_owned())
    }

    fn require_ui_generation(&self, generation: u64) -> Result<Generation, String> {
        let current = self.current_generation()?;
        if current.get() == generation {
            Ok(current)
        } else {
            Err("That action belongs to a score that is no longer loaded.".to_owned())
        }
    }

    fn now(&self) -> SampleTime {
        SampleTime::new(self.audio.now_sample())
    }

    fn ensure_audio_ready(&mut self) -> Result<(), String> {
        if let Err(message) = self.audio.ensure_ready() {
            // Device loss is a safety boundary. Do not let a dead stream
            // advance the score, and silence MIDI OUT even when the internal
            // endpoint can no longer render its panic command.
            let at = self.now();
            if let Ok(transition) = self.performance.handle(PerformanceCommand::Panic { at }) {
                let _ = self.apply_transition(transition);
            }
            return Err(message);
        }
        Ok(())
    }

    fn apply_transition(&mut self, transition: Transition) -> Result<Option<CoreEventDto>, String> {
        let retry_target = match transition.event().copied() {
            Some(PerformanceEvent::Triggered {
                generation,
                event,
                kind: TriggerKind::Tap,
                ..
            }) => Some((generation, event)),
            _ => None,
        };
        let midi_clock_sample = self.audio.now_sample();
        let midi_clock_instant = Instant::now();
        for command in transition.audio_commands().copied() {
            // Beat Tap intentionally retains the pre-release behavior: its
            // quick automatic key-up must not engage the new piano damping
            // envelope. Rhythm Tap and audition/free-piano keep it.
            if self.beat_tap_mode
                && matches!(
                    command,
                    tapconductor_performance::AudioCommand::DampenGroup { .. }
                )
            {
                continue;
            }
            if let Err(error) = self.audio.send_performance_command(command) {
                self.recover_audio_delivery_failure(retry_target);
                return Err(format!(
                    "{error} Playback was stopped{} so the failed gesture can be retried safely.",
                    if retry_target.is_some() {
                        " and the score cursor was restored"
                    } else {
                        ""
                    }
                ));
            }
            self.midi
                .send_performance_command(command, midi_clock_sample, midi_clock_instant);
        }
        let event = match transition.event().copied() {
            Some(PerformanceEvent::ScoreReady { generation, .. }) => Some(CoreEventDto::Ready {
                generation: generation.get(),
            }),
            Some(PerformanceEvent::Triggered {
                generation,
                event,
                next,
                kind,
                ..
            }) => {
                let played_index = usize::try_from(event.get().saturating_sub(1)).ok();
                if kind == TriggerKind::Tap {
                    Some(CoreEventDto::Cursor {
                        generation: generation.get(),
                        index: next
                            .and_then(|id| usize::try_from(id.get().saturating_sub(1)).ok())
                            .unwrap_or_else(|| self.performance.cursor_index()),
                        played_index,
                    })
                } else {
                    None
                }
            }
            Some(PerformanceEvent::CursorMoved { generation, next }) => {
                Some(CoreEventDto::Cursor {
                    generation: generation.get(),
                    index: usize::try_from(next.get().saturating_sub(1)).unwrap_or(0),
                    played_index: None,
                })
            }
            Some(PerformanceEvent::Ignored {
                reason: IgnoreReason::EndOfScore,
            }) => Some(CoreEventDto::Ended {
                generation: self.current_generation()?.get(),
            }),
            _ => None,
        };
        Ok(event)
    }

    fn apply_direct_transition(
        &mut self,
        transition: Transition,
    ) -> Result<Option<CoreEventDto>, String> {
        let midi_clock_sample = self.audio.now_sample();
        let midi_clock_instant = Instant::now();
        for command in transition.audio_commands().copied() {
            self.audio.send_performance_command(command)?;
            self.midi
                .send_performance_command(command, midi_clock_sample, midi_clock_instant);
        }
        Ok(None)
    }

    fn recover_audio_delivery_failure(&mut self, retry_target: Option<(Generation, EventId)>) {
        let at = self.now();
        let recovery = match retry_target {
            Some((generation, event)) => self.performance.handle(PerformanceCommand::Reposition {
                generation,
                event,
                at,
            }),
            None => self.performance.handle(PerformanceCommand::Panic { at }),
        };
        let Ok(recovery) = recovery else {
            return;
        };
        let midi_clock_sample = self.audio.now_sample();
        let midi_clock_instant = Instant::now();
        for command in recovery.audio_commands().copied() {
            // Recovery transitions contain a dedicated atomic panic command;
            // it cannot be lost even when the ordinary audio queue is full.
            let _ = self.audio.send_performance_command(command);
            self.midi
                .send_performance_command(command, midi_clock_sample, midi_clock_instant);
        }
    }
}

fn direct_midi_sequence() -> ScoreSequence {
    let chord = Chord::from_midi_numbers(&[60]).expect("middle C is a valid MIDI chord");
    ScoreSequence::new(vec![Slice::new(EventId::new(1), chord)])
        .expect("a one-note direct MIDI sequence is valid")
}

fn velocity_from_midi1(value: u8) -> Result<Velocity, String> {
    if value == 0 || value > 127 {
        return Err("Tap velocity must be between 1 and 127.".to_owned());
    }
    Velocity::new(((u32::from(value) * u32::from(u16::MAX)) / 127) as u16)
        .ok_or_else(|| "Tap velocity must be non-zero.".to_owned())
}

fn sequence_from_events(events: &[tapconductor_score::TapEvent]) -> Result<ScoreSequence, String> {
    let mut slices = Vec::with_capacity(events.len());
    for (index, event) in events.iter().enumerate() {
        let mut attacks_by_staff: BTreeMap<
            (u16, tapconductor_score::Rational),
            Vec<&tapconductor_score::NoteAttack>,
        > = BTreeMap::new();
        for attack in &event.attacks {
            attacks_by_staff
                .entry((attack.staff, attack.end))
                .or_default()
                .push(attack);
        }
        let mut staff_groups = Vec::with_capacity(attacks_by_staff.len());
        for ((staff, end), attacks) in attacks_by_staff {
            let pitches: Vec<MidiPitch> = attacks
                .iter()
                .map(|attack| {
                    MidiPitch::new(attack.midi_pitch).ok_or_else(|| {
                        format!("Invalid MIDI pitch {} in score.", attack.midi_pitch)
                    })
                })
                .collect::<Result<_, _>>()?;
            let chord = Chord::from_pitches(&pitches).map_err(|error| error.to_string())?;
            let release_on = {
                events
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .find(|(_, candidate)| candidate.position.absolute >= end)
                    .map(|(release_index, _)| EventId::new(release_index as u64 + 1))
            };
            staff_groups.push(StaffSlice::new(
                staff,
                chord,
                tapconductor_performance::SliceReleaseBoundary::from_event(release_on),
            ));
        }
        slices.push(
            Slice::from_staff_groups(EventId::new(index as u64 + 1), &staff_groups)
                .map_err(|error| error.to_string())?,
        );
    }
    ScoreSequence::new(slices).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{MIN_TAP_INTERVAL, TapInputGate};
    use std::time::{Duration, Instant};

    #[test]
    fn tap_input_gate_ignores_taps_inside_sixty_milliseconds_without_extending_window() {
        let start = Instant::now();
        let mut gate = TapInputGate::default();

        assert!(gate.accept(start));
        assert!(!gate.accept(start + Duration::from_millis(59)));
        assert!(gate.accept(start + MIN_TAP_INTERVAL));
    }

    #[test]
    fn tap_input_gate_measures_the_next_window_from_the_last_non_ignored_tap() {
        let start = Instant::now();
        let mut gate = TapInputGate::default();

        assert!(gate.accept(start));
        assert!(!gate.accept(start + Duration::from_millis(30)));
        assert!(!gate.accept(start + Duration::from_millis(59)));
        assert!(gate.accept(start + Duration::from_millis(60)));
        assert!(!gate.accept(start + Duration::from_millis(119)));
        assert!(gate.accept(start + Duration::from_millis(120)));
    }
}

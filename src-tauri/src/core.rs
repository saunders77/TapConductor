use crate::{
    audio_runtime::AudioManager,
    dto::{CoreEventDto, LoadedScoreDto},
    midi_runtime::{MidiInputAction, MidiManager},
    session::ScoreSession,
};
use std::{collections::HashMap, path::PathBuf, sync::mpsc::Sender, time::Instant};
use tapconductor_performance::{
    Chord, EngineConfig, EventId, Generation, IgnoreReason, InputId, MidiPitch, PerformanceCommand,
    PerformanceEngine, PerformanceEvent, SampleRate, SampleTime, ScoreSequence, Slice, Transition,
    TriggerKind, Velocity,
};

pub struct AppCore {
    pub audio: AudioManager,
    pub midi: MidiManager,
    performance: PerformanceEngine,
    score: Option<ScoreSession>,
    input_ids: HashMap<String, InputId>,
    next_input_id: u64,
}

impl AppCore {
    pub fn new(midi_action_sender: Sender<MidiInputAction>) -> Result<Self, String> {
        let audio = AudioManager::new();
        let rate = SampleRate::new(audio.sample_rate())
            .ok_or_else(|| "Invalid audio sample rate.".to_owned())?;
        let performance = PerformanceEngine::with_default_gate(rate, EngineConfig::default())
            .map_err(|error| error.to_string())?;
        Ok(Self {
            midi: MidiManager::new(midi_action_sender, audio.sample_rate()),
            audio,
            performance,
            score: None,
            input_ids: HashMap::new(),
            next_input_id: 1,
        })
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
        let sample_rate = SampleRate::new(self.audio.sample_rate())
            .ok_or_else(|| "Invalid audio sample rate.".to_owned())?;
        self.performance
            .set_sample_rate(sample_rate)
            .map_err(|error| error.to_string())?;
        self.midi.set_sample_rate(sample_rate.get());
        Ok(event)
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
        let (input, inserted) = self.input_for_down(token.clone())?;
        let velocity = velocity_from_midi1(midi_velocity)?;
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
        let (input, inserted) = self.input_for_down(token.clone())?;
        let velocity = velocity_from_midi1(midi_velocity)?;
        let result = self
            .performance
            .handle(PerformanceCommand::Audition {
                generation: engine_generation,
                event,
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
        let transition = self
            .performance
            .handle(PerformanceCommand::InputReleased {
                input,
                at: self.now(),
            })
            .map_err(|error| error.to_string())?;
        // The engine has now accepted the physical release and removed its
        // latch. Keep the token if handle() rejects so a later release can retry.
        self.input_ids.remove(token);
        self.apply_transition(transition)
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
        self.apply_transition(transition)
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
        let pitches: Vec<MidiPitch> = event
            .attacks
            .iter()
            .map(|attack| {
                MidiPitch::new(attack.midi_pitch)
                    .ok_or_else(|| format!("Invalid MIDI pitch {} in score.", attack.midi_pitch))
            })
            .collect::<Result<_, _>>()?;
        let chord = Chord::from_pitches(&pitches).map_err(|error| error.to_string())?;
        slices.push(Slice::new(EventId::new(index as u64 + 1), chord));
    }
    ScoreSequence::new(slices).map_err(|error| error.to_string())
}

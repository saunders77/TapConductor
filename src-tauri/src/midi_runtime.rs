// Copyright (c) 2026 Michael Saunders
use crate::dto::{DeviceDto, MidiPortsDto, PianoShortcutCommandDto};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use tapconductor_midi::{
    MidiChannel, MidiInputConfig, MidiInputMapper, MidiMessage, MidiNote, MidiOutGroupId,
    MidiOutNote, MidiOutState, MidiTapEvent, Velocity,
    backend::{
        MidiBackend, MidiDeviceInfo, MidiInputConnection, MidiOutputConnection, MidirBackend,
    },
};
use tapconductor_performance as performance;

#[derive(Clone, Debug)]
pub enum MidiInputAction {
    Down {
        token: String,
        midi_pitch: u8,
        velocity: u8,
    },
    Up {
        token: String,
    },
    Shortcut {
        command: PianoShortcutCommandDto,
        token: String,
        pressed: bool,
    },
    Panic,
    Shutdown,
}

enum ShortcutGateOutput {
    Pass(MidiInputAction),
    Consume,
    Shortcut(MidiInputAction),
}

#[derive(Default)]
struct PianoShortcutGate {
    function_tokens: HashSet<String>,
    consumed_tokens: HashSet<String>,
    command_tokens: HashMap<String, PianoShortcutCommandDto>,
}

impl PianoShortcutGate {
    fn process(
        &mut self,
        action: MidiInputAction,
        function_pitch: u8,
        function_is_physically_held: bool,
    ) -> ShortcutGateOutput {
        match action {
            MidiInputAction::Down {
                token,
                midi_pitch,
                velocity,
            } => {
                if midi_pitch == function_pitch {
                    self.function_tokens.insert(token);
                    return ShortcutGateOutput::Consume;
                }
                if !function_is_physically_held {
                    return ShortcutGateOutput::Pass(MidiInputAction::Down {
                        token,
                        midi_pitch,
                        velocity,
                    });
                }
                self.consumed_tokens.insert(token.clone());
                let Some(command) = command_for_pitch(midi_pitch) else {
                    return ShortcutGateOutput::Consume;
                };
                self.command_tokens.insert(token.clone(), command);
                ShortcutGateOutput::Shortcut(MidiInputAction::Shortcut {
                    command,
                    token,
                    pressed: true,
                })
            }
            MidiInputAction::Up { token } => {
                if self.function_tokens.remove(&token) {
                    return ShortcutGateOutput::Consume;
                }
                if !self.consumed_tokens.remove(&token) {
                    return ShortcutGateOutput::Pass(MidiInputAction::Up { token });
                }
                let Some(command) = self.command_tokens.remove(&token) else {
                    return ShortcutGateOutput::Consume;
                };
                ShortcutGateOutput::Shortcut(MidiInputAction::Shortcut {
                    command,
                    token,
                    pressed: false,
                })
            }
            MidiInputAction::Panic => {
                self.function_tokens.clear();
                self.consumed_tokens.clear();
                self.command_tokens.clear();
                ShortcutGateOutput::Pass(MidiInputAction::Panic)
            }
            action => ShortcutGateOutput::Pass(action),
        }
    }
}

fn command_for_pitch(pitch: u8) -> Option<PianoShortcutCommandDto> {
    match pitch % 12 {
        4 => Some(PianoShortcutCommandDto::Forward),
        2 => Some(PianoShortcutCommandDto::Back),
        3 => Some(PianoShortcutCommandDto::Replay),
        1 => Some(PianoShortcutCommandDto::Beginning),
        11 => Some(PianoShortcutCommandDto::ToggleFreePlay),
        _ => None,
    }
}

enum OutputWorkerCommand {
    Play {
        due: Instant,
        group: MidiOutGroupId,
        note: MidiOutNote,
    },
    Release {
        due: Instant,
        group: MidiOutGroupId,
    },
    Panic,
    Shutdown,
}

struct OutputWorker {
    sender: Sender<OutputWorkerCommand>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for OutputWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(OutputWorkerCommand::Shutdown);
        // A disconnected hardware port can leave a native MIDI send or close
        // call stuck. Detach instead of blocking the application state lock.
        self.thread.take();
    }
}

impl OutputWorker {
    fn shutdown(mut self) -> Result<(), String> {
        // A closed command channel means the worker has already exited; it
        // still needs to be joined so its native connection is definitely
        // dropped before CoreMIDI is restarted.
        let _ = self.sender.send(OutputWorkerCommand::Shutdown);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        run_midi_operation("output shutdown", move || {
            thread
                .join()
                .map_err(|_| "The MIDI output scheduler panicked while stopping.".to_owned())
        })
    }
}

const MIDI_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MidiManager {
    backend: MidirBackend,
    input_action_sender: Sender<MidiInputAction>,
    input_connection: Option<Box<dyn MidiInputConnection>>,
    output_worker: Option<OutputWorker>,
    selected_input_id: Option<String>,
    selected_output_id: Option<String>,
    selected_input_name: Option<String>,
    selected_output_name: Option<String>,
    last_output_error: Option<String>,
    last_discovered_input_names: Vec<String>,
    last_discovered_output_names: Vec<String>,
    last_input_discovery_error: Option<String>,
    last_output_discovery_error: Option<String>,
    sample_rate: u32,
    shortcut_function_pitch: Arc<AtomicU8>,
}

impl MidiManager {
    pub fn new(input_action_sender: Sender<MidiInputAction>, sample_rate: u32) -> Self {
        Self {
            backend: MidirBackend,
            input_action_sender,
            input_connection: None,
            output_worker: None,
            selected_input_id: None,
            selected_output_id: None,
            selected_input_name: None,
            selected_output_name: None,
            last_output_error: None,
            last_discovered_input_names: Vec::new(),
            last_discovered_output_names: Vec::new(),
            last_input_discovery_error: None,
            last_output_discovery_error: None,
            sample_rate,
            shortcut_function_pitch: Arc::new(AtomicU8::new(36)),
        }
    }

    pub fn ports(&mut self) -> Result<MidiPortsDto, String> {
        let (input_result, output_result) = self.discover_ports()?;
        self.last_input_discovery_error = input_result.as_ref().err().cloned();
        self.last_output_discovery_error = output_result.as_ref().err().cloned();
        let inputs = input_result.unwrap_or_default();
        let outputs = output_result.unwrap_or_default();
        self.last_discovered_input_names =
            inputs.iter().map(|device| device.name.clone()).collect();
        self.last_discovered_output_names =
            outputs.iter().map(|device| device.name.clone()).collect();
        Ok(MidiPortsDto {
            inputs: inputs.into_iter().map(device_dto).collect(),
            outputs: outputs.into_iter().map(device_dto).collect(),
            selected_input: self.selected_input_id.clone(),
            selected_output: self.selected_output_id.clone(),
            input_discovery_error: self.last_input_discovery_error.clone(),
            output_discovery_error: self.last_output_discovery_error.clone(),
        })
    }

    #[cfg(target_os = "ios")]
    fn discover_ports(
        &self,
    ) -> Result<
        (
            Result<Vec<MidiDeviceInfo>, String>,
            Result<Vec<MidiDeviceInfo>, String>,
        ),
        String,
    > {
        // CoreMIDI enumeration is fast and synchronous. Keeping both queries on
        // this command thread avoids concurrent MIDIClientCreate calls on iOS.
        Ok((
            self.backend
                .input_devices()
                .map_err(|error| error.to_string()),
            self.backend
                .output_devices()
                .map_err(|error| error.to_string()),
        ))
    }

    #[cfg(not(target_os = "ios"))]
    fn discover_ports(
        &self,
    ) -> Result<
        (
            Result<Vec<MidiDeviceInfo>, String>,
            Result<Vec<MidiDeviceInfo>, String>,
        ),
        String,
    > {
        let backend = self.backend;
        let (input_sender, input_receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("tapconductor-midi-input-discovery".to_owned())
            .spawn(move || {
                let result = backend.input_devices().map_err(|error| error.to_string());
                let _ = input_sender.send(result);
            })
            .map_err(|error| format!("Unable to start MIDI input discovery: {error}"))?;

        let backend = self.backend;
        let (output_sender, output_receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("tapconductor-midi-output-discovery".to_owned())
            .spawn(move || {
                let result = backend.output_devices().map_err(|error| error.to_string());
                let _ = output_sender.send(result);
            })
            .map_err(|error| format!("Unable to start MIDI output discovery: {error}"))?;

        let deadline = Instant::now() + MIDI_OPERATION_TIMEOUT;
        let inputs = receive_midi_result(input_receiver, deadline, "input discovery");
        let outputs = receive_midi_result(output_receiver, deadline, "output discovery");
        Ok((inputs, outputs))
    }

    pub fn selected_input_name(&self) -> Option<String> {
        self.selected_input_name.clone()
    }

    pub fn selected_output_name(&self) -> Option<String> {
        self.selected_output_name.clone()
    }

    pub fn output_error(&self) -> Option<String> {
        self.last_output_error.clone()
    }

    pub fn discovered_input_names(&self) -> Vec<String> {
        self.last_discovered_input_names.clone()
    }

    pub fn discovered_output_names(&self) -> Vec<String> {
        self.last_discovered_output_names.clone()
    }

    pub fn input_discovery_error(&self) -> Option<String> {
        self.last_input_discovery_error.clone()
    }

    pub fn output_discovery_error(&self) -> Option<String> {
        self.last_output_discovery_error.clone()
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        debug_assert!(sample_rate > 0);
        self.sample_rate = sample_rate.max(1);
    }

    pub fn set_shortcut_function_pitch(&self, midi_pitch: u8) {
        self.shortcut_function_pitch
            .store(midi_pitch, Ordering::Relaxed);
    }

    pub fn set_input(&mut self, device_id: Option<String>) -> Result<(), String> {
        if let Some(connection) = self.input_connection.take() {
            // Native disconnect has no portable cancellation API. Keep it away
            // from the shared application-state lock.
            let _ = thread::Builder::new()
                .name("tapconductor-midi-input-close".to_owned())
                .spawn(move || drop(connection));
        }
        self.selected_input_id = None;
        self.selected_input_name = None;
        let _ = self.input_action_sender.send(MidiInputAction::Panic);

        let Some(device_id) = device_id.filter(|id| !id.is_empty()) else {
            return Ok(());
        };
        let discovery_id = device_id.clone();
        let devices = run_midi_operation("input discovery", move || {
            MidirBackend
                .input_devices()
                .map_err(|error| error.to_string())
        })?;
        let name = devices
            .into_iter()
            .find(|device| device.id == discovery_id)
            .map(|device| device.name)
            .ok_or_else(|| "The selected MIDI input is unavailable.".to_owned())?;
        let sender = self.input_action_sender.clone();
        let shortcut_function_pitch = self.shortcut_function_pitch.clone();
        // In normal rhythm/free-play modes, keep a MIDI key's input token
        // latched while CC64 sustain is down. The mapper emits its matching
        // Up when the pedal is released, so the existing performance gate
        // sustains the sounded score note (or direct MIDI note) without a
        // separate audio-only pedal path. Beat mode only consumes note-down
        // timing, so its tap behavior remains unchanged.
        let connection_id = device_id.clone();
        let connection = run_midi_operation("input connection", move || {
            let mut mapper = MidiInputMapper::<256>::new(MidiInputConfig {
                respect_sustain_pedal: true,
                ..MidiInputConfig::default()
            });
            let mut shortcut_gate = PianoShortcutGate::default();
            let mut physical_function_keys = HashSet::<(u8, u8)>::new();
            MidirBackend
                .connect_input(
                    &connection_id,
                    Box::new(move |message| {
                        let function_pitch = shortcut_function_pitch.load(Ordering::Relaxed);
                        match message.message {
                            MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity,
                            } if velocity.to_midi1() > 0 && note.get() == function_pitch => {
                                physical_function_keys.insert((channel.zero_based(), note.get()));
                            }
                            MidiMessage::NoteOff { channel, note, .. }
                            | MidiMessage::NoteOn {
                                channel,
                                note,
                                velocity: _,
                            } => {
                                physical_function_keys.remove(&(channel.zero_based(), note.get()));
                            }
                            _ => {}
                        }
                        let function_is_physically_held = physical_function_keys
                            .iter()
                            .any(|(_, pitch)| *pitch == function_pitch);
                        mapper.process(message, |event| {
                            let action = match event {
                                MidiTapEvent::Down {
                                    token,
                                    source_note,
                                    velocity,
                                    ..
                                } => MidiInputAction::Down {
                                    token: format!("midi:{}", token.0),
                                    midi_pitch: source_note.get(),
                                    velocity: velocity.to_midi1().max(1),
                                },
                                MidiTapEvent::Up { token, .. } => MidiInputAction::Up {
                                    token: format!("midi:{}", token.0),
                                },
                                MidiTapEvent::Panic { .. } => MidiInputAction::Panic,
                            };
                            match shortcut_gate.process(
                                action,
                                function_pitch,
                                function_is_physically_held,
                            ) {
                                ShortcutGateOutput::Pass(action)
                                | ShortcutGateOutput::Shortcut(action) => {
                                    let _ = sender.send(action);
                                }
                                ShortcutGateOutput::Consume => {}
                            }
                        });
                    }),
                )
                .map_err(|error| error.to_string())
        })?;
        self.input_connection = Some(connection);
        self.selected_input_id = Some(device_id);
        self.selected_input_name = Some(name);
        Ok(())
    }

    pub fn set_output(&mut self, device_id: Option<String>) -> Result<(), String> {
        self.output_worker.take();
        self.selected_output_id = None;
        self.selected_output_name = None;
        self.last_output_error = None;
        let Some(device_id) = device_id.filter(|id| !id.is_empty()) else {
            return Ok(());
        };
        let discovery_id = device_id.clone();
        let devices = run_midi_operation("output discovery", move || {
            MidirBackend
                .output_devices()
                .map_err(|error| error.to_string())
        })?;
        let name = devices
            .into_iter()
            .find(|device| device.id == discovery_id)
            .map(|device| device.name)
            .ok_or_else(|| "The selected MIDI output is unavailable.".to_owned())?;
        let connection_id = device_id.clone();
        let connection = run_midi_operation("output connection", move || {
            MidirBackend
                .connect_output(&connection_id)
                .map_err(|error| error.to_string())
        })?;
        self.output_worker = Some(spawn_output_worker(connection)?);
        self.selected_output_id = Some(device_id);
        self.selected_output_name = Some(name);
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let input = self.selected_input_id.clone();
        let output = self.selected_output_id.clone();
        let mut errors = Vec::new();

        // A process restart closes every CoreMIDI client before discovery.
        // Reproduce that lifecycle here so MIDIRestart does not rescan while
        // TapConductor still owns clients backed by the previous registry.
        if let Some(connection) = self.input_connection.take()
            && let Err(error) = run_midi_operation("input shutdown", move || {
                drop(connection);
                Ok(())
            })
        {
            errors.push(error);
        }
        if let Some(worker) = self.output_worker.take()
            && let Err(error) = worker.shutdown()
        {
            errors.push(error);
        }
        self.selected_input_id = None;
        self.selected_input_name = None;
        self.selected_output_id = None;
        self.selected_output_name = None;
        self.last_output_error = None;
        let _ = self.input_action_sender.send(MidiInputAction::Panic);

        if let Err(error) = self.backend.reload() {
            errors.push(error.to_string());
        }

        // MIDIRestart asks drivers to rescan, but registry notifications can
        // arrive after the call returns. Do not immediately recreate clients
        // against the pre-refresh endpoint snapshot.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        thread::sleep(Duration::from_millis(750));

        if let Err(error) = self.set_input(input) {
            errors.push(error);
        }
        if let Err(error) = self.set_output(output) {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join(" "))
        }
    }

    pub fn send_performance_command(
        &mut self,
        command: performance::AudioCommand,
        now_sample: u64,
        now_instant: Instant,
        output_velocity: Option<u8>,
    ) {
        if self.output_worker.is_none() {
            return;
        }
        let output_commands = match command {
            performance::AudioCommand::PlaySlice {
                at,
                group,
                chord,
                velocity,
                roll_interval_frames,
                roll_order,
            } => {
                if output_velocity == Some(0) {
                    return;
                }
                let velocity = output_velocity
                    .map(|value| {
                        Velocity::from_midi1(value).expect("velocity is clamped to MIDI 1")
                    })
                    .unwrap_or_else(|| Velocity::new(velocity.get()));
                let mut notes = Vec::with_capacity(chord.pitches().len());
                for pitch in chord.pitches() {
                    let note = match MidiNote::new(pitch.get()) {
                        Ok(note) => note,
                        Err(error) => {
                            self.disable_output(format!(
                                "MIDI OUT received an invalid score pitch ({error})"
                            ));
                            return;
                        }
                    };
                    notes.push(MidiOutNote { note, velocity });
                }
                if roll_order == performance::ChordRollOrder::AscendingPitch {
                    notes.sort_by_key(|note| note.note.get());
                }
                notes
                    .into_iter()
                    .enumerate()
                    .map(|(index, note)| OutputWorkerCommand::Play {
                        due: due_instant(
                            at.frame().saturating_add(
                                u64::from(roll_interval_frames).saturating_mul(index as u64),
                            ),
                            now_sample,
                            now_instant,
                            self.sample_rate,
                        ),
                        group: MidiOutGroupId(group.get()),
                        note,
                    })
                    .collect()
            }
            performance::AudioCommand::ReleaseGroup { at, group } => {
                vec![OutputWorkerCommand::Release {
                    due: due_instant(at.frame(), now_sample, now_instant, self.sample_rate),
                    group: MidiOutGroupId(group.get()),
                }]
            }
            // DampenGroup is generated for every physical input release,
            // including score taps, direct MIDI play, and all audition forms.
            // An external instrument has no equivalent of the sampler's
            // key-up envelope, so translate that boundary to MIDI Note Off.
            performance::AudioCommand::DampenGroup { at, group } => {
                vec![OutputWorkerCommand::Release {
                    due: due_instant(at.frame(), now_sample, now_instant, self.sample_rate),
                    group: MidiOutGroupId(group.get()),
                }]
            }
            performance::AudioCommand::Panic { .. } => vec![OutputWorkerCommand::Panic],
        };
        let send_failed = self.output_worker.as_ref().is_some_and(|worker| {
            output_commands
                .into_iter()
                .any(|command| worker.sender.send(command).is_err())
        });
        if send_failed {
            // Internal audio has already accepted this transition. Disable a
            // failed optional MIDI sink without turning a sounded tap into a
            // rejected command or suppressing its cursor event.
            self.disable_output("MIDI output scheduler stopped unexpectedly".to_owned());
        }
    }

    fn disable_output(&mut self, message: String) {
        self.output_worker.take();
        self.selected_output_id = None;
        self.selected_output_name = None;
        self.last_output_error = Some(format!("{message}; MIDI OUT was disabled."));
    }
}

fn device_dto(device: tapconductor_midi::backend::MidiDeviceInfo) -> DeviceDto {
    DeviceDto {
        id: device.id,
        name: device.name,
        is_default: false,
    }
}

fn receive_midi_result<T>(
    receiver: mpsc::Receiver<Result<T, String>>,
    deadline: Instant,
    operation: &str,
) -> Result<T, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    receiver.recv_timeout(remaining).map_err(|error| match error {
        RecvTimeoutError::Timeout => {
            format!("MIDI {operation} timed out after 5 seconds. Check the device connection, then reload devices.")
        }
        RecvTimeoutError::Disconnected => {
            format!("MIDI {operation} stopped before it returned a result.")
        }
    })?
}

fn run_midi_operation<T: Send + 'static>(
    operation: &'static str,
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name(format!("tapconductor-midi-{}", operation.replace(' ', "-")))
        .spawn(move || {
            let _ = sender.send(task());
        })
        .map_err(|error| format!("Unable to start MIDI {operation}: {error}"))?;
    receive_midi_result(receiver, Instant::now() + MIDI_OPERATION_TIMEOUT, operation)
}

fn due_instant(
    target_sample: u64,
    now_sample: u64,
    now_instant: Instant,
    sample_rate: u32,
) -> Instant {
    let frames = target_sample.saturating_sub(now_sample);
    now_instant + Duration::from_secs_f64(frames as f64 / f64::from(sample_rate.max(1)))
}

fn spawn_output_worker(connection: Box<dyn MidiOutputConnection>) -> Result<OutputWorker, String> {
    let (sender, receiver) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("tapconductor-midi-out".to_owned())
        .spawn(move || output_worker_loop(receiver, connection))
        .map_err(|error| format!("Unable to create the MIDI output scheduler: {error}"))?;
    Ok(OutputWorker {
        sender,
        thread: Some(thread),
    })
}

fn output_worker_loop(
    receiver: Receiver<OutputWorkerCommand>,
    mut output: Box<dyn MidiOutputConnection>,
) {
    let mut state = MidiOutState::<256>::default();
    let channel = MidiChannel::new(0).expect("MIDI channel one is valid");
    let mut pending: Vec<OutputWorkerCommand> = Vec::new();

    loop {
        pending.sort_by_key(command_due);
        let wait = pending
            .first()
            .and_then(command_due)
            .map(|due| due.saturating_duration_since(Instant::now()));
        let received = match wait {
            Some(duration) => receiver.recv_timeout(duration),
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(OutputWorkerCommand::Panic) => {
                pending.clear();
                let _ = state.panic(output.as_mut());
            }
            Ok(OutputWorkerCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                let _ = state.panic(output.as_mut());
                break;
            }
            Ok(command) => {
                if let OutputWorkerCommand::Release { due, group } = command {
                    pending.retain(|pending_command| {
                        !matches!(
                            pending_command,
                            OutputWorkerCommand::Play {
                                due: note_due,
                                group: note_group,
                                ..
                            } if *note_group == group && *note_due >= due
                        )
                    });
                }
                pending.push(command);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        let mut index = 0;
        while index < pending.len() {
            if command_due(&pending[index]).is_some_and(|due| due <= now) {
                let command = pending.remove(index);
                let result = match command {
                    OutputWorkerCommand::Play { group, note, .. } => {
                        state.play_group_note(output.as_mut(), group, channel, note)
                    }
                    OutputWorkerCommand::Release { group, .. } => {
                        match state.release_group(output.as_mut(), group) {
                            // A physical release now sends MIDI Note Off at
                            // DampenGroup. The performance engine can still
                            // deliver its later sampler ReleaseGroup; that
                            // stale duplicate is expected and harmless.
                            Err(tapconductor_midi::MidiOutError::UnknownGroup(_)) => Ok(()),
                            result => result,
                        }
                    }
                    OutputWorkerCommand::Panic | OutputWorkerCommand::Shutdown => Ok(()),
                };
                if result.is_err() {
                    // Attempt controller-based silence even after a partial
                    // chord write, then disconnect the scheduler. The next
                    // producer send observes the closed channel and disables
                    // MIDI OUT without rejecting internal audio playback.
                    let _ = state.panic(output.as_mut());
                    return;
                }
            } else {
                index += 1;
            }
        }
    }
}

fn command_due(command: &OutputWorkerCommand) -> Option<Instant> {
    match command {
        OutputWorkerCommand::Play { due, .. } | OutputWorkerCommand::Release { due, .. } => {
            Some(*due)
        }
        OutputWorkerCommand::Panic | OutputWorkerCommand::Shutdown => None,
    }
}

#[cfg(test)]
mod piano_shortcut_tests {
    use super::{MidiInputAction, PianoShortcutGate, ShortcutGateOutput, spawn_output_worker};
    use crate::dto::PianoShortcutCommandDto;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tapconductor_midi::{
        MidiMessage,
        backend::{MidiBackendError, MidiOutputConnection},
    };

    struct DropTrackingOutput(Arc<AtomicBool>);

    impl Drop for DropTrackingOutput {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    impl MidiOutputConnection for DropTrackingOutput {
        fn send(&mut self, _message: MidiMessage) -> Result<(), MidiBackendError> {
            Ok(())
        }
    }

    fn down(token: &str, pitch: u8) -> MidiInputAction {
        MidiInputAction::Down {
            token: token.to_owned(),
            midi_pitch: pitch,
            velocity: 96,
        }
    }

    #[test]
    fn maps_command_notes_by_pitch_class_and_swallows_releases() {
        let mut gate = PianoShortcutGate::default();
        assert!(matches!(
            gate.process(down("fn", 36), 36, true),
            ShortcutGateOutput::Consume
        ));
        let ShortcutGateOutput::Shortcut(MidiInputAction::Shortcut {
            command, pressed, ..
        }) = gate.process(down("command", 88), 36, true)
        else {
            panic!("E in any octave should produce a shortcut");
        };
        assert_eq!(command, PianoShortcutCommandDto::Forward);
        assert!(pressed);
        assert!(matches!(
            gate.process(
                MidiInputAction::Up {
                    token: "fn".to_owned()
                },
                36,
                false
            ),
            ShortcutGateOutput::Consume
        ));
        assert!(matches!(
            gate.process(
                MidiInputAction::Up {
                    token: "command".to_owned()
                },
                36,
                false
            ),
            ShortcutGateOutput::Shortcut(MidiInputAction::Shortcut { pressed: false, .. })
        ));
    }

    #[test]
    fn passes_notes_when_function_pitch_is_not_held() {
        let mut gate = PianoShortcutGate::default();
        assert!(matches!(
            gate.process(down("note", 64), 36, false),
            ShortcutGateOutput::Pass(_)
        ));
    }

    #[test]
    fn sustained_function_note_does_not_count_as_physically_held() {
        let mut gate = PianoShortcutGate::default();
        gate.process(down("fn", 36), 36, true);
        assert!(matches!(
            gate.process(down("note", 64), 36, false),
            ShortcutGateOutput::Pass(_)
        ));
    }

    #[test]
    fn orderly_output_shutdown_drops_the_native_connection_before_returning() {
        let dropped = Arc::new(AtomicBool::new(false));
        let worker = spawn_output_worker(Box::new(DropTrackingOutput(Arc::clone(&dropped))))
            .expect("output worker starts");

        worker.shutdown().expect("output worker stops");

        assert!(dropped.load(Ordering::Acquire));
    }
}

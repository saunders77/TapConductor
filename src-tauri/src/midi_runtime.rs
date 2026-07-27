use crate::dto::{DeviceDto, MidiPortsDto};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};
use tapconductor_midi::{
    MidiChannel, MidiInputConfig, MidiInputMapper, MidiNote, MidiOutGroupId, MidiOutNote,
    MidiOutState, MidiTapEvent, Velocity,
    backend::{MidiBackend, MidiInputConnection, MidiOutputConnection, MidirBackend},
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
    Panic,
    Shutdown,
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
    sample_rate: u32,
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
            sample_rate,
        }
    }

    pub fn ports(&self) -> Result<MidiPortsDto, String> {
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
        let inputs = receive_midi_result(input_receiver, deadline, "input discovery")?
            .into_iter()
            .map(device_dto)
            .collect();
        let outputs = receive_midi_result(output_receiver, deadline, "output discovery")?
            .into_iter()
            .map(device_dto)
            .collect();
        Ok(MidiPortsDto {
            inputs,
            outputs,
            selected_input: self.selected_input_id.clone(),
            selected_output: self.selected_output_id.clone(),
        })
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

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        debug_assert!(sample_rate > 0);
        self.sample_rate = sample_rate.max(1);
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
            MidirBackend
                .connect_input(
                    &connection_id,
                    Box::new(move |message| {
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
                            let _ = sender.send(action);
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
            } => {
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
                    notes.push(MidiOutNote {
                        note,
                        velocity: Velocity::new(velocity.get()),
                    });
                }
                notes.sort_by_key(|note| note.note.get());
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

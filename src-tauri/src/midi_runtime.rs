use crate::dto::{DeviceDto, MidiPortsDto};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};
use tapconductor_midi::{
    MidiChannel, MidiInputConfig, MidiInputMapper, MidiNote, MidiOutChord, MidiOutGroupId,
    MidiOutNote, MidiOutState, MidiTapEvent, Velocity,
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
        notes: Vec<MidiOutNote>,
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
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

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
        let inputs = self
            .backend
            .input_devices()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|device| DeviceDto {
                id: device.id,
                name: device.name,
                is_default: false,
            })
            .collect();
        let outputs = self
            .backend
            .output_devices()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|device| DeviceDto {
                id: device.id,
                name: device.name,
                is_default: false,
            })
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
        self.input_connection.take();
        self.selected_input_id = None;
        self.selected_input_name = None;
        let _ = self.input_action_sender.send(MidiInputAction::Panic);

        let Some(device_id) = device_id.filter(|id| !id.is_empty()) else {
            return Ok(());
        };
        let name = self
            .backend
            .input_devices()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|device| device.id == device_id)
            .map(|device| device.name)
            .ok_or_else(|| "The selected MIDI input is unavailable.".to_owned())?;
        let sender = self.input_action_sender.clone();
        let mut mapper = MidiInputMapper::<256>::new(MidiInputConfig::default());
        let connection = self
            .backend
            .connect_input(
                &device_id,
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
            .map_err(|error| error.to_string())?;
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
        let name = self
            .backend
            .output_devices()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|device| device.id == device_id)
            .map(|device| device.name)
            .ok_or_else(|| "The selected MIDI output is unavailable.".to_owned())?;
        let connection = self
            .backend
            .connect_output(&device_id)
            .map_err(|error| error.to_string())?;
        self.output_worker = Some(spawn_output_worker(connection)?);
        self.selected_output_id = Some(device_id);
        self.selected_output_name = Some(name);
        Ok(())
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
        // Damping is an internal piano-timbre change. It must not alter the
        // existing MIDI note-off schedule.
        if matches!(command, performance::AudioCommand::DampenGroup { .. }) {
            return;
        }
        let output_command = match command {
            performance::AudioCommand::PlaySlice {
                at,
                group,
                chord,
                velocity,
                roll_interval_frames: _,
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
                OutputWorkerCommand::Play {
                    due: due_instant(at.frame(), now_sample, now_instant, self.sample_rate),
                    group: MidiOutGroupId(group.get()),
                    notes,
                }
            }
            performance::AudioCommand::ReleaseGroup { at, group } => OutputWorkerCommand::Release {
                due: due_instant(at.frame(), now_sample, now_instant, self.sample_rate),
                group: MidiOutGroupId(group.get()),
            },
            performance::AudioCommand::Panic { .. } => OutputWorkerCommand::Panic,
            performance::AudioCommand::DampenGroup { .. } => unreachable!("handled above"),
        };
        let send_failed = self
            .output_worker
            .as_ref()
            .is_some_and(|worker| worker.sender.send(output_command).is_err());
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
            Ok(command) => pending.push(command),
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        let mut index = 0;
        while index < pending.len() {
            if command_due(&pending[index]).is_some_and(|due| due <= now) {
                let command = pending.remove(index);
                let result = match command {
                    OutputWorkerCommand::Play { group, notes, .. } => {
                        MidiOutChord::try_from_slice(&notes).and_then(|chord| {
                            state.play_group(output.as_mut(), group, channel, &chord)
                        })
                    }
                    OutputWorkerCommand::Release { group, .. } => {
                        state.release_group(output.as_mut(), group)
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

mod audio_runtime;
mod commands;
mod core;
mod dto;
mod midi_runtime;
mod session;

use crate::{core::AppCore, midi_runtime::MidiInputAction};
use std::sync::{Arc, Mutex, mpsc};
use tauri::Emitter;

pub struct AppState {
    core: Mutex<AppCore>,
}

pub fn run() {
    let (midi_sender, midi_receiver) = mpsc::channel();
    let midi_shutdown_sender = midi_sender.clone();
    let core = AppCore::new(midi_sender).expect("failed to initialize TapConductor core");
    let state = Arc::new(AppState {
        core: Mutex::new(core),
    });
    // The dispatch thread must not own the last strong application-state
    // reference: AppCore owns the channel sender, so a strong reference here
    // would form a shutdown cycle and keep the audio/MIDI connections alive.
    let setup_state = Arc::downgrade(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = midi_shutdown_sender.send(MidiInputAction::Shutdown);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_score,
            commands::set_part_enabled,
            commands::performance_input_down,
            commands::release_input,
            commands::audition_event,
            commands::audition_note,
            commands::set_cursor,
            commands::panic,
            commands::audio_devices,
            commands::set_audio_device,
            commands::set_volume,
            commands::midi_ports,
            commands::set_midi_input,
            commands::set_midi_output,
            commands::diagnostics,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let state = setup_state.clone();
            std::thread::Builder::new()
                .name("tapconductor-midi-input".to_owned())
                .spawn(move || {
                    while let Ok(action) = midi_receiver.recv() {
                        let Some(state) = state.upgrade() else {
                            break;
                        };
                        let result = state.core.lock().map_err(|_| {
                            "TapConductor's native state was poisoned after an unexpected failure."
                                .to_owned()
                        });
                        let event = match result {
                            Ok(mut core) => match action {
                                MidiInputAction::Down { token, velocity } => {
                                    core.input_down(token, velocity)
                                }
                                MidiInputAction::Up { token } => core.release_input(&token),
                                MidiInputAction::Panic => core.panic_midi_inputs(),
                                MidiInputAction::Shutdown => {
                                    let _ = core.panic();
                                    break;
                                }
                            },
                            Err(error) => Err(error),
                        };
                        match event {
                            Ok(Some(event)) => {
                                let _ = handle.emit("performance-event", event);
                            }
                            Ok(None) => {}
                            Err(message) => {
                                let _ = handle.emit(
                                    "performance-event",
                                    dto::CoreEventDto::Fault { message },
                                );
                            }
                        }
                    }
                })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run TapConductor");
}

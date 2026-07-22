mod audio_runtime;
mod commands;
mod core;
mod dto;
mod midi_runtime;
mod omr;
mod session;

use crate::{core::AppCore, midi_runtime::MidiInputAction};
use std::sync::{Arc, Mutex, mpsc};
use tauri::{Emitter, Manager};

pub struct AppState {
    core: Mutex<AppCore>,
    omr: Mutex<omr::OmrManager>,
}

pub fn handle_omr_export_callback() -> bool {
    omr::handle_export_callback_arguments(std::env::args_os())
}

pub fn run() {
    let (midi_sender, midi_receiver) = mpsc::channel();
    let midi_shutdown_sender = midi_sender.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = midi_shutdown_sender.send(MidiInputAction::Shutdown);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_score,
            commands::set_part_enabled,
            commands::set_roll_delays,
            commands::set_tap_mode,
            commands::performance_input_down,
            commands::release_input,
            commands::audition_event,
            commands::audition_note,
            commands::audition_chord,
            commands::set_cursor,
            commands::panic,
            commands::set_midi_free_play,
            commands::audio_devices,
            commands::set_audio_device,
            commands::set_volume,
            commands::midi_ports,
            commands::set_midi_input,
            commands::set_midi_output,
            commands::diagnostics,
            omr::omr_available,
            omr::recognize_pdf,
            omr::finish_omr_recognition,
            omr::discard_omr_recognition,
            omr::review_recognition,
            omr::omr_project_for_score,
            omr::poll_omr_export,
        ])
        .setup(move |app| {
            let resource_dir = app.path().resource_dir()?;
            let app_local_data_dir = app.path().app_local_data_dir()?;
            let salamander_directory = resource_dir.join("instruments").join("salamander");
            let core = AppCore::new(midi_sender, Some(&salamander_directory))
                .map_err(std::io::Error::other)?;
            let omr = omr::OmrManager::new(&resource_dir, &app_local_data_dir)
                .map_err(std::io::Error::other)?;
            let state = Arc::new(AppState {
                core: Mutex::new(core),
                omr: Mutex::new(omr),
            });
            // The dispatch thread must not own the last strong application-state
            // reference: AppCore owns the channel sender, so a strong reference
            // here would form a shutdown cycle.
            let setup_state = Arc::downgrade(&state);
            app.manage(state);
            let handle = app.handle().clone();
            std::thread::Builder::new()
                .name("tapconductor-midi-input".to_owned())
                .spawn(move || {
                    while let Ok(action) = midi_receiver.recv() {
                        let Some(state) = setup_state.upgrade() else {
                            break;
                        };
                        let result = state.core.lock().map_err(|_| {
                            "TapConductor's native state was poisoned after an unexpected failure."
                                .to_owned()
                        });
                        let event = match result {
                            Ok(mut core) => {
                                if core.beat_tap_mode() {
                                    match action {
                                        MidiInputAction::Down {
                                            token, velocity, ..
                                        } => {
                                            let _ = handle.emit(
                                                "beat-midi-input",
                                                dto::BeatMidiInputDto::Down { token, velocity },
                                            );
                                            Ok(None)
                                        }
                                        MidiInputAction::Up { token } => {
                                            let _ = handle.emit(
                                                "beat-midi-input",
                                                dto::BeatMidiInputDto::Up { token },
                                            );
                                            Ok(None)
                                        }
                                        MidiInputAction::Panic => core.panic_midi_inputs(),
                                        MidiInputAction::Shutdown => {
                                            let _ = core.panic();
                                            break;
                                        }
                                    }
                                } else {
                                    match action {
                                        MidiInputAction::Down {
                                            token,
                                            midi_pitch,
                                            velocity,
                                        } => {
                                            if core.midi_free_play() {
                                                core.direct_midi_down(token, midi_pitch, velocity)
                                            } else {
                                                core.input_down(token, velocity)
                                            }
                                        }
                                        MidiInputAction::Up { token } => core.release_input(&token),
                                        MidiInputAction::Panic => core.panic_midi_inputs(),
                                        MidiInputAction::Shutdown => {
                                            let _ = core.panic();
                                            break;
                                        }
                                    }
                                }
                            }
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

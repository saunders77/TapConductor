// Copyright (c) 2026 Michael Saunders
mod audio_runtime;
mod commands;
mod core;
mod crash_marker;
mod dto;
mod midi_runtime;
mod session;

use crate::{
    core::AppCore,
    crash_marker::{NativeTelemetryState, install_panic_marker},
    midi_runtime::MidiInputAction,
};
use std::sync::{Arc, Mutex, mpsc};
use tauri::{Emitter, Manager};
#[cfg(target_os = "ios")]
use tauri_plugin_apple_audio_session::AppleAudioSessionExt;

pub struct AppState {
    core: Mutex<AppCore>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (midi_sender, midi_receiver) = mpsc::channel();
    let midi_shutdown_sender = midi_sender.clone();

    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    #[cfg(target_os = "ios")]
    let builder = builder.plugin(tauri_plugin_apple_audio_session::init());

    builder
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = midi_shutdown_sender.send(MidiInputAction::Shutdown);
            }
            #[cfg(mobile)]
            match event {
                tauri::WindowEvent::Suspended => {
                    if let Some(state) = _window.try_state::<Arc<AppState>>() {
                        if let Ok(mut core) = state.core.lock() {
                            if let Ok(Some(event)) = core.suspend_audio() {
                                let _ = _window.emit("performance-event", event);
                            }
                        }
                    }
                    #[cfg(target_os = "ios")]
                    if let Err(error) = _window.apple_audio_session().deactivate() {
                        let _ = _window.emit("audio-lifecycle-error", error.to_string());
                    }
                }
                tauri::WindowEvent::Resumed => {
                    #[cfg(target_os = "ios")]
                    if let Err(error) = _window.apple_audio_session().activate() {
                        let _ = _window.emit("audio-lifecycle-error", error.to_string());
                        return;
                    }
                    if let Some(state) = _window.try_state::<Arc<AppState>>() {
                        if let Ok(mut core) = state.core.lock() {
                            if let Err(message) = core.resume_audio() {
                                let _ = _window.emit("audio-lifecycle-error", message);
                            }
                        }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_score,
            commands::load_demo_score,
            commands::set_part_enabled,
            commands::set_roll_delays,
            commands::set_tap_mode,
            commands::set_legato_mode,
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
            commands::reload_audio_systems,
            commands::set_instrument,
            commands::set_volume,
            commands::midi_ports,
            commands::set_midi_input,
            commands::set_midi_output,
            commands::diagnostics,
            commands::set_native_telemetry_consent,
            commands::take_native_crash_marker,
            commands::get_installer_telemetry_consent,
        ])
        .setup(move |app| {
            let marker_path = app.path().app_data_dir().ok().and_then(|app_data_dir| {
                std::fs::create_dir_all(&app_data_dir)
                    .ok()
                    .map(|()| app_data_dir.join("telemetry-crash-marker-v1"))
            });
            let native_telemetry = Arc::new(NativeTelemetryState::new(marker_path));
            install_panic_marker(native_telemetry.clone());
            app.manage(native_telemetry);
            let resource_dir = app.path().resource_dir()?;
            let salamander_directory = resource_dir.join("instruments").join("salamander");
            let core = AppCore::new(midi_sender, Some(&salamander_directory))
                .map_err(std::io::Error::other)?;
            let state = Arc::new(AppState {
                core: Mutex::new(core),
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

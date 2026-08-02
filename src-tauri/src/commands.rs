use crate::{
    AppState,
    core::AppCore,
    dto::{DeviceDto, DiagnosticsDto, LoadedScoreDto, MidiPortsDto},
};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

fn lock_core<'a>(
    state: &'a State<'_, Arc<AppState>>,
) -> Result<std::sync::MutexGuard<'a, AppCore>, String> {
    state.core.lock().map_err(|_| {
        "TapConductor's native state was poisoned after an unexpected failure.".to_owned()
    })
}

fn emit_event(app: &AppHandle, event: Option<crate::dto::CoreEventDto>) -> Result<(), String> {
    if let Some(event) = event {
        app.emit("performance-event", event)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn load_score(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    path: tauri_plugin_dialog::FilePath,
) -> Result<LoadedScoreDto, String> {
    let path = path
        .into_path()
        .map_err(|error| format!("The selected score is not a readable local file: {error}"))?;
    let (score, event) = lock_core(&state)?.load_score(path)?;
    emit_event(&app, event)?;
    Ok(score)
}

#[tauri::command]
pub fn load_demo_score(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    kind: String,
) -> Result<LoadedScoreDto, String> {
    let file_name = match kind.as_str() {
        "choir" => "All-Night Vigil - Rachmaninoff 1915.mxl",
        "piano" => "TapConductor-Demo.musicxml",
        _ => return Err(format!("Unknown demo score: {kind}")),
    };
    let path = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("demo")
        .join(file_name);
    let (score, event) = lock_core(&state)?.load_score(path)?;
    emit_event(&app, event)?;
    Ok(score)
}

#[tauri::command]
pub fn set_part_enabled(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    generation: u64,
    part_id: String,
    enabled: bool,
) -> Result<LoadedScoreDto, String> {
    let (score, event) = lock_core(&state)?.set_part_enabled(generation, &part_id, enabled)?;
    emit_event(&app, event)?;
    Ok(score)
}

#[tauri::command]
pub fn set_roll_delays(
    state: State<'_, Arc<AppState>>,
    regular_ms: u16,
    audition_ms: u16,
) -> Result<(), String> {
    lock_core(&state)?.set_roll_delays(regular_ms, audition_ms)
}

#[tauri::command]
pub fn set_tap_mode(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    beat: bool,
) -> Result<(), String> {
    let event = lock_core(&state)?.set_beat_tap_mode(beat)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn performance_input_down(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    token: String,
    velocity: u8,
) -> Result<(), String> {
    let event = lock_core(&state)?.input_down(token, velocity)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn release_input(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    token: String,
) -> Result<(), String> {
    let event = lock_core(&state)?.release_input(&token)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn audition_event(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    generation: u64,
    index: usize,
    token: String,
    velocity: u8,
) -> Result<(), String> {
    let event = lock_core(&state)?.audition(generation, index, token, velocity)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn audition_note(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    generation: u64,
    index: usize,
    midi_pitch: u8,
    token: String,
    velocity: u8,
) -> Result<(), String> {
    let event = lock_core(&state)?.audition_note(generation, index, midi_pitch, token, velocity)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn audition_chord(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    generation: u64,
    index: usize,
    midi_pitches: Vec<u8>,
    token: String,
    velocity: u8,
) -> Result<(), String> {
    let event =
        lock_core(&state)?.audition_chord(generation, index, midi_pitches, token, velocity)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn set_cursor(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    generation: u64,
    index: usize,
) -> Result<(), String> {
    let event = lock_core(&state)?.reposition(generation, index)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn panic(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let event = lock_core(&state)?.panic()?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn set_midi_free_play(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), String> {
    let event = lock_core(&state)?.set_midi_free_play(enabled)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn audio_devices(state: State<'_, Arc<AppState>>) -> Result<Vec<DeviceDto>, String> {
    lock_core(&state)?.audio.devices()
}

#[tauri::command]
pub fn set_audio_device(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let mut core = lock_core(&state)?;
    let event = core.set_audio_device((!id.is_empty()).then_some(id))?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn reload_audio_systems(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let event = lock_core(&state)?.reload_audio_systems()?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn set_instrument(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    instrument: String,
) -> Result<(), String> {
    let event = lock_core(&state)?.set_instrument(&instrument)?;
    emit_event(&app, event)
}

#[tauri::command]
pub fn set_volume(state: State<'_, Arc<AppState>>, value: f32) -> Result<(), String> {
    lock_core(&state)?.audio.set_volume(value)
}

#[tauri::command]
pub fn midi_ports(state: State<'_, Arc<AppState>>) -> Result<MidiPortsDto, String> {
    lock_core(&state)?.midi.ports()
}

#[tauri::command]
pub fn set_midi_input(state: State<'_, Arc<AppState>>, id: Option<String>) -> Result<(), String> {
    lock_core(&state)?.midi.set_input(id)
}

#[tauri::command]
pub fn set_midi_output(state: State<'_, Arc<AppState>>, id: Option<String>) -> Result<(), String> {
    lock_core(&state)?.midi.set_output(id)
}

#[tauri::command]
pub fn diagnostics(state: State<'_, Arc<AppState>>) -> Result<DiagnosticsDto, String> {
    let core = lock_core(&state)?;
    Ok(core.audio.diagnostics(
        core.midi.selected_input_name(),
        core.midi.selected_output_name(),
        core.midi.output_error(),
    ))
}

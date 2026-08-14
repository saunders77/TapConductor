// Copyright (c) 2026 Michael Saunders
use serde::Deserialize;
use std::collections::HashMap;
use tauri::AppHandle;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosMenuOption {
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacosMenuState {
    pub audio_outputs: Vec<MacosMenuOption>,
    pub instruments: Vec<MacosMenuOption>,
    pub midi_inputs: Vec<MacosMenuOption>,
    pub midi_outputs: Vec<MacosMenuOption>,
    pub parts: Vec<MacosMenuOption>,
    pub legato: bool,
    pub midi_free_play: bool,
    pub piano_shortcut_pitch: u8,
    pub score_loaded: bool,
    pub can_replay: bool,
    pub header_footer_visible: bool,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

impl Default for MacosMenuState {
    fn default() -> Self {
        Self {
            audio_outputs: vec![MacosMenuOption {
                label: "System default".to_owned(),
                selected: true,
            }],
            instruments: vec![
                MacosMenuOption {
                    label: "Piano".to_owned(),
                    selected: true,
                },
                MacosMenuOption {
                    label: "Synthesizer".to_owned(),
                    selected: false,
                },
            ],
            midi_inputs: vec![MacosMenuOption {
                label: "Off".to_owned(),
                selected: true,
            }],
            midi_outputs: vec![MacosMenuOption {
                label: "Off".to_owned(),
                selected: true,
            }],
            parts: Vec::new(),
            legato: false,
            midi_free_play: false,
            piano_shortcut_pitch: 36,
            score_loaded: false,
            can_replay: false,
            header_footer_visible: true,
            labels: HashMap::new(),
        }
    }
}

#[tauri::command]
pub fn sync_macos_menu(app: AppHandle, state: MacosMenuState) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    install(&app, &state).map_err(|error| error.to_string())?;
    #[cfg(not(target_os = "macos"))]
    let _ = (app, state);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn install(app: &AppHandle, state: &MacosMenuState) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

    fn append_choices(
        app: &AppHandle,
        parent: &Submenu<tauri::Wry>,
        prefix: &str,
        choices: &[MacosMenuOption],
        empty_label: &str,
    ) -> tauri::Result<()> {
        if choices.is_empty() {
            let empty = MenuItem::with_id(
                app,
                format!("macos-menu:{prefix}:empty"),
                empty_label,
                false,
                None::<&str>,
            )?;
            parent.append(&empty)?;
            return Ok(());
        }
        for (index, choice) in choices.iter().enumerate() {
            let item = CheckMenuItem::with_id(
                app,
                format!("macos-menu:{prefix}:{index}"),
                &choice.label,
                true,
                choice.selected,
                None::<&str>,
            )?;
            parent.append(&item)?;
        }
        Ok(())
    }

    let label = |key: &str, fallback: &'static str| -> &str {
        state
            .labels
            .get(key)
            .map(String::as_str)
            .unwrap_or(fallback)
    };

    let menu = Menu::new(app)?;

    let application = Submenu::new(app, "TapConductor", true)?;
    application.append(&PredefinedMenuItem::about(app, None, None)?)?;
    application.append(&PredefinedMenuItem::separator(app)?)?;
    application.append(&PredefinedMenuItem::services(app, None)?)?;
    application.append(&PredefinedMenuItem::separator(app)?)?;
    application.append(&PredefinedMenuItem::hide(app, None)?)?;
    application.append(&PredefinedMenuItem::hide_others(app, None)?)?;
    application.append(&PredefinedMenuItem::show_all(app, None)?)?;
    application.append(&PredefinedMenuItem::separator(app)?)?;
    application.append(&PredefinedMenuItem::quit(app, None)?)?;
    menu.append(&application)?;

    let file = Submenu::new(app, label("file", "File"), true)?;
    file.append(&MenuItem::with_id(
        app,
        "macos-menu:open-score",
        label("openScore", "Open A Score…"),
        true,
        Some("CmdOrCtrl+O"),
    )?)?;
    menu.append(&file)?;

    let view = Submenu::new(app, label("view", "View"), true)?;
    view.append(&MenuItem::with_id(
        app,
        "macos-menu:toggle-header-footer",
        if state.header_footer_visible {
            label("hideChrome", "Hide Header and Footer")
        } else {
            label("showChrome", "Show Header and Footer")
        },
        true,
        None::<&str>,
    )?)?;
    menu.append(&view)?;

    // There is deliberately no Edit menu. A score is performed and navigated,
    // not edited, so macOS' stock text-editing commands do not apply here.
    let audio = Submenu::new(app, label("audio", "Audio"), true)?;
    let audio_outputs = Submenu::new(app, label("audioOutput", "Audio Output"), true)?;
    append_choices(
        app,
        &audio_outputs,
        "audio-output",
        &state.audio_outputs,
        label("noneAvailable", "None available"),
    )?;
    audio.append(&audio_outputs)?;
    let instruments = Submenu::new(app, label("instrument", "Instrument"), true)?;
    append_choices(
        app,
        &instruments,
        "instrument",
        &state.instruments,
        label("noneAvailable", "None available"),
    )?;
    audio.append(&instruments)?;
    let midi_inputs = Submenu::new(app, label("midiIn", "MIDI IN"), true)?;
    append_choices(
        app,
        &midi_inputs,
        "midi-input",
        &state.midi_inputs,
        label("noneAvailable", "None available"),
    )?;
    audio.append(&midi_inputs)?;
    let midi_outputs = Submenu::new(app, label("midiOut", "MIDI OUT"), true)?;
    append_choices(
        app,
        &midi_outputs,
        "midi-output",
        &state.midi_outputs,
        label("noneAvailable", "None available"),
    )?;
    audio.append(&midi_outputs)?;
    audio.append(&PredefinedMenuItem::separator(app)?)?;
    audio.append(&MenuItem::with_id(
        app,
        "macos-menu:refresh-devices",
        label("refresh", "Refresh Audio & MIDI Devices"),
        true,
        None::<&str>,
    )?)?;
    audio.append(&PredefinedMenuItem::separator(app)?)?;
    audio.append(&CheckMenuItem::with_id(
        app,
        "macos-menu:toggle-legato",
        label("legato", "Legato"),
        true,
        state.legato,
        None::<&str>,
    )?)?;
    audio.append(&CheckMenuItem::with_id(
        app,
        "macos-menu:toggle-midi-free-play",
        label("playMidiDirect", "Play MIDI Input Directly"),
        true,
        state.midi_free_play,
        Some("CmdOrCtrl+Period"),
    )?)?;
    let parts = Submenu::new(app, label("parts", "Parts"), state.score_loaded)?;
    append_choices(
        app,
        &parts,
        "part",
        &state.parts,
        label("noneAvailable", "None available"),
    )?;
    audio.append(&parts)?;
    menu.append(&audio)?;

    let navigation = Submenu::new(app, label("navigation", "Navigation"), true)?;
    navigation.append(&MenuItem::with_id(
        app,
        "macos-menu:tap",
        label("tap", "TAP"),
        state.score_loaded,
        Some("Enter"),
    )?)?;
    navigation.append(&PredefinedMenuItem::separator(app)?)?;
    navigation.append(&MenuItem::with_id(
        app,
        "macos-menu:back",
        label("back", "Back"),
        state.score_loaded,
        Some("ArrowLeft"),
    )?)?;
    navigation.append(&MenuItem::with_id(
        app,
        "macos-menu:forward",
        label("forward", "Forward"),
        state.score_loaded,
        Some("ArrowRight"),
    )?)?;
    navigation.append(&MenuItem::with_id(
        app,
        "macos-menu:replay",
        label("replay", "Play Last Chord"),
        state.can_replay,
        Some("Space"),
    )?)?;
    navigation.append(&MenuItem::with_id(
        app,
        "macos-menu:beginning",
        label("beginning", "Go to Beginning of Score"),
        state.score_loaded,
        Some("CmdOrCtrl+ArrowLeft"),
    )?)?;
    menu.append(&navigation)?;

    let settings = Submenu::new(app, label("settings", "Settings"), true)?;
    let shortcut_note = Submenu::new(app, label("shortcutNote", "Piano Key Shortcut Note"), true)?;
    const NOTE_NAMES: [&str; 12] = [
        "C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B",
    ];
    for octave in -1..=9 {
        let first_pitch = ((octave + 1) * 12).max(0);
        let last_pitch = (((octave + 2) * 12) - 1).min(127);
        if first_pitch > last_pitch {
            continue;
        }
        let octave_menu = Submenu::new(
            app,
            label("octave", "Octave {octave}").replace("{octave}", &octave.to_string()),
            true,
        )?;
        for pitch in first_pitch..=last_pitch {
            let note = NOTE_NAMES[(pitch % 12) as usize];
            octave_menu.append(&CheckMenuItem::with_id(
                app,
                format!("macos-menu:piano-pitch:{pitch}"),
                format!("{note}{octave} (MIDI {pitch})"),
                true,
                pitch as u8 == state.piano_shortcut_pitch,
                None::<&str>,
            )?)?;
        }
        shortcut_note.append(&octave_menu)?;
    }
    settings.append(&shortcut_note)?;
    menu.append(&settings)?;

    let window = Submenu::new(app, label("window", "Window"), true)?;
    window.append(&PredefinedMenuItem::minimize(app, None)?)?;
    window.append(&PredefinedMenuItem::fullscreen(app, None)?)?;
    window.append(&PredefinedMenuItem::separator(app)?)?;
    window.append(&PredefinedMenuItem::bring_all_to_front(app, None)?)?;
    menu.append(&window)?;

    let help = Submenu::new(app, label("help", "Help"), true)?;
    help.append(&MenuItem::with_id(
        app,
        "macos-menu:info",
        label("info", "TapConductor Info"),
        true,
        Some("F1"),
    )?)?;
    menu.append(&help)?;

    app.set_menu(menu)?;
    Ok(())
}

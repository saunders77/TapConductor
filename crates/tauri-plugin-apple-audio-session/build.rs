// Copyright (c) 2026 Michael Saunders
const COMMANDS: &[&str] = &["activate", "deactivate"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}

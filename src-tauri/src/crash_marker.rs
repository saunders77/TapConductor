// Copyright (c) 2026 Michael Saunders
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MARKER_CONTENT: &[u8] = b"rust_panic_v1";

pub struct NativeTelemetryState {
    enabled: AtomicBool,
    marker_path: Option<PathBuf>,
}

impl NativeTelemetryState {
    pub fn new(marker_path: Option<PathBuf>) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            marker_path,
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.enabled.store(enabled, Ordering::Release);
        if !enabled
            && let Some(marker_path) = &self.marker_path
            && marker_path.exists()
        {
            fs::remove_file(marker_path).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn take_marker(&self) -> Result<bool, String> {
        let Some(marker_path) = &self.marker_path else {
            return Ok(false);
        };
        if !self.enabled.load(Ordering::Acquire) || !marker_path.exists() {
            return Ok(false);
        }
        let recognized = fs::read(marker_path)
            .map(|content| content == MARKER_CONTENT)
            .unwrap_or(false);
        fs::remove_file(marker_path).map_err(|error| error.to_string())?;
        Ok(recognized)
    }
}

pub fn install_panic_marker(state: Arc<NativeTelemetryState>) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        if state.enabled.load(Ordering::Acquire)
            && let Some(marker_path) = &state.marker_path
        {
            // The fixed marker deliberately excludes panic text,
            // locations, backtraces, paths, and application/user data.
            let _ = fs::write(marker_path, MARKER_CONTENT);
        }
        previous_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_state_removes_and_never_consumes_marker() {
        let marker = std::env::temp_dir().join(format!(
            "tapconductor-crash-marker-test-{}",
            std::process::id()
        ));
        fs::write(&marker, MARKER_CONTENT).expect("write marker");
        let state = NativeTelemetryState::new(Some(marker.clone()));
        state.set_enabled(false).expect("disable");
        assert!(!marker.exists());
        assert!(!state.take_marker().expect("take"));
    }

    #[test]
    fn enabled_state_consumes_only_fixed_marker() {
        let marker = std::env::temp_dir().join(format!(
            "tapconductor-crash-marker-valid-test-{}",
            std::process::id()
        ));
        let state = NativeTelemetryState::new(Some(marker.clone()));
        state.set_enabled(true).expect("enable");
        fs::write(&marker, MARKER_CONTENT).expect("write marker");
        assert!(state.take_marker().expect("take"));
        assert!(!marker.exists());
    }
}

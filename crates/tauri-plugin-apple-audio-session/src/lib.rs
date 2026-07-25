use serde::{Deserialize, Serialize};
use tauri::{
    Manager, Runtime,
    plugin::{Builder, TauriPlugin},
};

#[cfg(not(target_os = "ios"))]
mod desktop;
mod error;
#[cfg(target_os = "ios")]
mod mobile;

#[cfg(not(target_os = "ios"))]
pub use desktop::AppleAudioSession;
pub use error::{Error, Result};
#[cfg(target_os = "ios")]
pub use mobile::AppleAudioSession;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub sample_rate: f64,
    pub io_buffer_duration: f64,
    pub output_channels: usize,
    pub route: String,
}

pub trait AppleAudioSessionExt<R: Runtime> {
    fn apple_audio_session(&self) -> &AppleAudioSession<R>;
}

impl<R: Runtime, T: Manager<R>> AppleAudioSessionExt<R> for T {
    fn apple_audio_session(&self) -> &AppleAudioSession<R> {
        self.state::<AppleAudioSession<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("apple-audio-session")
        .setup(|app, api| {
            #[cfg(target_os = "ios")]
            let session = mobile::init(app, api)?;
            #[cfg(not(target_os = "ios"))]
            let session = desktop::init(app, api)?;
            app.manage(session);
            Ok(())
        })
        .build()
}

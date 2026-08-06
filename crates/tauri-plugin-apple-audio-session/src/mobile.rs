// Copyright (c) 2026 Michael Saunders
use tauri::{
    AppHandle, Runtime,
    plugin::{PluginApi, PluginHandle},
};

tauri::ios_plugin_binding!(init_plugin_apple_audio_session);

pub fn init<R: Runtime>(
    _app: &AppHandle<R>,
    api: PluginApi<R, ()>,
) -> crate::Result<AppleAudioSession<R>> {
    let handle = api.register_ios_plugin(init_plugin_apple_audio_session)?;
    let session = AppleAudioSession(handle);
    session.activate()?;
    Ok(session)
}

pub struct AppleAudioSession<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> AppleAudioSession<R> {
    pub fn activate(&self) -> crate::Result<crate::SessionInfo> {
        self.0.run_mobile_plugin("activate", ()).map_err(Into::into)
    }

    pub fn deactivate(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("deactivate", ())
            .map_err(Into::into)
    }
}

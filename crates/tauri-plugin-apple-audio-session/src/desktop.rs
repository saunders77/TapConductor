use tauri::{AppHandle, Runtime, plugin::PluginApi};

pub fn init<R: Runtime>(
    app: &AppHandle<R>,
    _api: PluginApi<R, ()>,
) -> crate::Result<AppleAudioSession<R>> {
    Ok(AppleAudioSession { _app: app.clone() })
}

pub struct AppleAudioSession<R: Runtime> {
    _app: AppHandle<R>,
}

impl<R: Runtime> AppleAudioSession<R> {
    pub fn activate(&self) -> crate::Result<crate::SessionInfo> {
        Ok(crate::SessionInfo::default())
    }

    pub fn deactivate(&self) -> crate::Result<()> {
        Ok(())
    }
}

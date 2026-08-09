// Copyright (c) 2026 Michael Saunders

#[cfg(target_os = "macos")]
pub fn synchronize_native_appearance(window: &tauri::Window) -> tauri::Result<()> {
    synchronize_native_appearance_for_ns_window(window.ns_window()?);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn synchronize_native_appearance_for_ns_window(ns_window: *mut std::ffi::c_void) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAppearanceCustomization, NSApplication, NSWindow};

    // Keep the title-bar material and all of its native foreground elements
    // under one AppKit appearance. AppKit then chooses and renders the window
    // title color itself; no private title view or hard-coded color is used.
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let appearance = NSApplication::sharedApplication(main_thread).effectiveAppearance();
    let ns_window = unsafe { &*(ns_window as *const NSWindow) };
    ns_window.setAppearance(Some(&appearance));
    ns_window.displayIfNeeded();
}

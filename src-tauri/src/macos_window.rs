// Copyright (c) 2026 Michael Saunders

#[cfg(target_os = "macos")]
pub fn force_light_appearance_for_ns_window(ns_window: *mut std::ffi::c_void) {
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSWindow};

    // Keep macOS window chrome permanently in the standard Aqua appearance.
    // AppKit owns the native dark title rendering and antialiasing.
    let aqua_name = unsafe { NSAppearanceNameAqua };
    let Some(appearance) = NSAppearance::appearanceNamed(aqua_name) else {
        return;
    };
    let ns_window = unsafe { &*(ns_window as *const NSWindow) };
    ns_window.setAppearance(Some(&appearance));
    ns_window.displayIfNeeded();
}

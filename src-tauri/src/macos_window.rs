// Copyright (c) 2026 Michael Saunders

#[cfg(target_os = "macos")]
pub fn ensure_title_contrast(window: &tauri::Window) -> tauri::Result<()> {
    ensure_title_contrast_for_ns_window(window.ns_window()?);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn ensure_title_contrast_for_ns_window(ns_window: *mut std::ffi::c_void) {
    use objc2_app_kit::{NSColor, NSTextField, NSView, NSWindow, NSWindowButton};

    fn apply_semantic_title_color(view: &NSView, title: &str) -> bool {
        if let Some(field) = view.downcast_ref::<NSTextField>()
            && field.stringValue().to_string() == title
        {
            // labelColor is an AppKit semantic color. AppKit resolves it
            // against this view's effective appearance, so it remains
            // contrasting in Light, Dark, high-contrast, active, and
            // inactive title-bar states without a hard-coded RGB value.
            field.setTextColor(Some(&NSColor::labelColor()));
            return true;
        }

        for child in view.subviews().iter() {
            if apply_semantic_title_color(&child, title) {
                return true;
            }
        }
        false
    }

    // Tauri exposes the native NSWindow pointer specifically for platform
    // integration. Start at the title-bar button container instead of walking
    // through the WKWebView hierarchy.
    let ns_window = unsafe { &*(ns_window as *const NSWindow) };
    let title = ns_window.title().to_string();
    if let Some(close_button) = ns_window.standardWindowButton(NSWindowButton::CloseButton)
        && let Some(title_bar) = unsafe { close_button.superview() }
    {
        apply_semantic_title_color(&title_bar, &title);
    }
}

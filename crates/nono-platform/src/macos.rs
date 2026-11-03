//! macOS-specific window and application tweaks.

#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSWindowCollectionBehavior};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// A desktop pet should live in the menu bar, not in the Dock.
pub fn set_accessory_activation_policy() {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("not on the main thread, cannot change activation policy");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if !app.setActivationPolicy(NSApplicationActivationPolicy::Accessory) {
        tracing::warn!("failed to set accessory activation policy");
    }
}

/// Removes the drop shadow and keeps the pet visible on every Space.
pub fn tune_window(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the pointer belongs to the window that is alive for this call.
    let view = unsafe { &*(appkit.ns_view.as_ptr() as *const NSView) };
    if let Some(ns_window) = view.window() {
        ns_window.setHasShadow(false);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary,
        );
    }
}

/// Diagnostic dump of the coordinate spaces involved in window placement.
pub fn log_geometry(window: &Window) {
    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return };
    let view = unsafe { &*(appkit.ns_view.as_ptr() as *const NSView) };
    let Some(ns_window) = view.window() else { return };
    let frame = ns_window.frame();
    let content = ns_window.contentRectForFrameRect(frame);
    let screen = ns_window.screen();
    let screen_frame = screen.as_ref().map(|s| s.frame());
    let visible = screen.as_ref().map(|s| s.visibleFrame());
    let scale = screen.as_ref().map(|s| s.backingScaleFactor());
    tracing::debug!(
        winit_outer = ?window.outer_position(),
        winit_inner = ?window.inner_size(),
        ns_frame = ?(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height),
        ns_content = ?(content.origin.x, content.origin.y, content.size.width, content.size.height),
        screen = ?screen_frame.map(|f| (f.origin.x, f.origin.y, f.size.width, f.size.height)),
        visible = ?visible.map(|f| (f.origin.x, f.origin.y, f.size.width, f.size.height)),
        backing = ?scale,
        "window geometry"
    );
}

pub fn screen_geometry() -> Option<(f64, f64, f64, f64)> {
    let screen = NSScreen::mainScreen(objc2::MainThreadMarker::new()?)?;
    let frame = screen.frame();
    Some((frame.origin.x, frame.origin.y, frame.size.width, frame.size.height))
}

/// Usable screen area (excludes the menu bar and the Dock) in **top-left
/// physical pixels**, matching winit's coordinate space.
///
/// M1 assumes the primary display; multi-display visible frames need the anchor
/// to be mapped to the right screen first.
pub fn visible_bounds() -> Option<nono_core::Rect> {
    let mtm = objc2::MainThreadMarker::new()?;
    let screen = NSScreen::mainScreen(mtm)?;
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    let scale = screen.backingScaleFactor() as f32;
    // AppKit uses a bottom-left origin; convert to winit's top-left space.
    let top = frame.size.height - (visible.origin.y + visible.size.height);
    Some(nono_core::Rect::new(
        visible.origin.x as f32 * scale,
        top as f32 * scale,
        visible.size.width as f32 * scale,
        visible.size.height as f32 * scale,
    ))
}

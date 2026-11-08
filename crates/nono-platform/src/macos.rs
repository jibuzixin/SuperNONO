//! macOS-specific window and application tweaks.

#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSScreen, NSView,
    NSWindowCollectionBehavior,
};
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use objc2_foundation::{ns_string, NSNumber};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use nono_core::{Display, Point, Rect};

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
            NSWindowCollectionBehavior::CanJoinAllSpaces | NSWindowCollectionBehavior::Stationary,
        );
    }
}

/// Diagnostic dump of the coordinate spaces involved in window placement.
pub fn log_geometry(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let view = unsafe { &*(appkit.ns_view.as_ptr() as *const NSView) };
    let Some(ns_window) = view.window() else {
        return;
    };
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
    Some((
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    ))
}

/// All connected displays with their usable area in **logical global points**
/// (top-left origin of the primary display, y growing downwards).
///
/// `NSScreen.frame`/`visibleFrame` are in AppKit points with a bottom-left
/// origin.  The primary display owns the global origin, so only the primary
/// screen's height is needed to flip Y.  Keeping points (not pixels) makes
/// screens with different backing scales line up exactly like the system's
/// own coordinate space.
pub fn displays() -> Option<Vec<Display>> {
    let mtm = MainThreadMarker::new()?;
    let main_height = CGDisplayBounds(CGMainDisplayID()).size.height;
    let main_id = CGMainDisplayID();
    let mut displays = Vec::new();
    for screen in NSScreen::screens(mtm).into_iter() {
        let Some(id) = screen_number(&screen) else {
            tracing::warn!("display without NSScreenNumber ignored");
            continue;
        };
        let visible = screen.visibleFrame();
        let bounds = Rect::new(
            visible.origin.x as f32,
            (main_height - (visible.origin.y + visible.size.height)) as f32,
            visible.size.width as f32,
            visible.size.height as f32,
        );
        displays.push(Display {
            id,
            name: screen.localizedName().to_string(),
            bounds,
            scale: screen.backingScaleFactor() as f32,
            primary: id == main_id,
        });
    }
    if displays.is_empty() {
        return None;
    }
    // Primary first, then reading order, so the tray list is predictable.
    displays.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(
                a.bounds
                    .x
                    .partial_cmp(&b.bounds.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                a.bounds
                    .y
                    .partial_cmp(&b.bounds.y)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    Some(displays)
}

fn screen_number(screen: &NSScreen) -> Option<u32> {
    let description = screen.deviceDescription();
    let value = description.objectForKey(ns_string!("NSScreenNumber"))?;
    let number = value.downcast_ref::<NSNumber>()?;
    Some(number.unsignedIntValue())
}

/// Current mouse position in **logical global points**, matching [`displays`].
///
/// `NSEvent::mouseLocation` is already in the global AppKit point space; the
/// only conversion needed is flipping Y through the primary screen's height.
/// Do **not** multiply by a backing scale here: on a mixed-DPI setup each
/// screen has its own scale and the global point space is the continuous one.
pub fn cursor_position() -> Option<Point> {
    let main_height = CGDisplayBounds(CGMainDisplayID()).size.height;
    let location = NSEvent::mouseLocation();
    Some(Point::new(
        location.x as f32,
        (main_height - location.y) as f32,
    ))
}

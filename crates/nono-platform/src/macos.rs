//! macOS-specific window and application tweaks.

#![cfg(target_os = "macos")]

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSApplicationActivationPolicy, NSColor,
    NSColorPanel, NSColorPanelMode, NSColorSpace, NSControl, NSEvent, NSScreen, NSTextField,
    NSView, NSWindow, NSWindowCollectionBehavior,
};
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use objc2_foundation::{ns_string, NSNumber, NSSize, NSString};
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

// ── custom tint colour picker ───────────────────────────────────────

fn color_panel_window(panel: &NSColorPanel) -> &NSWindow {
    // NSColorPanel is an NSPanel/NSWindow subclass; the pointer cast only
    // accesses inherited window methods.
    unsafe { &*(panel as *const NSColorPanel as *const NSWindow) }
}

fn color_channel(value: f64) -> u8 {
    (value * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Opens the shared macOS colour panel (colour wheel / RGB sliders).
#[allow(deprecated)]
pub fn show_color_panel(initial: Option<[u8; 3]>) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    let panel = NSColorPanel::sharedColorPanel(mtm);
    if let Some([r, g, b]) = initial {
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            r as f64 / 255.0,
            g as f64 / 255.0,
            b as f64 / 255.0,
            1.0,
        );
        panel.setColor(&color);
    }
    panel.setMode(NSColorPanelMode::Wheel);
    panel.orderFront(None);
    true
}

pub fn color_panel_visible() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let panel = NSColorPanel::sharedColorPanel(mtm);
    color_panel_window(&panel).isVisible()
}

pub fn color_panel_color() -> Option<[u8; 3]> {
    let mtm = MainThreadMarker::new()?;
    let panel = NSColorPanel::sharedColorPanel(mtm);
    let color = panel.color();
    let rgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some([
        color_channel(rgb.redComponent()),
        color_channel(rgb.greenComponent()),
        color_channel(rgb.blueComponent()),
    ])
}

pub fn hide_color_panel() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let panel = NSColorPanel::sharedColorPanel(mtm);
    color_panel_window(&panel).orderOut(None);
}

fn parse_color_text(text: &str) -> Option<[u8; 3]> {
    let trimmed = text.trim();
    if trimmed.contains(',') {
        let parts = trimmed
            .split(',')
            .map(|part| part.trim().parse::<u8>().ok())
            .collect::<Option<Vec<_>>>()?;
        if parts.len() == 3 {
            return Some([parts[0], parts[1], parts[2]]);
        }
        return None;
    }
    let hex = trimmed.trim_start_matches('#');
    let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok();
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        3 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 => Some([pair(0)?, pair(2)?, pair(4)?]),
        _ => None,
    }
}

/// Shows a small native input dialog for an RGB hex value.
///
/// Returns `None` when cancelled; re-prompts when the text is not a valid
/// colour so the user gets immediate feedback.
#[allow(deprecated)]
pub fn prompt_hex_color(initial: Option<[u8; 3]>) -> Option<[u8; 3]> {
    let mtm = MainThreadMarker::new()?;
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);

    let alert = NSAlert::new(mtm);
    alert.setMessageText(ns_string!("自定义 NONO 颜色"));
    alert.addButtonWithTitle(ns_string!("应用"));
    alert.addButtonWithTitle(ns_string!("取消"));

    let field = NSTextField::new(mtm);
    field.setFrameSize(NSSize::new(260.0, 24.0));
    field.setEditable(true);
    field.setSelectable(true);
    field.setBezeled(true);
    field.setDrawsBackground(true);
    let initial_text = initial
        .map(|[r, g, b]| format!("#{r:02X}{g:02X}{b:02X}"))
        .unwrap_or_else(|| "#FFFFFF".to_string());
    field.setStringValue(&NSString::from_str(&initial_text));
    field.setPlaceholderString(Some(ns_string!("#RRGGBB")));
    let field_view = unsafe { &*(&*field as *const NSTextField as *const NSView) };
    alert.setAccessoryView(Some(field_view));

    let mut hint = "输入 #RRGGBB 或 RRGGBB；也支持 #RGB 简写。".to_string();
    loop {
        alert.setInformativeText(&NSString::from_str(&hint));
        if alert.runModal() != NSAlertFirstButtonReturn {
            return None;
        }
        let control = unsafe { &*(&*field as *const NSTextField as *const NSControl) };
        let text = control.stringValue().to_string();
        if let Some(rgb) = parse_color_text(&text) {
            return Some(rgb);
        }
        hint = "格式无效，请输入例如 #FF5A96、FF5A96 或 #F59。".to_string();
    }
}

#[cfg(test)]
mod color_tests {
    use super::parse_color_text;

    #[test]
    fn parses_hex_and_rgb_inputs() {
        assert_eq!(parse_color_text("#FF5A96"), Some([255, 90, 150]));
        assert_eq!(parse_color_text("ff5a96"), Some([255, 90, 150]));
        assert_eq!(parse_color_text("  #F59 "), Some([255, 85, 153]));
        assert_eq!(parse_color_text("255, 90, 150"), Some([255, 90, 150]));
        assert_eq!(parse_color_text("#GGGGGG"), None);
        assert_eq!(parse_color_text("#FF5A9"), None);
        assert_eq!(parse_color_text(""), None);
    }
}

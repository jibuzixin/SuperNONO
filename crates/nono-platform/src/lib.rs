//! Platform glue: one transparent frameless window, a native tray menu, and a
//! presenter that can blit premultiplied RGBA frames with alpha.
//!
//! macOS is the reference implementation.  `Presenter` is a trait so a Windows
//! (layered window) or Linux (ARGB visual) backend can be added without touching
//! the application layer.

pub mod audio;
pub mod icon;
pub mod presenter;
pub mod tray;
pub mod window;

#[cfg(target_os = "macos")]
pub mod macos;

pub use audio::AudioPlayer;
pub use presenter::{create_presenter, Presenter, PresenterError};
pub use tray::{build_tray, TrayError, TrayMenu};
pub use window::create_pet_window;

use nono_core::Display;

/// Connected displays, primary first, with usable bounds in **logical global
/// points** (top-left origin of the primary display).
///
/// Returns `None` on platforms that do not expose per-screen geometry yet; the
/// application then falls back to winit's monitor list.
pub fn displays() -> Option<Vec<Display>> {
    #[cfg(target_os = "macos")]
    {
        macos::displays()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Current mouse position in **logical global points** (`None` if unavailable).
pub fn cursor_position() -> Option<nono_core::Point> {
    #[cfg(target_os = "macos")]
    {
        macos::cursor_position()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Opens a native colour picker for the custom body tint.
pub fn show_color_panel(initial: Option<[u8; 3]>) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::show_color_panel(initial)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = initial;
        false
    }
}

/// Whether the native colour panel is currently on screen.
pub fn color_panel_visible() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::color_panel_visible()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Final colour selected in the native colour panel, as sRGB.
pub fn color_panel_color() -> Option<[u8; 3]> {
    #[cfg(target_os = "macos")]
    {
        macos::color_panel_color()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn hide_color_panel() {
    #[cfg(target_os = "macos")]
    {
        macos::hide_color_panel();
    }
}

/// Native text prompt for `#RRGGBB` / `RRGGBB` / `#RGB` input.
pub fn prompt_hex_color(initial: Option<[u8; 3]>) -> Option<[u8; 3]> {
    #[cfg(target_os = "macos")]
    {
        macos::prompt_hex_color(initial)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = initial;
        None
    }
}

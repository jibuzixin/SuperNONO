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

/// Usable area of the display NONO is on, in top-left physical pixels.
/// Returns `None` on platforms that do not expose it yet.
pub fn screen_visible_bounds() -> Option<nono_core::Rect> {
    #[cfg(target_os = "macos")]
    {
        macos::visible_bounds()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Current mouse position in top-left physical pixels (`None` if unavailable).
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

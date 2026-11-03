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

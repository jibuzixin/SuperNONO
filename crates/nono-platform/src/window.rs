//! Creates the single pet window.
//!
//! The window is frameless, transparent, always on top and exactly the size of
//! the current animation canvas.  Its position is derived from the pet anchor so
//! switching between animations does not make the character jump.
//!
//! `rect` is in **logical global points**; winit converts it to the platform's
//! window coordinates.  Using logical positions (instead of physical pixels)
//! avoids the per-monitor backing scale ambiguity when the pet crosses displays
//! with different DPI.

use nono_core::Rect;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowLevel};

pub fn create_pet_window(
    event_loop: &ActiveEventLoop,
    rect: Rect,
) -> Result<Window, winit::error::OsError> {
    let attributes = Window::default_attributes()
        .with_title("超能 NONO")
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(true)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(LogicalSize::new(
            rect.w.max(1.0) as f64,
            rect.h.max(1.0) as f64,
        ))
        .with_position(LogicalPosition::new(rect.x as f64, rect.y as f64));
    event_loop.create_window(attributes)
}

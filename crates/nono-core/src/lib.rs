//! Core domain logic for the NONO desktop pet.
//!
//! This crate deliberately has no windowing, rendering or audio code.  It owns
//! the parts that should be unit-testable and reusable by extensions:
//!
//! * [`event`] — the JSON event bus every subsystem talks through.
//! * [`catalog`] — the runtime view of `assets/catalog.json`.
//! * [`state`] — the character state machine and playback clock.
//! * [`behavior`] — idle/wander scheduling ("personality").
//! * [`config`] — persisted user settings.

pub mod behavior;
pub mod catalog;
pub mod config;
pub mod event;
pub mod geometry;
pub mod state;

pub use behavior::Behavior;
pub use catalog::{AnimationKind, AnimationSpec, AudioSpec, Catalog};
pub use config::Config;
pub use event::{Bus, Event};
pub use geometry::{pet_window_rect, Direction, Point, Rect};
pub use state::{Mode, PetState, Tick};

/// Milliseconds since the Unix epoch, used on the wire.
pub fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

//! Movement-speed settings.
//!
//! Speeds are logical points per second; the renderer/window layer multiplies
//! them by the display DPI.  A global speed is always present; follow and idle
//! (free-roam) speeds can override it, or stay `None` to follow the global
//! value.

/// Default global speed: the original wander speed (60 pt/s).
pub const DEFAULT_GLOBAL: f32 = 60.0;
/// Follow mode keeps its original faster default as an explicit override.
pub const DEFAULT_FOLLOW: Option<f32> = Some(140.0);
/// Free-roaming uses the global speed unless the user overrides it.
pub const DEFAULT_IDLE: Option<f32> = None;

pub const MIN_SPEED: f32 = 10.0;
pub const MAX_SPEED: f32 = 400.0;
/// Tray nudge step, in points per second.
pub const STEP: f32 = 10.0;

pub fn sanitize(speed: f32) -> f32 {
    speed.clamp(MIN_SPEED, MAX_SPEED)
}

pub fn sanitize_override(speed: Option<f32>) -> Option<f32> {
    speed.map(sanitize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_original_hand_tuned_speeds() {
        assert_eq!(DEFAULT_GLOBAL, 60.0);
        assert_eq!(DEFAULT_FOLLOW, Some(140.0));
        assert_eq!(DEFAULT_IDLE, None);
    }

    #[test]
    fn sanitize_clamps_both_global_and_overrides() {
        assert_eq!(sanitize(0.0), MIN_SPEED);
        assert_eq!(sanitize(9999.0), MAX_SPEED);
        assert_eq!(sanitize_override(Some(5.0)), Some(MIN_SPEED));
        assert_eq!(sanitize_override(None), None);
    }
}

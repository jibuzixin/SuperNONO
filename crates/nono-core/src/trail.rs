//! Walking motion trail ("afterimage") timing.
//!
//! The trail is time-based: a snapshot is spawned every *interval*, and each
//! snapshot fades in/out over its *lifetime*.  The interval controls how dense
//! the afterimages are; the lifetime controls how long the whole trail is;
//! *opacity* scales how strong each snapshot is.  All three values are
//! customisable (tray nudge buttons + `config.toml`).

pub const DEFAULT_ENABLED: bool = true;
pub const DEFAULT_INTERVAL_MS: u32 = 120;
pub const DEFAULT_LIFETIME_MS: u32 = 640;
/// Opacity of a freshly spawned afterimage (before the fade curve).
pub const DEFAULT_OPACITY: f32 = 0.22;

pub const MIN_INTERVAL_MS: u32 = 20;
pub const MAX_INTERVAL_MS: u32 = 400;
pub const MIN_LIFETIME_MS: u32 = 100;
pub const MAX_LIFETIME_MS: u32 = 1600;
pub const MIN_OPACITY: f32 = 0.02;
pub const MAX_OPACITY: f32 = 0.60;

/// Tray nudge steps.
pub const INTERVAL_STEP_MS: u32 = 10;
pub const LIFETIME_STEP_MS: u32 = 50;
pub const OPACITY_STEP: f32 = 0.02;

/// A lifetime shorter than this many intervals would leave visual gaps.
pub const MIN_LIFETIME_INTERVALS: f32 = 2.0;
/// Upper bound on retained snapshots, so a small interval with a huge lifetime
/// cannot balloon memory or per-frame compositing cost.
pub const MAX_LIFETIME_INTERVALS: f32 = 48.0;
/// Largest transparent margin per side.  Extreme speed × lifetime combinations
/// clip the oldest afterimages instead of allocating a giant window.
pub const MAX_PAD_PX: f32 = 700.0;

/// Clamps persisted timing values into a sane range and keeps the lifetime
/// comfortably longer than the interval.
pub fn sanitize(interval_ms: u32, lifetime_ms: u32) -> (u32, u32) {
    let interval = interval_ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS);
    let min_lifetime = ((interval as f32 * MIN_LIFETIME_INTERVALS).ceil() as u32)
        .max(MIN_LIFETIME_MS);
    let max_lifetime = ((interval as f32 * MAX_LIFETIME_INTERVALS) as u32)
        .min(MAX_LIFETIME_MS)
        .max(min_lifetime);
    let lifetime = lifetime_ms.clamp(min_lifetime, max_lifetime);
    (interval, lifetime)
}

pub fn sanitize_opacity(opacity: f32) -> f32 {
    opacity.clamp(MIN_OPACITY, MAX_OPACITY)
}

/// Opacity of an afterimage that is `age` seconds into a `lifetime`-second life.
///
/// It fades in over the first 10% of its life, then fades out with a slight
/// ease; at the end of its lifetime it is fully transparent.  Every snapshot
/// gets its own age-based opacity, which is what makes the trail read as a
/// gradient instead of a stack of identical copies.
pub fn alpha_at(age_secs: f32, lifetime_secs: f32, peak: f32) -> f32 {
    if lifetime_secs <= 0.0 || age_secs < 0.0 || age_secs > lifetime_secs {
        return 0.0;
    }
    let t = (age_secs / lifetime_secs).clamp(0.0, 1.0);
    let fade_in = (t / 0.10).min(1.0).sqrt();
    let fade_out = (1.0 - t).powf(1.6);
    (peak * fade_in * fade_out).clamp(0.0, 1.0)
}

/// Transparent margin (physical px) a walk window needs for the afterimages.
///
/// The oldest snapshot sits `speed * lifetime` behind the pet, so that is how
/// far the stage must reach; `margin` covers antialiased edges and rounding.
pub fn pad_px(speed_px_per_sec: f32, lifetime_ms: u32, margin: f32) -> u32 {
    let distance = speed_px_per_sec.max(0.0) * lifetime_ms as f32 / 1000.0;
    (distance + margin).min(MAX_PAD_PX).ceil().max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_inside_the_custom_ranges() {
        assert_eq!(
            sanitize(DEFAULT_INTERVAL_MS, DEFAULT_LIFETIME_MS),
            (DEFAULT_INTERVAL_MS, DEFAULT_LIFETIME_MS)
        );
        assert_eq!(sanitize_opacity(DEFAULT_OPACITY), DEFAULT_OPACITY);
        assert!(sanitize_opacity(0.0) >= MIN_OPACITY);
        assert!(sanitize_opacity(9.0) <= MAX_OPACITY);
    }

    #[test]
    fn sanitise_clamps_and_keeps_a_long_enough_lifetime() {
        assert_eq!(sanitize(5, 5), (MIN_INTERVAL_MS, MIN_LIFETIME_MS));
        assert_eq!(sanitize(9999, 9999), (MAX_INTERVAL_MS, MAX_LIFETIME_MS));
        let (interval, lifetime) = sanitize(160, 200);
        assert_eq!(interval, 160);
        assert_eq!(lifetime, 320);
        // Dense trails are capped in snapshot count so memory stays bounded.
        assert_eq!(sanitize(MIN_INTERVAL_MS, 9999), (MIN_INTERVAL_MS, 960));
    }

    #[test]
    fn opacity_fades_in_then_out_and_is_zero_at_both_ends() {
        let life = 0.48;
        assert_eq!(alpha_at(0.0, life, DEFAULT_OPACITY), 0.0);
        assert_eq!(alpha_at(life, life, DEFAULT_OPACITY), 0.0);
        assert_eq!(alpha_at(life + 0.1, life, DEFAULT_OPACITY), 0.0);

        // Fade-in finishes at 10% of the lifetime.
        assert!(
            (alpha_at(0.048, life, DEFAULT_OPACITY)
                - DEFAULT_OPACITY * 0.9f32.powf(1.6))
            .abs()
                < 1e-4
        );
        // Strictly darker as the snapshot ages once past the fade-in.
        let samples = [0.10, 0.25, 0.40, 0.55, 0.70, 0.85, 1.0]
            .map(|t| alpha_at(t * life, life, DEFAULT_OPACITY));
        for pair in samples.windows(2) {
            assert!(pair[0] > pair[1], "{pair:?} should be decreasing");
        }
    }

    #[test]
    fn opacity_peaks_near_the_start_of_life() {
        let life = 0.48;
        let mut best: f32 = 0.0;
        for i in 0..=100 {
            best = best.max(alpha_at(life * i as f32 / 100.0, life, DEFAULT_OPACITY));
        }
        assert!(best > DEFAULT_OPACITY * 0.8);
        assert!(best <= DEFAULT_OPACITY);
    }

    #[test]
    fn padding_grows_with_speed_and_lifetime() {
        assert_eq!(pad_px(0.0, 480, 2.0), 2);
        let slow = pad_px(120.0, 480, 2.0);
        let fast = pad_px(280.0, 480, 2.0);
        let long = pad_px(280.0, 800, 2.0);
        assert!(slow < fast && fast < long);
        // 280 px/s * 0.48 s = 134.4 px + 2 px margin.
        assert_eq!(fast, 137);
        // Extreme settings are clipped to keep the stage bounded.
        assert_eq!(pad_px(4000.0, 1600, 2.0), MAX_PAD_PX as u32);
    }
}

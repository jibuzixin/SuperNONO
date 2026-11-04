//! Idle behaviour: what NONO does when nobody is interacting with it.
//!
//! The scheduler is deliberately small: every few seconds it picks one of
//! "wander a bit", "make an expression" or "look around", then goes back to
//! waiting.  All animation work still goes through [`PetState`].

use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::catalog::Catalog;
use crate::config::Config;
use crate::geometry::{Direction, Point, Rect};
use crate::state::{Mode, PetState};

#[derive(Debug)]
pub struct Behavior {
    pub enabled: bool,
    min_idle: f32,
    max_idle: f32,
    /// Idle/free-roam speed in logical pt/s.
    wander_speed: f32,
    next_decision: Instant,
    rng: StdRng,
}

impl Behavior {
    pub fn new(now: Instant, config: &Config) -> Self {
        Self::with_seed(now, config, StdRng::from_entropy().gen::<u64>())
    }

    pub fn with_seed(now: Instant, config: &Config, seed: u64) -> Self {
        let mut behavior = Self {
            enabled: config.wander,
            min_idle: config.roam_min_secs,
            max_idle: config.roam_max_secs,
            wander_speed: config.effective_idle_speed(),
            next_decision: now,
            rng: StdRng::seed_from_u64(seed),
        };
        behavior.reschedule(now);
        behavior
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Updates the speed used by "wander a bit" walks (logical pt/s).
    pub fn set_wander_speed(&mut self, speed: f32) {
        if speed > 0.0 {
            self.wander_speed = speed;
        }
    }

    pub fn wander_speed(&self) -> f32 {
        self.wander_speed
    }

    /// Applied when the user changes the roaming interval in the menu.
    pub fn set_interval(&mut self, min_secs: f32, max_secs: f32, now: Instant) {
        self.min_idle = min_secs.max(1.0);
        self.max_idle = max_secs.max(self.min_idle);
        self.reschedule(now);
    }

    /// Pushes the next idle decision out (used after a user-driven mode such as
    /// mouse following so the pet does not immediately wander off).
    pub fn delay(&mut self, now: Instant) {
        self.reschedule(now);
    }

    /// When the application should next wake up to make an idle decision.
    pub fn next_wake(&self) -> Instant {
        if self.enabled {
            self.next_decision
        } else {
            Instant::now() + Duration::from_secs(3600)
        }
    }

    fn reschedule(&mut self, now: Instant) {
        let secs = self.rng.gen_range(self.min_idle..=self.max_idle);
        self.next_decision = now + Duration::from_secs_f32(secs);
    }

    /// Advances the scheduler.  Returns `true` when the pet was given a new
    /// animation (the caller should re-render).
    pub fn update(
        &mut self,
        state: &mut PetState,
        catalog: &Catalog,
        bounds: Rect,
        now: Instant,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        if state.mode != Mode::Idle {
            // The next decision should happen shortly after the current action
            // or walk ends, not immediately.
            self.next_decision = now + Duration::from_secs_f32(self.rng.gen_range(1.5..3.5));
            return false;
        }
        if now < self.next_decision {
            return false;
        }

        self.reschedule(now);
        let roll: f32 = self.rng.gen_range(0.0..1.0);

        if roll < 0.55 {
            if let Some(target) = self.pick_walk_target(state, bounds) {
                state.start_walk(catalog, target, self.wander_speed * state.dpi, now);
                return true;
            }
        }

        if roll < 0.75 {
            let expressions = catalog.expression_infos(state.form);
            if !expressions.is_empty() {
                let pick = &expressions[self.rng.gen_range(0..expressions.len())];
                if state.play_expression(catalog, &pick.slug, now) {
                    return true;
                }
            }
        }

        // "Look around": change facing and hold the idle pose.
        let dir = Direction::ALL[self.rng.gen_range(0..Direction::ALL.len())];
        if dir != state.facing {
            state.facing = dir;
            state.idle(catalog, now);
            return true;
        }
        false
    }

    fn pick_walk_target(&mut self, state: &PetState, bounds: Rect) -> Option<Point> {
        let max_radius = 180.0 * state.dpi;
        let min_radius = 50.0 * state.dpi;
        let angle: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
        let distance = self.rng.gen_range(min_radius..=max_radius);
        let target = Point::new(
            state.anchor.x + angle.cos() * distance,
            state.anchor.y + angle.sin() * distance,
        )
        .clamp(bounds.inset(24.0));
        if target.distance(state.anchor) < 24.0 {
            return None;
        }
        Some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn wander_speed_uses_the_idle_override_or_global() {
        let start = Instant::now();
        let global = Config::default();
        let behavior = Behavior::with_seed(start, &global, 7);
        assert!((behavior.wander_speed() - 60.0).abs() < 1e-6);

        let mut config = Config::default();
        config.move_speed = 90.0;
        config.idle_speed = None;
        assert!((Behavior::with_seed(start, &config, 7).wander_speed() - 90.0).abs() < 1e-6);

        config.idle_speed = Some(120.0);
        let mut behavior = Behavior::with_seed(start, &config, 7);
        assert!((behavior.wander_speed() - 120.0).abs() < 1e-6);
        behavior.set_wander_speed(150.0);
        assert!((behavior.wander_speed() - 150.0).abs() < 1e-6);
    }

    #[test]
    fn disabled_behavior_never_interrupts_idle() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(root).unwrap();
        let mut config = Config::default();
        config.wander = false;
        let start = Instant::now();
        let mut state = PetState::new(&catalog, 1, Point::new(800.0, 600.0), 1.5, 2.0, start);
        let mut behavior = Behavior::with_seed(start, &config, 7);
        let bounds = Rect::new(0.0, 0.0, 1600.0, 1200.0);
        for i in 0..10 {
            let now = start + Duration::from_secs(60 * i);
            assert!(!behavior.update(&mut state, &catalog, bounds, now));
        }
        assert_eq!(state.mode, Mode::Idle);
    }

    #[test]
    fn enabled_behavior_does_something_after_waiting() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(root).unwrap();
        let config = Config::default();
        let start = Instant::now();
        let mut state = PetState::new(&catalog, 1, Point::new(800.0, 600.0), 1.5, 2.0, start);
        let mut behavior = Behavior::with_seed(start, &config, 42);
        let bounds = Rect::new(0.0, 0.0, 1600.0, 1200.0);
        let woke = behavior.update(&mut state, &catalog, bounds, start + Duration::from_secs(30));
        assert!(woke);
        assert_ne!(state.mode, Mode::Idle);
    }
}

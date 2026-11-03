//! Character state machine and playback clock.
//!
//! The state machine is driven by a `now: Instant` supplied by the caller, which
//! keeps it deterministic and unit-testable.  Rendering, window placement and
//! audio are side effects the application layer performs in response.

use std::time::{Duration, Instant};

use crate::catalog::{AnimationSpec, Catalog};
use crate::geometry::{pet_window_rect, Direction, Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Walk,
    Action,
    Expression,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Idle => "Idle",
            Mode::Walk => "Walk",
            Mode::Action => "Action",
            Mode::Expression => "Expression",
        }
    }

    pub fn is_busy(self) -> bool {
        !matches!(self, Mode::Idle)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Tick {
    /// The anchor moved this tick.
    pub moved: bool,
    /// The current one-shot animation reached its last frame this tick.
    pub finished: bool,
    /// The active animation asset changed this tick.
    pub anim_changed: bool,
}

#[derive(Debug)]
pub struct PetState {
    pub form: u8,
    pub mode: Mode,
    pub anim: AnimationSpec,
    pub anim_started: Instant,
    /// Screen-space position of the pet's feet (physical px).
    pub anchor: Point,
    pub facing: Direction,
    pub scale: f32,
    pub dpi: f32,
    /// A one-shot animation has reached its last frame.
    pub finished: bool,
    walk_target: Option<Point>,
    walk_speed: f32,
    last_tick: Instant,
}

impl PetState {
    pub fn new(
        catalog: &Catalog,
        form: u8,
        anchor: Point,
        scale: f32,
        dpi: f32,
        now: Instant,
    ) -> Self {
        let facing = Direction::Down;
        let anim = catalog
            .idle_spec(form, facing)
            .expect("catalog validated at load time");
        Self {
            form,
            mode: Mode::Idle,
            anim,
            anim_started: now,
            anchor,
            facing,
            scale,
            dpi,
            finished: false,
            walk_target: None,
            walk_speed: 0.0,
            last_tick: now,
        }
    }

    pub fn window_rect(&self) -> Rect {
        pet_window_rect(
            self.anchor,
            self.anim.canvas,
            self.anim.pet_anchor,
            self.scale,
            self.dpi,
        )
    }

    /// Whether the pet should be rendered continuously right now.
    pub fn is_animating(&self) -> bool {
        self.mode.is_busy() && !self.finished
    }

    /// Current frame of the active animation.  Idle always holds frame 0.
    pub fn frame(&self, now: Instant) -> f32 {
        if self.mode == Mode::Idle {
            return self.anim.start_frame as f32;
        }
        let elapsed = now.saturating_duration_since(self.anim_started).as_secs_f32();
        let raw = elapsed * self.anim.fps;
        if self.anim.loop_anim {
            let frames = self.anim.frames.max(1) as f32;
            raw % frames
        } else {
            raw.min((self.anim.frames.max(1) - 1) as f32)
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(0.5, 3.0);
    }

    pub fn set_dpi(&mut self, dpi: f32) {
        if dpi > 0.0 {
            self.dpi = dpi;
        }
    }

    pub fn idle(&mut self, catalog: &Catalog, now: Instant) {
        if let Ok(anim) = catalog.idle_spec(self.form, self.facing) {
            self.anim = anim;
        }
        self.mode = Mode::Idle;
        self.anim_started = now;
        self.finished = false;
        self.walk_target = None;
        self.walk_speed = 0.0;
        self.last_tick = now;
    }

    /// Loads a different form.  Returns `true` if it actually changed.
    pub fn set_form(&mut self, catalog: &Catalog, form: u8, now: Instant) -> bool {
        if form == self.form || catalog.form(form).is_err() {
            return false;
        }
        self.form = form;
        self.facing = Direction::Down;
        self.idle(catalog, now);
        true
    }

    pub fn play_expression(&mut self, catalog: &Catalog, slug: &str, now: Instant) -> bool {
        match catalog.expression_spec(self.form, slug) {
            Ok(anim) => {
                self.anim = anim;
                self.mode = Mode::Expression;
                self.anim_started = now;
                self.finished = false;
                self.walk_target = None;
                self.last_tick = now;
                true
            }
            Err(err) => {
                tracing::warn!(%err, slug, "expression unavailable");
                false
            }
        }
    }

    pub fn play_action(&mut self, catalog: &Catalog, slug: &str, now: Instant) -> bool {
        match catalog.action_spec(self.form, slug) {
            Ok(anim) => {
                // A form change cannot swallow an action because the catalog is
                // checked per form.
                self.anim = anim;
                self.mode = Mode::Action;
                self.anim_started = now;
                self.finished = false;
                self.walk_target = None;
                self.last_tick = now;
                true
            }
            Err(err) => {
                tracing::warn!(%err, slug, "action unavailable");
                false
            }
        }
    }

    /// Walks in a straight line towards `target` (physical screen px).
    ///
    /// The direction is latched at the start so the eight-way walk cycle can
    /// play without switching assets mid-step.
    pub fn start_walk(&mut self, catalog: &Catalog, target: Point, speed: f32, now: Instant) {
        let dir = Direction::from_delta(target.x - self.anchor.x, target.y - self.anchor.y);
        match catalog.walk_spec(self.form, dir) {
            Ok(anim) => {
                self.facing = dir;
                self.anim = anim;
                self.mode = Mode::Walk;
                self.anim_started = now;
                self.finished = false;
                self.walk_target = Some(target);
                self.walk_speed = speed.max(1.0);
                self.last_tick = now;
            }
            Err(err) => tracing::warn!(%err, "walk animation unavailable"),
        }
    }

    pub fn stop_walk(&mut self, catalog: &Catalog, now: Instant) {
        if self.mode == Mode::Walk {
            self.idle(catalog, now);
        }
    }

    /// Advances movement and detects the end of a one-shot animation.
    pub fn tick(&mut self, catalog: &Catalog, now: Instant) -> Tick {
        let before = self.anim.id.clone();
        let mut tick = Tick::default();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;

        if self.mode == Mode::Walk {
            if let Some(target) = self.walk_target {
                let remaining = target.distance(self.anchor);
                let step = self.walk_speed * dt;
                if remaining <= step || remaining < 0.5 {
                    self.anchor = target;
                    tick.moved = true;
                    self.idle(catalog, now);
                } else {
                    let t = step / remaining;
                    self.anchor.x += (target.x - self.anchor.x) * t;
                    self.anchor.y += (target.y - self.anchor.y) * t;
                    tick.moved = true;
                }
            }
        } else if self.mode.is_busy() && !self.finished {
            let elapsed = now.saturating_duration_since(self.anim_started);
            let duration = Duration::from_secs_f32(self.anim.duration_secs());
            if elapsed >= duration {
                self.finished = true;
                tick.finished = true;
            }
        }

        tick.anim_changed = before != self.anim.id;
        tick
    }

    /// Applies the auto-end rules of the asset pack after [`Tick::finished`].
    ///
    /// Expressions always return to idle.  Actions return to idle only when the
    /// manifest says `auto_end` (charge and shutdown hold their final frame and
    /// keep the pet occupied until the user interacts again).
    pub fn after_finish(&mut self, catalog: &Catalog, now: Instant) -> bool {
        match self.mode {
            Mode::Expression => {
                self.idle(catalog, now);
                true
            }
            Mode::Action if self.anim.auto_end => {
                self.idle(catalog, now);
                true
            }
            Mode::Action => false,
            _ => false,
        }
    }

    /// Used by the drag handler: the pet follows the cursor.
    pub fn drag_by(&mut self, delta: Point) {
        self.anchor.x += delta.x;
        self.anchor.y += delta.y;
        self.last_tick = Instant::now();
    }

    /// Keeps the pet's feet inside `bounds` after a drag or a monitor change.
    pub fn clamp_to(&mut self, bounds: Rect) {
        let margin = 4.0;
        self.anchor.x = self.anchor.x.clamp(bounds.x + margin, bounds.right() - margin);
        self.anchor.y = self.anchor.y.clamp(bounds.y + margin, bounds.bottom() - margin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn catalog_and_state() -> Option<(Catalog, PetState)> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        if !root.join("catalog.json").is_file() {
            return None;
        }
        let catalog = Catalog::load(root).unwrap();
        let now = Instant::now();
        let state = PetState::new(&catalog, 1, Point::new(800.0, 600.0), 1.5, 2.0, now);
        Some((catalog, state))
    }

    #[test]
    fn idle_holds_frame_zero_and_is_not_animating() {
        let Some((_catalog, state)) = catalog_and_state() else { return };
        let now = Instant::now();
        assert_eq!(state.mode, Mode::Idle);
        assert_eq!(state.frame(now), 0.0);
        assert!(!state.is_animating());
        // 66x96 walk canvas at 1.5x on a 2x display.
        assert_eq!(state.window_rect().w, 198.0);
        assert_eq!(state.window_rect().h, 288.0);
    }

    #[test]
    fn walk_moves_then_returns_to_idle() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        let target = Point::new(state.anchor.x + 160.0, state.anchor.y);
        state.start_walk(&catalog, target, 400.0, start);
        assert_eq!(state.mode, Mode::Walk);
        assert_eq!(state.facing, Direction::Right);

        let mut now = start;
        for _ in 0..20 {
            now += Duration::from_millis(16);
            state.tick(&catalog, now);
        }
        assert!(state.anchor.x > 800.0);
        // 400 px/s over 320 ms -> arrives.
        for _ in 0..30 {
            now += Duration::from_millis(16);
            state.tick(&catalog, now);
        }
        assert_eq!(state.mode, Mode::Idle);
        assert!((state.anchor.x - target.x).abs() < 0.5);
    }

    #[test]
    fn expression_finishes_and_returns_to_idle() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.play_expression(&catalog, "happy", start));
        assert_eq!(state.mode, Mode::Expression);
        let duration = Duration::from_secs_f32(state.anim.duration_secs());
        let tick = state.tick(&catalog, start + duration + Duration::from_millis(10));
        assert!(tick.finished);
        assert!(state.after_finish(&catalog, start + duration));
        assert_eq!(state.mode, Mode::Idle);
    }

    #[test]
    fn shutdown_holds_last_frame() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.play_action(&catalog, "shutdown", start));
        let duration = Duration::from_secs_f32(state.anim.duration_secs());
        let now = start + duration + Duration::from_millis(10);
        state.tick(&catalog, now);
        assert!(!state.after_finish(&catalog, now));
        assert_eq!(state.mode, Mode::Action);
        assert_eq!(state.frame(now), (state.anim.frames - 1) as f32);
    }
}

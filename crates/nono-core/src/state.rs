//! Character state machine and playback clock.
//!
//! The state machine is driven by a `now: Instant` supplied by the caller, which
//! keeps it deterministic and unit-testable.  Rendering, window placement and
//! audio are side effects the application layer performs in response.

use std::time::{Duration, Instant};

use crate::catalog::{AnimationKind, AnimationSpec, Catalog};
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
    /// Transparent margin (canvas units) the renderer added so glow fits.
    glow_pad: f32,
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
            .walk_spec(form, facing)
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
            glow_pad: 0.0,
            finished: false,
            walk_target: None,
            walk_speed: 0.0,
            last_tick: now,
        }
    }

    pub fn window_rect(&self) -> Rect {
        // The tuning scale is a canvas zoom; the window must include the
        // enlarged canvas so scaled-up actions are not clipped.
        let k = self.anim.placement.scale.max(0.05);
        // Glow margin: the renderer draws the clip inside a larger transparent
        // canvas, so the window has to grow by the same amount (and the anchor
        // shifts with it) or the halo is cut at the edge.
        let pad = self.glow_pad.max(0.0) * k;
        pet_window_rect(
            self.anchor,
            (self.anim.canvas.0 * k + 2.0 * pad, self.anim.canvas.1 * k + 2.0 * pad),
            (self.anim.pet_anchor.0 * k + pad, self.anim.pet_anchor.1 * k + pad),
            self.scale,
            self.dpi,
        )
    }

    /// Sets the glow margin reported by the renderer (canvas units).
    pub fn set_glow_pad(&mut self, pad: f32) {
        self.glow_pad = pad.max(0.0);
    }

    /// The glow margin currently applied to the window (canvas units).
    pub fn glow_pad(&self) -> f32 {
        self.glow_pad
    }

    /// Current walking speed in physical pixels per second (0 when not walking).
    pub fn walk_speed(&self) -> f32 {
        if self.mode == Mode::Walk {
            self.walk_speed
        } else {
            0.0
        }
    }

    /// Applies a speed change to a walk that is already in progress.
    pub fn set_walk_speed(&mut self, speed: f32) {
        if self.mode == Mode::Walk {
            self.walk_speed = speed.max(1.0);
        }
    }

    /// Pauses the playback clock by `paused` (e.g. the time a Lottie file took
    /// to load).  Without this a slow load would advance the animation clock and
    /// the clip could finish before its first frame is ever shown.
    pub fn compensate_pause(&mut self, paused: Duration) {
        self.anim_started += paused;
    }

    /// Whether the current animation still produces new frames.
    ///
    /// Idle loops the walk cycle in place (the original client does the same),
    /// so it animates just like walking; only a held last frame (charge /
    /// shutdown) stops the clock.
    pub fn is_animating(&self) -> bool {
        !self.finished && self.anim.effective_frames() > 1
    }

    /// Current frame of the active animation.  Idle plays the walk cycle in
    /// place; one-shot animations clamp to their last frame.
    pub fn frame(&self, now: Instant) -> f32 {
        self.frame_at(now, 1.0)
    }

    /// Like [`frame`], but with a playback-speed multiplier (for the walk cycle).
    pub fn frame_at(&self, now: Instant, speed: f32) -> f32 {
        let elapsed = now.saturating_duration_since(self.anim_started).as_secs_f32();
        let raw = elapsed * self.anim.fps * speed.max(0.01);
        let frames = self.anim.effective_frames();
        if self.anim.loop_anim {
            raw % frames as f32
        } else {
            raw.min((frames - 1) as f32)
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
        match catalog.walk_spec(self.form, self.facing) {
            Ok(anim) => {
                // Do not restart the cycle when the pet was already walking in
                // this direction: stopping should look like a seamless in-place
                // continuation.
                let same_cycle = self.anim.kind == AnimationKind::Walk
                    && self.anim.file == anim.file
                    && self.anim.fps == anim.fps;
                self.anim = anim;
                if !same_cycle {
                    self.anim_started = now;
                }
            }
            Err(err) => tracing::warn!(%err, "idle animation unavailable"),
        }
        self.mode = Mode::Idle;
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
                let same_cycle = self.anim.kind == AnimationKind::Walk
                    && self.anim.file == anim.file
                    && self.anim.fps == anim.fps;
                self.facing = dir;
                self.anim = anim;
                self.mode = Mode::Walk;
                if !same_cycle {
                    self.anim_started = now;
                }
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
        } else if self.mode.is_busy() && !self.finished && !self.anim.loop_anim {
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
                // Some actions leave the pet facing a specific direction
                // (e.g. battery: left on form 1, right on form 2).
                if let Some(dir) = self.anim.return_facing {
                    self.facing = dir;
                }
                self.idle(catalog, now);
                true
            }
            Mode::Action => {
                // Charge/shutdown hold their last frame, except that a shutdown
                // on a form with a charge animation continues into a looping
                // recharge state.
                if let Some(next) = self.anim.after_action.clone() {
                    match catalog.action_spec(self.form, &next) {
                        Ok(mut spec) => {
                            spec.loop_anim = true;
                            spec.auto_end = false;
                            spec.return_facing = None;
                            self.anim = spec;
                            self.mode = Mode::Action;
                            self.anim_started = now;
                            self.finished = false;
                            self.walk_target = None;
                            self.walk_speed = 0.0;
                            self.last_tick = now;
                            return true;
                        }
                        Err(err) => tracing::warn!(%err, next, "after_action unavailable"),
                    }
                }
                false
            }
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
    fn frame_at_scales_the_walk_playback_speed() {
        let Some((_catalog, state)) = catalog_and_state() else { return };
        let later = state.anim_started + Duration::from_millis(200);
        // 25 fps × 0.2 s = frame 5; at 2× speed the cycle is at frame 10.
        assert!((state.frame(later) - 5.0).abs() < 0.01);
        assert!((state.frame_at(later, 2.0) - 10.0).abs() < 0.01);
        assert!((state.frame_at(later, 0.5) - 2.5).abs() < 0.01);
    }

    #[test]
    fn idle_loops_the_walk_cycle_in_place() {
        let Some((_catalog, state)) = catalog_and_state() else { return };
        let now = Instant::now();
        assert_eq!(state.mode, Mode::Idle);
        assert!(state.is_animating(), "idle keeps the walk cycle running");
        // 25 fps, 13 frames: 0.5 s in the cycle is frame 12.5.
        let later = now + Duration::from_millis(500);
        let expected = 12.5 % state.anim.frames as f32;
        assert!((state.frame(later) - expected).abs() < 0.01);
        // 66x96 walk canvas at 1.5x on a 2x display.
        assert_eq!(state.window_rect().w, 198.0);
        assert_eq!(state.window_rect().h, 288.0);
    }

    #[test]
    fn walking_and_stopping_keeps_the_cycle_phase() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        let target = Point::new(state.anchor.x + 40.0, state.anchor.y);
        state.start_walk(&catalog, target, 400.0, start);
        let mid = start + Duration::from_millis(200);
        let phase = state.frame(mid);
        state.idle(&catalog, mid);
        assert_eq!(state.mode, Mode::Idle);
        assert!((state.frame(mid) - phase).abs() < 0.01, "stopping should not restart the cycle");
    }

    #[test]
    fn actions_can_override_the_return_facing() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        // Form 1 battery -> face left afterwards.
        assert!(state.play_action(&catalog, "battery", start));
        let end = start + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, end);
        assert!(state.after_finish(&catalog, end));
        assert_eq!(state.mode, Mode::Idle);
        assert_eq!(state.facing, Direction::Left);

        // Summon always returns to the front view.
        state.facing = Direction::Right;
        assert!(state.play_action(&catalog, "summon", end));
        let end2 = end + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, end2);
        assert!(state.after_finish(&catalog, end2));
        assert_eq!(state.facing, Direction::Down);
    }

    #[test]
    fn form2_battery_faces_right() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.set_form(&catalog, 2, start));
        assert!(state.play_action(&catalog, "battery", start));
        let end = start + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, end);
        assert!(state.after_finish(&catalog, end));
        assert_eq!(state.facing, Direction::Right);
    }

    #[test]
    fn shutdown_keeps_the_facing_when_it_finishes() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        // Form 3 has no charge animation, so it holds the final frame.
        assert!(state.set_form(&catalog, 3, start));
        state.facing = Direction::Left;
        assert!(state.play_action(&catalog, "shutdown", start));
        let end = start + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, end);
        assert!(!state.after_finish(&catalog, end));
        assert_eq!(state.facing, Direction::Left);
    }

    #[test]
    fn load_pause_does_not_advance_the_animation_clock() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.play_action(&catalog, "cube", start));
        // Simulate a 2 s Lottie parse; the first visible frame must still be 0.
        state.compensate_pause(Duration::from_secs(2));
        assert_eq!(state.frame(start), 0.0);
        assert_eq!(state.frame(start + Duration::from_secs(1)), 0.0);
        let frame = state.frame(start + Duration::from_millis(2400));
        assert!((frame - 10.0).abs() < 0.01, "expected 0.4s * 25fps = 10, got {frame}");
    }

    #[test]
    fn walk_speed_can_be_adjusted_mid_walk() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        let target = Point::new(state.anchor.x + 600.0, state.anchor.y);
        state.start_walk(&catalog, target, 400.0, start);
        assert!((state.walk_speed() - 400.0).abs() < 0.01);
        state.set_walk_speed(800.0);
        assert!((state.walk_speed() - 800.0).abs() < 0.01);
        // Outside Walk the stored speed is reported as zero.
        state.stop_walk(&catalog, start);
        assert_eq!(state.walk_speed(), 0.0);
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
    fn shutdown_enters_a_looping_charge_when_available() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.play_action(&catalog, "shutdown", start));
        let end = start + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, end);
        assert!(state.after_finish(&catalog, end));
        assert_eq!(state.mode, Mode::Action);
        assert_eq!(state.anim.id, "form1/action/charge");
        assert!(state.anim.loop_anim);
        assert!(state.is_animating());
        // A looping charge never finishes on its own.
        let later = end + Duration::from_secs(10);
        let tick = state.tick(&catalog, later);
        assert!(!tick.finished);
        assert!(state.frame(later) < state.anim.effective_frames() as f32);
    }

    #[test]
    fn form3_shutdown_holds_the_last_frame() {
        let Some((catalog, mut state)) = catalog_and_state() else { return };
        let start = Instant::now();
        assert!(state.set_form(&catalog, 3, start));
        assert!(state.play_action(&catalog, "shutdown", start));
        let now = start + Duration::from_secs_f32(state.anim.duration_secs()) + Duration::from_millis(20);
        state.tick(&catalog, now);
        assert!(!state.after_finish(&catalog, now));
        assert_eq!(state.mode, Mode::Action);
        assert_eq!(state.frame(now), (state.anim.effective_frames() - 1) as f32);
        assert!(!state.is_animating());
    }
}

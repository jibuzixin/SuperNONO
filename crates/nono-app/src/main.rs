//! 超能 NONO desktop pet — application shell.
//!
//! Responsibilities of this crate are deliberately thin: parse the CLI, own the
//! winit event loop, and translate state-machine changes into window geometry,
//! rendering and audio.  All behaviour lives in `nono-core`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use nono_core::{
    config, Behavior, Bus, Catalog, Config, Direction, Event, Mode, PetState, Point, Rect,
};
use nono_platform::{build_tray, create_pet_window, create_presenter, AudioPlayer, Presenter, TrayMenu};
use nono_render::Renderer;
use rand::Rng;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

/// How often the event loop wakes while an animation is playing.  Rendering
/// itself is throttled to the animation's own frame rate.
const ANIMATION_POLL: Duration = Duration::from_millis(16);
/// Click vs drag threshold, in physical pixels.
const DRAG_THRESHOLD: f32 = 5.0;

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(String),
    Bus,
}

struct DragState {
    /// Cursor position (window-local) when the button went down.
    press_cursor: PhysicalPosition<f64>,
    /// Accumulated cursor travel, used to tell clicks from drags.
    distance: f32,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    assets_root: PathBuf,
    catalog: Catalog,
    config: Config,
    config_path: PathBuf,
    state: PetState,
    behavior: Behavior,
    renderer: Renderer,
    bus: Bus,
    bus_rx: tokio::sync::broadcast::Receiver<nono_core::event::Envelope>,
    audio: AudioPlayer,
    window: Option<Window>,
    presenter: Option<Box<dyn Presenter>>,
    tray: Option<TrayMenu>,
    drag: Option<DragState>,
    cursor: PhysicalPosition<f64>,
    /// Usable screen area (menu bar / Dock excluded), top-left physical px.
    screen_bounds: Rect,
    /// Kept on the same rect as `screen_bounds` for the behaviour scheduler.
    bounds: Rect,
    crop_buf: Vec<u8>,
    last_anim_id: String,
    last_frame_key: i64,
    dirty: bool,
    visible: bool,
    render_count: u64,
    render_time_sum_ms: f64,
    /// Debug/automation hook: an animation id to start right after setup.
    initial_play: Option<String>,
    /// Mouse-follow mode: when this expires, following stops.
    follow_until: Option<Instant>,
    follow_next: Instant,
    /// Delay before `initial_play` runs (debug/automation).
    play_delay: f32,
    scheduled_play: Option<(Instant, String)>,
}

impl App {
    fn new(
        assets_root: PathBuf,
        catalog: Catalog,
        config: Config,
        config_path: PathBuf,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let now = Instant::now();
        let bus = Bus::new();
        let bus_rx = bus.subscribe();
        let renderer = Renderer::new().context("initialising the ThorVG renderer")?;
        let state = PetState::new(&catalog, config.form, Point::new(200.0, 200.0), config.scale, 2.0, now);
        let behavior = Behavior::new(now, &config);

        let wake_proxy = proxy.clone();
        bus.set_waker(move || {
            let _ = wake_proxy.send_event(UserEvent::Bus);
        });

        Ok(Self {
            proxy,
            assets_root,
            catalog,
            config,
            config_path,
            state,
            behavior,
            renderer,
            bus,
            bus_rx,
            audio: AudioPlayer::new(),
            window: None,
            presenter: None,
            tray: None,
            drag: None,
            cursor: PhysicalPosition::new(0.0, 0.0),
            screen_bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
            bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
            crop_buf: Vec::new(),
            last_anim_id: String::new(),
            last_frame_key: i64::MIN,
            dirty: true,
            visible: true,
            initial_play: None,
            follow_until: None,
            follow_next: Instant::now(),
            play_delay: 0.0,
            scheduled_play: None,
            render_count: 0,
            render_time_sum_ms: 0.0,
        })
    }

    // ── setup ───────────────────────────────────────────────────────

    fn setup(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        let (monitor_bounds, dpi) = match &monitor {
            Some(monitor) => {
                let pos = monitor.position();
                let size = monitor.size();
                (
                    Rect::new(
                        pos.x as f32,
                        pos.y as f32,
                        size.width as f32,
                        size.height as f32,
                    ),
                    monitor.scale_factor(),
                )
            }
            None => (Rect::new(0.0, 0.0, 1440.0, 900.0), 2.0),
        };
        // macOS clamps oversized action windows to the usable screen area, which
        // would shift the pet.  Clip the window ourselves instead.
        let screen_bounds = nono_platform::screen_visible_bounds().unwrap_or(monitor_bounds);
        self.bounds = screen_bounds;
        self.screen_bounds = screen_bounds;
        let default_anchor = Point::new(
            screen_bounds.x + screen_bounds.w * 0.75,
            screen_bounds.y + screen_bounds.h * 0.82,
        );
        let anchor = self
            .config
            .anchor
            .unwrap_or(default_anchor)
            .clamp(screen_bounds);
        let now = Instant::now();
        self.state = PetState::new(&self.catalog, self.config.form, anchor, self.config.scale, dpi as f32, now);
        self.bounds = self.compute_roam_bounds();
        self.state.clamp_to(self.bounds);
        tracing::debug!(
            roam = ?(self.bounds.x, self.bounds.y, self.bounds.w, self.bounds.h),
            "pet roaming bounds"
        );

        #[cfg(target_os = "macos")]
        nono_platform::macos::set_accessory_activation_policy();

        tracing::debug!(
            monitor_pos = ?monitor.as_ref().map(|m| m.position()),
            monitor_size = ?monitor.as_ref().map(|m| m.size()),
            dpi,
            anchor = ?(anchor.x, anchor.y),
            "monitor geometry"
        );
        let rect = self.visible_rect();
        let window = create_pet_window(event_loop, rect).context("creating the pet window")?;
        #[cfg(target_os = "macos")]
        {
            nono_platform::macos::tune_window(&window);
            nono_platform::macos::log_geometry(&window);
        }

        // The monitor scale factor is authoritative, but the window may land on
        // a display with a different scale.
        let actual_dpi = window.scale_factor();
        if (actual_dpi - dpi).abs() > 0.01 {
            self.state.set_dpi(actual_dpi as f32);
            let rect = self.visible_rect();
            let _ = window.request_inner_size(PhysicalSize::new(rect.w as u32, rect.h as u32));
            window.set_outer_position(PhysicalPosition::new(rect.x as i32, rect.y as i32));
        }

        let presenter = create_presenter(&window).context("creating the transparent presenter")?;
        self.window = Some(window);
        self.presenter = Some(presenter);

        // Tray icon: generated PNG if present, otherwise a drawn fallback.
        let icon_path = self.assets_root.join("icon/nono-tray.png");
        let icon = nono_platform::icon::load_icon(&icon_path)
            .unwrap_or_else(|| nono_platform::icon::fallback_icon(32));
        let proxy = self.proxy.clone();
        match build_tray(&self.catalog, &self.config, icon, move |id| {
            let _ = proxy.send_event(UserEvent::Menu(id));
        }) {
            Ok(tray) => {
                tray.sync_checks(&self.config);
                self.tray = Some(tray);
            }
            Err(err) => tracing::warn!(%err, "tray icon unavailable"),
        }

        self.audio.set_enabled(self.config.sound);
        self.audio.set_volume(self.config.volume);
        self.renderer.set_tint(self.config.tint);

        self.sync_animation(now, true, "setup");
        self.render_now(now);
        self.bus.publish(Event::ChangeForm { form: self.state.form });

        if let Some(play) = self.initial_play.clone() {
            if self.play_delay > 0.0 {
                self.scheduled_play =
                    Some((now + Duration::from_secs_f32(self.play_delay), play));
            } else {
                self.play_scripted(&play, now);
            }
        }
        Ok(())
    }

    // ── animation / rendering ───────────────────────────────────────

    fn sync_animation(&mut self, now: Instant, force: bool, reason: &'static str) {
        let anim_id = self.state.anim.id.clone();
        if !force && anim_id == self.last_anim_id {
            return;
        }
        let rect = self.visible_rect();
        let dpi = self.state.dpi;
        let scale = self.state.scale * self.state.dpi;
        // Load BEFORE resizing the window: a cold Lottie parse on the first play
        // must not leave an empty, already-enlarged window on screen.
        let load_started = Instant::now();
        match self.renderer.load(&self.state.anim, scale) {
            Ok(()) => tracing::debug!(
                anim = %anim_id,
                load_ms = load_started.elapsed().as_secs_f64() * 1000.0,
                "animation loaded"
            ),
            Err(err) => {
                tracing::warn!(%err, anim = %anim_id, "rendering the animation failed");
                return;
            }
        }
        if let Some(window) = &self.window {
            let _ = window.request_inner_size(PhysicalSize::new(rect.w as u32, rect.h as u32));
            window.set_outer_position(PhysicalPosition::new(rect.x as i32, rect.y as i32));
            // During big actions the window covers a lot of desktop; let clicks
            // through.  Actions that hold their last frame stay interactive.
            let interactive = self.state.mode != Mode::Action || !self.state.anim.auto_end;
            if let Err(err) = window.set_cursor_hittest(interactive) {
                tracing::debug!(%err, "set_cursor_hittest failed");
            }
        }
        if let Some(presenter) = self.presenter.as_mut() {
            if let Err(err) = presenter.resize(rect.w as u32, rect.h as u32, dpi as f64) {
                tracing::warn!(%err, "presenter resize failed");
            }
        }

        self.audio.stop();
        if self.state.mode == Mode::Action {
            if let Some(audio) = self.state.anim.audio.as_ref() {
                self.audio.play(&audio.path, audio.loop_until_stop);
            }
        }

        self.last_anim_id = anim_id;
        self.last_frame_key = i64::MIN;
        self.dirty = true;
        self.bus.publish(Event::PlayAnimation {
            anim_id: self.last_anim_id.clone(),
            form: self.state.form,
            loop_anim: self.state.anim.loop_anim,
        });
        tracing::debug!(
            anim = %self.last_anim_id,
            mode = self.state.mode.as_str(),
            reason,
            force,
            size = ?(rect.w, rect.h),
            "animation switch"
        );
        let _ = now;
    }

    fn render_now(&mut self, now: Instant) {
        let started = Instant::now();
        let frame = self.state.frame(now);
        if let Err(err) = self.renderer.render_frame(frame) {
            tracing::warn!(%err, frame, "render failed");
            return;
        }
        let (full_w, full_h) = self.renderer.size();
        let desired = self.state.window_rect();
        let visible = desired.intersect(self.screen_bounds);
        let ox = ((visible.x - desired.x).round().max(0.0) as u32).min(full_w.saturating_sub(1));
        let oy = ((visible.y - desired.y).round().max(0.0) as u32).min(full_h.saturating_sub(1));
        let crop_w = (visible.w.round() as u32).min(full_w - ox).max(1);
        let crop_h = (visible.h.round() as u32).min(full_h - oy).max(1);
        let dpi = self.state.dpi as f64;
        {
            // Split borrows: the renderer is read, the crop buffer is written,
            // and only then is the presenter called.
            let Self { renderer, presenter, crop_buf, .. } = self;
            let Some(presenter) = presenter.as_mut() else { return };
            let full = renderer.rgba8();
            if ox == 0 && oy == 0 && crop_w == full_w && crop_h == full_h {
                if let Err(err) = presenter.present(full, full_w, full_h, dpi) {
                    tracing::warn!(%err, "present failed");
                }
            } else {
                Self::crop_into(full, full_w, ox, oy, crop_w, crop_h, crop_buf);
                if let Err(err) = presenter.present(crop_buf, crop_w, crop_h, dpi) {
                    tracing::warn!(%err, "present failed");
                }
            }
        }
        let width = crop_w;
        let height = crop_h;
        self.last_frame_key = frame_key(frame);
        self.dirty = false;
        self.render_count += 1;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.render_time_sum_ms += elapsed_ms;
        if self.render_count % 25 == 0 {
            tracing::debug!(
                frame,
                size = ?(width, height),
                avg_ms = self.render_time_sum_ms / 25.0,
                "render stats"
            );
            self.render_time_sum_ms = 0.0;
        }
    }

    fn position_window(&self) {
        let Some(window) = &self.window else { return };
        let rect = self.visible_rect();
        window.set_outer_position(PhysicalPosition::new(rect.x as i32, rect.y as i32));
    }

    /// The part of the animation window that fits on the usable screen.
    fn visible_rect(&self) -> Rect {
        self.state.window_rect().intersect(self.screen_bounds)
    }

    /// Largest extent of the pet's *silhouette* around its anchor, over all
    /// eight walk directions (left, top, right, bottom, in canvas units).
    fn pet_margins(&self) -> (f32, f32, f32, f32) {
        let (mut left, mut top, mut right, mut bottom) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for dir in Direction::ALL {
            let Ok(spec) = self.catalog.walk_spec(self.state.form, dir) else {
                continue;
            };
            let Some(bb) = spec.content_bbox else { continue };
            left = left.max(spec.pet_anchor.0 - bb.0);
            top = top.max(spec.pet_anchor.1 - bb.1);
            right = right.max(bb.2 - spec.pet_anchor.0);
            bottom = bottom.max(bb.3 - spec.pet_anchor.1);
        }
        (left, top, right, bottom)
    }

    /// Screen rect the anchor may move in: the pet silhouette never leaves the
    /// usable screen, even when dragged against an edge.
    fn compute_roam_bounds(&self) -> Rect {
        let (left, top, right, bottom) = self.pet_margins();
        let s = self.state.scale * self.state.dpi;
        let sb = self.screen_bounds;
        let x0 = sb.x + left * s;
        let y0 = sb.y + top * s;
        let x1 = sb.right() - right * s;
        let y1 = sb.bottom() - bottom * s;
        if x1 <= x0 || y1 <= y0 {
            sb
        } else {
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        }
    }

    /// Copies a sub-rectangle of an RGBA frame (used when the window had to be
    /// clipped at a screen edge).
    fn crop_into(src: &[u8], src_w: u32, x: u32, y: u32, w: u32, h: u32, out: &mut Vec<u8>) {
        out.clear();
        out.reserve((w as usize) * (h as usize) * 4);
        for row in y..y + h {
            let start = ((row as usize) * (src_w as usize) + (x as usize)) * 4;
            out.extend_from_slice(&src[start..start + (w as usize) * 4]);
        }
    }

    /// Reloads the renderer at the current scale while keeping the animation
    /// phase (used for real scale-factor changes and menu zoom changes).
    fn reload_preserving_frame(&mut self, now: Instant) {
        self.bounds = self.compute_roam_bounds();
        self.state.clamp_to(self.bounds);
        let rect = self.visible_rect();
        let dpi = self.state.dpi;
        if let Some(window) = &self.window {
            let _ = window.request_inner_size(PhysicalSize::new(rect.w as u32, rect.h as u32));
            window.set_outer_position(PhysicalPosition::new(rect.x as i32, rect.y as i32));
            let interactive = self.state.mode != Mode::Action || !self.state.anim.auto_end;
            if let Err(err) = window.set_cursor_hittest(interactive) {
                tracing::debug!(%err, "set_cursor_hittest failed");
            }
        }
        let scale = self.state.scale * self.state.dpi;
        if let Err(err) = self.renderer.load(&self.state.anim, scale) {
            tracing::warn!(%err, "reloading the animation failed");
        }
        if let Some(presenter) = self.presenter.as_mut() {
            if let Err(err) = presenter.resize(rect.w as u32, rect.h as u32, dpi as f64) {
                tracing::warn!(%err, "presenter resize failed");
            }
        }
        self.last_anim_id = self.state.anim.id.clone();
        self.last_frame_key = i64::MIN;
        self.dirty = true;
        self.render_now(now);
    }

    fn frame_due(&self, now: Instant) -> bool {
        let key = frame_key(self.state.frame(now));
        key != self.last_frame_key || self.dirty
    }

    // ── interactions ────────────────────────────────────────────────

    fn on_click(&mut self, now: Instant) {
        self.bus.publish(Event::UserClick { button: "left".into() });
        let expressions = self.catalog.expression_infos(self.state.form);
        if expressions.is_empty() {
            return;
        }
        // Prefer cheerful reactions, but keep some variety.
        let idx = {
            let mut rng = rand::thread_rng();
            if expressions.iter().any(|e| e.slug == "happy") && rng.gen_bool(0.6) {
                expressions.iter().position(|e| e.slug == "happy").unwrap()
            } else {
                rng.gen_range(0..expressions.len())
            }
        };
        let slug = expressions[idx].slug.clone();
        if self.state.play_expression(&self.catalog, &slug, now) {
            self.sync_animation(now, false, "state-change");
            self.render_now(now);
        }
    }

    fn on_secondary_click(&mut self, now: Instant) {
        self.bus.publish(Event::UserClick { button: "right".into() });
        let actions = self.catalog.action_infos(self.state.form);
        if actions.is_empty() {
            return;
        }
        let slug = {
            let mut rng = rand::thread_rng();
            actions[rng.gen_range(0..actions.len())].slug.clone()
        };
        if self.state.play_action(&self.catalog, &slug, now) {
            self.sync_animation(now, false, "state-change");
            self.render_now(now);
        }
    }

    /// Handles the `--play` automation hook (`action:1:cube`,
    /// `expression:1:happy`, `walk:1:down`, `form:2`).
    fn play_scripted(&mut self, spec: &str, now: Instant) {
        let parts = spec.split(':').collect::<Vec<_>>();
        match parts.as_slice() {
            ["follow"] => self.toggle_follow(now),
            ["form", form] => {
                if let Ok(form) = form.parse::<u8>() {
                    self.switch_form(form, now);
                }
            }
            ["action", form, slug] => {
                if let Ok(form) = form.parse::<u8>() {
                    self.menu_action(&format!("{form}:{slug}"), now);
                }
            }
            ["expression", form, slug] => {
                if let Ok(form) = form.parse::<u8>() {
                    self.menu_expression(&format!("{form}:{slug}"), now);
                }
            }
            ["walk", form, dir] => {
                if let (Ok(form), Some(dir)) = (form.parse::<u8>(), Direction::from_key(dir)) {
                    if self.state.set_form(&self.catalog, form, now) {
                        self.config.form = form;
                    }
                    self.state.facing = dir;
                    if let Ok(anim) = self.catalog.walk_spec(self.state.form, dir) {
                        self.state.anim = anim;
                        self.state.mode = Mode::Walk;
                        self.state.anim_started = now;
                        self.sync_animation(now, true, "play-scripted");
                    }
                }
            }
            _ => tracing::warn!(spec, "unrecognised --play spec"),
        }
    }

    /// Tray item: follow the mouse for 20 seconds (click again to stop).
    fn toggle_follow(&mut self, now: Instant) {
        if self.follow_until.is_some() {
            self.stop_follow(now);
            return;
        }
        if nono_platform::cursor_position().is_none() {
            tracing::warn!("cursor position unavailable, follow disabled");
            return;
        }
        tracing::debug!("follow started (20 s)");
        self.follow_until = Some(now + Duration::from_secs(20));
        self.follow_next = now;
        if let Some(tray) = &self.tray {
            tray.follow.set_checked(true);
        }
        // Leave any held action / charge loop and start following on foot.
        self.state.idle(&self.catalog, now);
        self.sync_animation(now, false, "follow-start");
        self.render_now(now);
        self.bus.publish(Event::ChangeState { state: "Follow".into() });
    }

    fn stop_follow(&mut self, now: Instant) {
        if self.follow_until.take().is_none() {
            return;
        }
        if let Some(tray) = &self.tray {
            tray.follow.set_checked(false);
        }
        tracing::debug!("follow stopped");
        self.state.idle(&self.catalog, now);
        self.behavior.delay(now);
        self.sync_animation(now, false, "follow-stop");
        self.render_now(now);
        self.bus.publish(Event::ChangeState { state: self.state.mode.as_str().into() });
    }

    /// One follow step: re-target the cursor every ~150 ms so the pet keeps
    /// walking towards it (and faces it when close enough).
    fn follow_step(&mut self, now: Instant) {
        let Some(cursor) = nono_platform::cursor_position() else {
            return;
        };
        let target = cursor.clamp(self.bounds);
        let distance = target.distance(self.state.anchor);
        let close_enough = 60.0 * self.state.dpi;
        if distance > close_enough {
            let speed = 140.0 * self.state.dpi;
            self.state.start_walk(&self.catalog, target, speed, now);
        } else {
            if self.state.mode == Mode::Walk {
                self.state.idle(&self.catalog, now);
            }
            let dir = Direction::from_delta(
                cursor.x - self.state.anchor.x,
                cursor.y - self.state.anchor.y,
            );
            if dir != self.state.facing {
                self.state.facing = dir;
                self.state.idle(&self.catalog, now);
            }
        }
        self.sync_animation(now, false, "follow");
        self.follow_next = now + Duration::from_millis(150);
    }

    /// Applies a palette choice (`default` or `r,g,b`) without touching assets.
    fn apply_tint(&mut self, spec: &str, now: Instant) {
        let tint = if spec == "default" {
            None
        } else {
            let parts = spec
                .split(',')
                .filter_map(|p| p.parse::<u8>().ok())
                .collect::<Vec<_>>();
            if parts.len() == 3 {
                Some([parts[0], parts[1], parts[2]])
            } else {
                tracing::warn!(spec, "invalid tint");
                return;
            }
        };
        self.config.tint = tint;
        self.renderer.set_tint(tint);
        // The tint is baked into the Lottie layers at load time, so reload the
        // current animation (keeping its playback position).
        self.reload_preserving_frame(now);
        self.save_config();
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
        tracing::debug!(?tint, "tint changed");
    }

    fn save_config(&mut self) {
        self.config.anchor = Some(self.state.anchor);
        if let Err(err) = self.config.save(&self.config_path) {
            tracing::warn!(%err, path = %self.config_path.display(), "saving config failed");
        }
    }

    fn on_menu(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        use nono_platform::tray::*;
        let now = Instant::now();
        match id {
            MENU_QUIT => {
                self.save_config();
                event_loop.exit();
            }
            MENU_HIDE => {
                self.visible = !self.visible;
                if let Some(window) = &self.window {
                    window.set_visible(self.visible);
                }
                if let Some(tray) = &self.tray {
                    tray.hide.set_text(if self.visible { "隐藏 NONO" } else { "显示 NONO" });
                }
            }
            MENU_SOUND => {
                self.config.sound = !self.config.sound;
                self.audio.set_enabled(self.config.sound);
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                self.save_config();
            }
            MENU_WANDER => {
                self.config.wander = !self.config.wander;
                self.behavior.set_enabled(self.config.wander);
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                self.save_config();
            }
            MENU_FOLLOW => self.toggle_follow(now),
            MENU_IDLE_ANIMATION => {
                self.config.idle_animation = !self.config.idle_animation;
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                // Redraw once so turning it off freezes on a current pose.
                self.dirty = true;
                self.render_now(Instant::now());
                self.save_config();
            }
            _ => {
                if let Some(form) = id.strip_prefix(MENU_FORM_PREFIX) {
                    if let Ok(form) = form.parse::<u8>() {
                        self.switch_form(form, now);
                    }
                } else if let Some(scale) = id.strip_prefix(MENU_SCALE_PREFIX) {
                    if let Ok(scale) = scale.parse::<f32>() {
                        self.set_scale(scale, now);
                    }
                } else if let Some(rest) = id.strip_prefix(MENU_ACTION_PREFIX) {
                    self.menu_action(rest, now);
                } else if let Some(rest) = id.strip_prefix(MENU_EXPRESSION_PREFIX) {
                    self.menu_expression(rest, now);
                } else if let Some(rest) = id.strip_prefix(MENU_TINT_PREFIX) {
                    self.apply_tint(rest, now);
                }
            }
        }
    }

    fn switch_form(&mut self, form: u8, now: Instant) {
        if self.follow_until.is_some() {
            self.stop_follow(now);
        }
        if !self.state.set_form(&self.catalog, form, now) {
            return;
        }
        self.config.form = form;
        self.bounds = self.compute_roam_bounds();
        self.state.clamp_to(self.bounds);
        self.sync_animation(now, false, "state-change");
        self.render_now(now);
        self.save_config();
        self.bus.publish(Event::ChangeForm { form });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
    }

    fn set_scale(&mut self, scale: f32, now: Instant) {
        self.config.scale = scale.clamp(0.5, 3.0);
        self.state.set_scale(self.config.scale);
        self.reload_preserving_frame(now);
        self.save_config();
        self.bus.publish(Event::SetScale { scale: self.config.scale });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
    }

    fn menu_action(&mut self, rest: &str, now: Instant) {
        if self.follow_until.is_some() {
            self.stop_follow(now);
        }
        let mut parts = rest.splitn(2, ':');
        let form = parts.next().and_then(|f| f.parse::<u8>().ok());
        let slug = parts.next().unwrap_or_default().to_string();
        if slug.is_empty() {
            return;
        }
        if let Some(form) = form {
            if form != self.state.form {
                self.switch_form(form, now);
            }
        }
        // A form may not have the requested action (form 3 has no charge).
        if !self.catalog.has_action(self.state.form, &slug) {
            tracing::info!(slug, "action not present in this form");
            return;
        }
        if self.state.play_action(&self.catalog, &slug, now) {
            self.sync_animation(now, false, "state-change");
            self.render_now(now);
        }
    }

    fn menu_expression(&mut self, rest: &str, now: Instant) {
        if self.follow_until.is_some() {
            self.stop_follow(now);
        }
        let mut parts = rest.splitn(2, ':');
        let form = parts.next().and_then(|f| f.parse::<u8>().ok());
        let slug = parts.next().unwrap_or_default().to_string();
        if slug.is_empty() {
            return;
        }
        if let Some(form) = form {
            if form != self.state.form {
                self.switch_form(form, now);
            }
        }
        if self.state.play_expression(&self.catalog, &slug, now) {
            self.sync_animation(now, false, "state-change");
            self.render_now(now);
        }
    }

    // ── scheduling ──────────────────────────────────────────────────

    fn schedule(&self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let idle = self.state.mode == Mode::Idle;
        let animating = self.state.is_animating() && (!idle || self.config.idle_animation);
        let next = if self.follow_until.is_some() {
            now + Duration::from_millis(50)
        } else if animating {
            now + ANIMATION_POLL
        } else if self.state.mode == Mode::Action && !self.state.anim.auto_end {
            // Holding the last frame of charge/shutdown: sleep until the user
            // interacts (menu, click, drag) — all of which wake the loop.
            now + Duration::from_secs(3600)
        } else {
            self.behavior.next_wake()
        };
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }
}

fn frame_key(frame: f32) -> i64 {
    // Whole animation frames: at 25 fps this caps renders at 25/s even though the
    // event loop wakes at ~60 Hz for smooth window movement.
    frame.floor() as i64
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(err) = self.setup(event_loop) {
            tracing::error!("{err:#}");
            event_loop.exit();
            return;
        }
        self.schedule(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_config();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(presenter) = self.presenter.as_mut() {
                    let dpi = self.state.dpi as f64;
                    if let Err(err) = presenter.resize(size.width, size.height, dpi) {
                        tracing::debug!(%err, "resize failed");
                    }
                }
                self.dirty = true;
                self.render_now(Instant::now());
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // winit also emits this when a window is resized on the same
                // display (observed with the big action windows); reloading in
                // that case restarts the animation, so only react to real changes.
                if (self.state.dpi as f64 - scale_factor).abs() > 0.001 {
                    tracing::debug!(scale_factor, "scale factor changed");
                    self.state.set_dpi(scale_factor as f32);
                    self.reload_preserving_frame(Instant::now());
                } else {
                    tracing::debug!(scale_factor, "spurious scale factor event ignored");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let previous = self.cursor;
                self.cursor = position;
                if let Some(drag) = &mut self.drag {
                    let dx = (position.x - drag.press_cursor.x) as f32;
                    let dy = (position.y - drag.press_cursor.y) as f32;
                    drag.distance += (dx * dx + dy * dy).sqrt();
                    if let Some(window) = &self.window {
                        if let Ok(current) = window.outer_position() {
                            let new_x = (current.x as f32 + dx).round();
                            let new_y = (current.y as f32 + dy).round();
                            window.set_outer_position(PhysicalPosition::new(
                                new_x as i32,
                                new_y as i32,
                            ));
                            // Move the anchor by the delta actually applied; using
                            // the window origin directly would be wrong when the
                            // window is clipped at a screen edge.
                            self.state.anchor.x += new_x - current.x as f32;
                            self.state.anchor.y += new_y - current.y as f32;
                            // Hard stop at the usable screen: the silhouette must
                            // never be cut off, even while the mouse keeps going.
                            self.state.clamp_to(self.bounds);
                            let rect = self.visible_rect();
                            window.set_outer_position(PhysicalPosition::new(
                                rect.x as i32,
                                rect.y as i32,
                            ));
                        }
                    }
                } else {
                    let _ = previous;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    let now = Instant::now();
                    if self.follow_until.is_some() {
                        self.stop_follow(now);
                    }
                    self.drag = Some(DragState { press_cursor: self.cursor, distance: 0.0 });
                    if self.state.mode == Mode::Walk {
                        self.state.stop_walk(&self.catalog, now);
                        self.sync_animation(now, false, "state-change");
                        self.render_now(now);
                    }
                    self.bus.publish(Event::DragStarted);
                }
                (MouseButton::Left, ElementState::Released) => {
                    let drag = self.drag.take();
                    let now = Instant::now();
                    match drag {
                        Some(drag) if drag.distance < DRAG_THRESHOLD => self.on_click(now),
                        Some(_) => {
                            self.state.clamp_to(self.bounds);
                            self.position_window();
                            self.save_config();
                            self.bus.publish(Event::DragEnded);
                        }
                        None => {}
                    }
                    self.schedule(event_loop);
                }
                (MouseButton::Right, ElementState::Released) => self.on_secondary_click(Instant::now()),
                _ => {}
            },
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(id) => {
                self.on_menu(event_loop, &id);
                self.schedule(event_loop);
            }
            UserEvent::Bus => {
                while let Ok(envelope) = self.bus_rx.try_recv() {
                    tracing::trace!(event = ?envelope.event, "bus");
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() || !self.visible {
            return;
        }
        let now = Instant::now();

        if let Some((at, spec)) = self.scheduled_play.clone() {
            if now >= at {
                self.scheduled_play = None;
                self.play_scripted(&spec, now);
            }
        }

        let tick = self.state.tick(&self.catalog, now);
        if tick.finished {
            let anim_id = self.state.anim.id.clone();
            self.bus.publish(Event::AnimationFinished { anim_id });
            if self.state.after_finish(&self.catalog, now) {
                self.sync_animation(now, false, "state-change");
            }
        }
        if tick.moved {
            self.position_window();
        }

        let mut following = false;
        if let Some(until) = self.follow_until {
            if now >= until {
                self.stop_follow(now);
            } else {
                following = true;
                if now >= self.follow_next {
                    self.follow_step(now);
                }
            }
        }
        if !following
            && self.behavior.update(&mut self.state, &self.catalog, self.bounds, now)
        {
            self.sync_animation(now, false, "state-change");
        }
        self.sync_animation(now, false, "state-change");

        let idle = self.state.mode == Mode::Idle;
        let animating = self.state.is_animating() && (!idle || self.config.idle_animation);
        if animating || self.dirty {
            if self.frame_due(now) {
                self.render_now(now);
            }
        }
        self.schedule(event_loop);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_config();
        self.audio.stop();
        tracing::info!("bye");
    }
}

// ── CLI ─────────────────────────────────────────────────────────────

const USAGE: &str = "\
超能 NONO 桌宠

用法:
  nono [--assets DIR] [--trace] [--play ID]
                                           启动桌宠（ID 如 action:1:cube、
                                           expression:1:happy、walk:1:down、form:2、
                                           follow）
  nono --check [--assets DIR]              校验素材目录并退出
  nono --list                              列出素材动作/表情
  nono --render-frame <form> <kind> <slug> <frame> <scale> <out.png>
                                           渲染单帧为 PNG（kind: walk|action|expression）
  nono --render-icon <out.png> [size]      生成托盘图标
环境变量:
  NONO_ASSETS                              覆盖素材目录
";

enum Command {
    Run { assets: PathBuf, trace: bool, play: Option<String>, play_delay: f32, tint: Option<[u8; 3]> },
    Check { assets: PathBuf },
    List { assets: PathBuf },
    RenderFrame { assets: PathBuf, form: u8, kind: String, slug: String, frame: f32, scale: f32, out: PathBuf, tint: Option<[u8; 3]> },
    RenderIcon { assets: PathBuf, out: PathBuf, size: u32, tint: Option<[u8; 3]> },
    DumpFrames { assets: PathBuf, out: PathBuf, form: Option<u8>, scale: f32, count: usize },
    DumpLayers { assets: PathBuf, out: PathBuf, form: Option<u8>, scale: f32, count: usize },
}

fn parse_args() -> Result<Command> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut assets = config::default_asset_root();
    let mut trace = false;
    let mut play: Option<String> = None;
    let mut play_delay = 0.0f32;
    let mut tint: Option<[u8; 3]> = None;
    let mut dump: Option<PathBuf> = None;
    let mut dump_layers: Option<PathBuf> = None;
    let mut dump_form: Option<u8> = None;
    let mut dump_scale = 2.0f32;
    let mut dump_count = 10usize;
    let mut command: Option<Command> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--assets" => {
                i += 1;
                let value = args.get(i).context("--assets needs a directory")?;
                assets = PathBuf::from(value);
            }
            "--trace" => trace = true,
            "--tint" => {
                i += 1;
                let value = args.get(i).context("--tint needs rrggbb or r,g,b")?;
                tint = parse_tint(value);
                if tint.is_none() {
                    bail!("invalid --tint {value:?} (use rrggbb or r,g,b)");
                }
            }
            "--play" => {
                i += 1;
                play = Some(args.get(i).context("--play needs an id like action:1:cube")?.clone());
            }
            "--play-after" => {
                i += 1;
                play_delay = args
                    .get(i)
                    .context("--play-after needs a delay in seconds")?
                    .parse::<f32>()
                    .context("delay must be a number")?;
                i += 1;
                play = Some(args.get(i).context("--play-after needs an id")?.clone());
            }
            "--dump-frames" => {
                i += 1;
                dump = Some(PathBuf::from(args.get(i).context("--dump-frames needs a directory")?));
            }
            "--dump-layers" => {
                i += 1;
                dump_layers = Some(PathBuf::from(args.get(i).context("--dump-layers needs a directory")?));
            }
            "--form" => {
                i += 1;
                dump_form = Some(args.get(i).context("--form needs 1, 2 or 3")?.parse::<u8>()?);
            }
            "--scale" => {
                i += 1;
                dump_scale = args.get(i).context("--scale needs a number")?.parse::<f32>()?;
            }
            "--count" => {
                i += 1;
                dump_count = args.get(i).context("--count needs a number")?.parse::<usize>()?;
            }
            "--check" => command = Some(Command::Check { assets: assets.clone() }),
            "--list" => command = Some(Command::List { assets: assets.clone() }),
            "--render-frame" => {
                if i + 6 >= args.len() {
                    bail!("--render-frame needs <form> <kind> <slug> <frame> <scale> <out.png>");
                }
                let form = args[i + 1].parse::<u8>().context("form must be 1, 2 or 3")?;
                let kind = args[i + 2].clone();
                let slug = args[i + 3].clone();
                let frame = args[i + 4].parse::<f32>().context("frame must be a number")?;
                let scale = args[i + 5].parse::<f32>().context("scale must be a number")?;
                let out = PathBuf::from(&args[i + 6]);
                command = Some(Command::RenderFrame { assets: assets.clone(), form, kind, slug, frame, scale, out, tint });
                i += 6;
            }
            "--render-icon" => {
                if i + 1 >= args.len() {
                    bail!("--render-icon needs <out.png> [size]");
                }
                let out = PathBuf::from(&args[i + 1]);
                let size = args.get(i + 2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(64);
                command = Some(Command::RenderIcon { assets: assets.clone(), out, size, tint });
                i += if args.get(i + 2).is_some() { 2 } else { 1 };
            }
            "--help" | "-h" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}\n\n{USAGE}"),
        }
        i += 1;
    }

    if let Some(out) = dump {
        command = Some(Command::DumpFrames { assets: assets.clone(), out, form: dump_form, scale: dump_scale, count: dump_count });
    }
    if let Some(out) = dump_layers {
        command = Some(Command::DumpLayers { assets: assets.clone(), out, form: dump_form, scale: dump_scale, count: dump_count });
    }
    Ok(command.unwrap_or(Command::Run { assets, trace, play, play_delay, tint }))
}

fn parse_tint(value: &str) -> Option<[u8; 3]> {
    if value.contains(',') {
        let parts = value
            .split(',')
            .filter_map(|p| p.trim().parse::<u8>().ok())
            .collect::<Vec<_>>();
        if parts.len() == 3 {
            return Some([parts[0], parts[1], parts[2]]);
        }
        return None;
    }
    let hex = value.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some([r, g, b]);
    }
    None
}

fn init_tracing(trace: bool) {
    let level = if trace {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .without_time()
        .try_init();
}

fn spec_for(catalog: &Catalog, form: u8, kind: &str, slug: &str) -> Result<nono_core::AnimationSpec> {
    let spec = match kind {
        "walk" => {
            let dir = Direction::from_key(slug)
                .with_context(|| format!("unknown direction {slug}"))?;
            catalog.walk_spec(form, dir)?
        }
        "action" => catalog.action_spec(form, slug)?,
        "expression" => catalog.expression_spec(form, slug)?,
        other => bail!("unknown kind {other} (use walk|action|expression)"),
    };
    Ok(spec)
}

fn render_icon(assets: PathBuf, out: PathBuf, size: u32, tint: Option<[u8; 3]>) -> Result<()> {
    let catalog = Catalog::load(&assets)?;
    let spec = catalog.walk_spec(catalog.default_form(), Direction::Down)?;
    let mut renderer = Renderer::new()?;
    renderer.set_tint(tint);
    // Render at the icon width so the vector art is rasterised crisply.
    let scale = size as f32 / spec.canvas.0;
    renderer.load(&spec, scale)?;
    renderer.render_frame(0.0)?;
    let (w, h) = renderer.size();
    let image = image::RgbaImage::from_raw(w, h, renderer.rgba8().to_vec())
        .context("renderer produced an unexpected buffer size")?;
    // Crop the head/upper body into a square.
    let side = w.min(h).min(size);
    let crop = image::imageops::crop_imm(&image, (w - side) / 2, 0, side, side).to_image();
    let icon = image::imageops::resize(&crop, size, size, image::imageops::FilterType::Lanczos3);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    icon.save(&out)?;
    println!("wrote {} ({}x{})", out.display(), size, size);
    Ok(())
}

fn main() -> Result<()> {
    let command = parse_args()?;
    match command {
        Command::Check { assets } => {
            init_tracing(false);
            let catalog = Catalog::load(&assets)
                .with_context(|| format!("loading {}", assets.display()))?;
            let mut total = 0;
            for form in catalog.forms() {
                total += form.actions.len() + form.expressions.len() + form.walk.directions.len();
                println!(
                    "{}: {} actions, {} expressions, {} walk directions",
                    form.name,
                    form.actions.len(),
                    form.expressions.len(),
                    form.walk.directions.len()
                );
            }
            println!("ok: {} animations in {}", total, assets.display());
            Ok(())
        }
        Command::List { assets } => {
            init_tracing(false);
            let catalog = Catalog::load(&assets)?;
            for form in catalog.forms() {
                println!("{}", form.name);
                println!("  动作: {}", form.actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(" / "));
                println!("  表情: {}", form.expressions.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(" / "));
            }
            Ok(())
        }
        Command::RenderFrame { assets, form, kind, slug, frame, scale, out, tint } => {
            init_tracing(false);
            let catalog = Catalog::load(&assets)?;
            let spec = spec_for(&catalog, form, &kind, &slug)?;
            let mut renderer = Renderer::new()?;
            renderer.set_tint(tint);
            renderer.render_png(&spec, scale, frame, &out)?;
            println!(
                "wrote {} ({}x{}, form {form} {kind} {slug} frame {frame})",
                out.display(),
                renderer.size().0,
                renderer.size().1
            );
            Ok(())
        }
        Command::RenderIcon { assets, out, size, tint } => {
            init_tracing(false);
            render_icon(assets, out, size, tint)
        }
        Command::DumpFrames { assets, out, form, scale, count } => {
            init_tracing(false);
            dump_frames(assets, out, form, scale, count)
        }
        Command::DumpLayers { assets, out, form, scale, count } => {
            init_tracing(false);
            dump_layers(assets, out, form, scale, count)
        }
        Command::Run { assets, trace, play, play_delay, tint } => run(assets, trace, play, play_delay, tint),
    }
}

fn walk_into(specs: &mut Vec<nono_core::AnimationSpec>, catalog: &Catalog, form: u8) {
    for dir in Direction::ALL {
        if let Ok(spec) = catalog.walk_spec(form, dir) {
            specs.push(spec);
        }
    }
    for a in catalog.action_infos(form) {
        if let Ok(spec) = catalog.action_spec(form, &a.slug) {
            specs.push(spec);
        }
    }
    for e in catalog.expression_infos(form) {
        if let Ok(spec) = catalog.expression_spec(form, &e.slug) {
            specs.push(spec);
        }
    }
}

fn dump_layers(assets: PathBuf, out: PathBuf, form: Option<u8>, scale: f32, count: usize) -> Result<()> {
    let catalog = Catalog::load(&assets)?;
    let mut renderer = Renderer::new()?;
    let layers_dir = out.join("layers");
    std::fs::create_dir_all(&layers_dir)?;
    for f in catalog.forms() {
        if let Some(want) = form {
            if f.id != want {
                continue;
            }
        }
        let mut specs = Vec::new();
        walk_into(&mut specs, &catalog, f.id);
        for spec in specs {
            let flat = spec.id.replace('/', "_");
            let bytes = std::fs::read(&spec.file)?;
            let mut root: serde_json::Value = serde_json::from_slice(&bytes)?;
            let units = nono_render::parts::color_map(&mut root);
            let temp = layers_dir.join(format!("_{flat}.tmp.json"));
            std::fs::write(&temp, serde_json::to_vec(&root)?)?;
            let mut temp_spec = spec.clone();
            temp_spec.file = temp.clone();
            let total = spec.effective_frames().max(1) as usize;
            let want = count.max(1).min(total);
            let dir = layers_dir.join(&flat);
            std::fs::create_dir_all(&dir)?;
            for j in 0..want {
                let frame = ((j * total) / want) as f32;
                renderer.render_png(&temp_spec, scale, frame, &dir.join(format!("{j:02}.png")))?;
            }
            let meta: Vec<serde_json::Value> = units
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "layer": u.layer,
                        "paths": u.paths,
                        "color": u.color,
                        "nm": u.name,
                    })
                })
                .collect();
            std::fs::write(
                layers_dir.join(format!("{flat}.json")),
                serde_json::to_vec_pretty(&serde_json::json!({ "units": meta }))?,
            )?;
            let _ = std::fs::remove_file(&temp);
            println!("parts {} ({} units, {} frames)", spec.id, units.len(), want);
        }
    }
    Ok(())
}

fn dump_frames(assets: PathBuf, out: PathBuf, form: Option<u8>, scale: f32, count: usize) -> Result<()> {
    let catalog = Catalog::load(&assets)?;
    let mut renderer = Renderer::new()?;
    let frames_dir = out.join("frames");
    std::fs::create_dir_all(&frames_dir)?;
    let mut specs = Vec::new();
    for f in catalog.forms() {
        if let Some(want) = form {
            if f.id != want {
                continue;
            }
        }
        for dir in Direction::ALL {
            if let Ok(spec) = catalog.walk_spec(f.id, dir) {
                specs.push(spec);
            }
        }
        for a in catalog.action_infos(f.id) {
            if let Ok(spec) = catalog.action_spec(f.id, &a.slug) {
                specs.push(spec);
            }
        }
        for e in catalog.expression_infos(f.id) {
            if let Ok(spec) = catalog.expression_spec(f.id, &e.slug) {
                specs.push(spec);
            }
        }
    }
    let mut entries = Vec::new();
    for spec in specs {
        let flat = spec.id.replace('/', "_");
        let dir = frames_dir.join(&flat);
        std::fs::create_dir_all(&dir)?;
        let total = spec.effective_frames().max(1) as usize;
        let want = count.max(1).min(total);
        let mut files = Vec::new();
        for j in 0..want {
            let frame = ((j * total) / want) as f32;
            let path = dir.join(format!("{j:02}.png"));
            renderer.render_png(&spec, scale, frame, &path)?;
            files.push(format!("frames/{flat}/{j:02}.png"));
        }
        let w = (spec.canvas.0 * scale).round() as u32;
        let h = (spec.canvas.1 * scale).round() as u32;
        let slug = spec.id.rsplit('/').next().unwrap_or("").to_string();
        entries.push(serde_json::json!({
            "id": spec.id,
            "form": spec.form,
            "kind": format!("{:?}", spec.kind).to_lowercase(),
            "slug": slug,
            "canvas": [spec.canvas.0, spec.canvas.1],
            "frame_size": [w, h],
            "frames": files,
        }));
        println!("dumped {} ({} frames)", spec.id, want);
    }
    let manifest = serde_json::json!({ "animations": entries });
    std::fs::write(out.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
    println!("manifest: {}", out.join("manifest.json").display());
    Ok(())
}

fn run(assets: PathBuf, trace: bool, play: Option<String>, play_delay: f32, tint: Option<[u8; 3]>) -> Result<()> {
    init_tracing(trace);
    let catalog = Catalog::load(&assets)
        .with_context(|| format!("loading assets from {}", assets.display()))?;
    let config_path = config::config_path();
    let mut config = Config::load(&config_path);
    if let Some(tint) = tint {
        config.tint = Some(tint);
    }
    tracing::info!(
        assets = %assets.display(),
        config = %config_path.display(),
        form = config.form,
        scale = config.scale,
        "starting 超能 NONO"
    );

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("creating the winit event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    let mut app = App::new(assets, catalog, config, config_path, proxy)?;
    app.initial_play = play;
    app.play_delay = play_delay;
    event_loop.run_app(&mut app).context("running the event loop")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_key_uses_whole_animation_frames() {
        // 25 fps with a ~60 Hz wake-up must not render more than 25 times a second.
        assert_eq!(frame_key(0.0), 0);
        assert_eq!(frame_key(0.99), 0);
        assert_eq!(frame_key(1.0), 1);
        assert_eq!(frame_key(12.9), 12);
    }

    #[test]
    fn icon_render_works() {
        let assets = config::default_asset_root();
        if !assets.join("catalog.json").is_file() {
            return;
        }
        let out = std::env::temp_dir().join("nono-test-icon.png");
        render_icon(assets, out.clone(), 48, None).unwrap();
        let img = image::open(&out).unwrap();
        assert_eq!(img.width(), 48);
        assert!(img.to_rgba8().chunks(4).any(|px| px[3] > 0));
        let _ = std::fs::remove_file(out);
    }
}

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
    bounds: Rect,
    last_anim_id: String,
    last_frame_key: i64,
    dirty: bool,
    visible: bool,
    render_count: u64,
    render_time_sum_ms: f64,
    /// Debug/automation hook: an animation id to start right after setup.
    initial_play: Option<String>,
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
            bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
            last_anim_id: String::new(),
            last_frame_key: i64::MIN,
            dirty: true,
            visible: true,
            initial_play: None,
            render_count: 0,
            render_time_sum_ms: 0.0,
        })
    }

    // ── setup ───────────────────────────────────────────────────────

    fn setup(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        let (bounds, default_anchor, dpi) = match &monitor {
            Some(monitor) => {
                let pos = monitor.position();
                let size = monitor.size();
                let bounds = Rect::new(
                    pos.x as f32,
                    pos.y as f32,
                    size.width as f32,
                    size.height as f32,
                );
                let anchor = Point::new(
                    bounds.x + bounds.w * 0.75,
                    bounds.y + bounds.h * 0.82,
                );
                (bounds, anchor, monitor.scale_factor())
            }
            None => (Rect::new(0.0, 0.0, 1440.0, 900.0), Point::new(1080.0, 738.0), 2.0),
        };
        self.bounds = bounds;

        let anchor = self
            .config
            .anchor
            .unwrap_or(default_anchor)
            .clamp(bounds);
        let now = Instant::now();
        self.state = PetState::new(&self.catalog, self.config.form, anchor, self.config.scale, dpi as f32, now);

        #[cfg(target_os = "macos")]
        nono_platform::macos::set_accessory_activation_policy();

        tracing::debug!(
            monitor_pos = ?monitor.as_ref().map(|m| m.position()),
            monitor_size = ?monitor.as_ref().map(|m| m.size()),
            dpi,
            anchor = ?(anchor.x, anchor.y),
            "monitor geometry"
        );
        let rect = self.state.window_rect();
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
            let rect = self.state.window_rect();
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

        self.sync_animation(now, true);
        self.render_now(now);
        self.bus.publish(Event::ChangeForm { form: self.state.form });

        if let Some(play) = self.initial_play.clone() {
            self.play_scripted(&play, now);
        }
        Ok(())
    }

    // ── animation / rendering ───────────────────────────────────────

    fn sync_animation(&mut self, now: Instant, force: bool) {
        let anim_id = self.state.anim.id.clone();
        if !force && anim_id == self.last_anim_id {
            return;
        }
        let rect = self.state.window_rect();
        let dpi = self.state.dpi;
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

        let scale = self.state.scale * self.state.dpi;
        match self.renderer.load(&self.state.anim, scale) {
            Ok(()) => {
                let (w, h) = self.renderer.size();
                if let Some(presenter) = self.presenter.as_mut() {
                    if let Err(err) = presenter.resize(w, h, dpi as f64) {
                        tracing::warn!(%err, "presenter resize failed");
                    }
                }
            }
            Err(err) => tracing::warn!(%err, anim = %anim_id, "rendering the animation failed"),
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
        let (width, height) = self.renderer.size();
        if let Some(presenter) = self.presenter.as_mut() {
            if let Err(err) = presenter.present(self.renderer.rgba8(), width, height, self.state.dpi as f64) {
                tracing::warn!(%err, "present failed");
            }
        }
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
        let rect = self.state.window_rect();
        window.set_outer_position(PhysicalPosition::new(rect.x as i32, rect.y as i32));
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
            self.sync_animation(now, false);
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
            self.sync_animation(now, false);
            self.render_now(now);
        }
    }

    /// Handles the `--play` automation hook (`action:1:cube`,
    /// `expression:1:happy`, `walk:1:down`, `form:2`).
    fn play_scripted(&mut self, spec: &str, now: Instant) {
        let parts = spec.split(':').collect::<Vec<_>>();
        match parts.as_slice() {
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
                        self.sync_animation(now, true);
                    }
                }
            }
            _ => tracing::warn!(spec, "unrecognised --play spec"),
        }
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
                }
            }
        }
    }

    fn switch_form(&mut self, form: u8, now: Instant) {
        if !self.state.set_form(&self.catalog, form, now) {
            return;
        }
        self.config.form = form;
        self.sync_animation(now, false);
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
        self.sync_animation(now, true);
        self.render_now(now);
        self.save_config();
        self.bus.publish(Event::SetScale { scale: self.config.scale });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
    }

    fn menu_action(&mut self, rest: &str, now: Instant) {
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
            self.sync_animation(now, false);
            self.render_now(now);
        }
    }

    fn menu_expression(&mut self, rest: &str, now: Instant) {
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
            self.sync_animation(now, false);
            self.render_now(now);
        }
    }

    // ── scheduling ──────────────────────────────────────────────────

    fn schedule(&self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let next = if self.state.is_animating() {
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
                self.state.set_dpi(scale_factor as f32);
                self.sync_animation(Instant::now(), true);
                self.render_now(Instant::now());
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
                            let new_x = current.x as f32 + dx;
                            let new_y = current.y as f32 + dy;
                            window.set_outer_position(PhysicalPosition::new(
                                new_x.round() as i32,
                                new_y.round() as i32,
                            ));
                            let s = self.state.scale * self.state.dpi;
                            self.state.anchor = Point::new(
                                new_x + self.state.anim.pet_anchor.0 * s,
                                new_y + self.state.anim.pet_anchor.1 * s,
                            );
                        }
                    }
                } else {
                    let _ = previous;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    self.drag = Some(DragState { press_cursor: self.cursor, distance: 0.0 });
                    let now = Instant::now();
                    if self.state.mode == Mode::Walk {
                        self.state.stop_walk(&self.catalog, now);
                        self.sync_animation(now, false);
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

        let tick = self.state.tick(&self.catalog, now);
        if tick.finished {
            let anim_id = self.state.anim.id.clone();
            self.bus.publish(Event::AnimationFinished { anim_id });
            if self.state.after_finish(&self.catalog, now) {
                self.sync_animation(now, false);
            }
        }
        if tick.moved {
            self.position_window();
        }

        if self.behavior.update(&mut self.state, &self.catalog, self.bounds, now) {
            self.sync_animation(now, false);
        }
        self.sync_animation(now, false);

        if self.state.is_animating() || self.dirty {
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
                                           expression:1:happy、walk:1:down、form:2）
  nono --check [--assets DIR]              校验素材目录并退出
  nono --list                              列出素材动作/表情
  nono --render-frame <form> <kind> <slug> <frame> <scale> <out.png>
                                           渲染单帧为 PNG（kind: walk|action|expression）
  nono --render-icon <out.png> [size]      生成托盘图标
环境变量:
  NONO_ASSETS                              覆盖素材目录
";

enum Command {
    Run { assets: PathBuf, trace: bool, play: Option<String> },
    Check { assets: PathBuf },
    List { assets: PathBuf },
    RenderFrame { assets: PathBuf, form: u8, kind: String, slug: String, frame: f32, scale: f32, out: PathBuf },
    RenderIcon { assets: PathBuf, out: PathBuf, size: u32 },
}

fn parse_args() -> Result<Command> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut assets = config::default_asset_root();
    let mut trace = false;
    let mut play: Option<String> = None;
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
            "--play" => {
                i += 1;
                play = Some(args.get(i).context("--play needs an id like action:1:cube")?.clone());
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
                command = Some(Command::RenderFrame { assets: assets.clone(), form, kind, slug, frame, scale, out });
                i += 6;
            }
            "--render-icon" => {
                if i + 1 >= args.len() {
                    bail!("--render-icon needs <out.png> [size]");
                }
                let out = PathBuf::from(&args[i + 1]);
                let size = args.get(i + 2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(64);
                command = Some(Command::RenderIcon { assets: assets.clone(), out, size });
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

    Ok(command.unwrap_or(Command::Run { assets, trace, play }))
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

fn render_icon(assets: PathBuf, out: PathBuf, size: u32) -> Result<()> {
    let catalog = Catalog::load(&assets)?;
    let spec = catalog.walk_spec(catalog.default_form(), Direction::Down)?;
    let mut renderer = Renderer::new()?;
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
        Command::RenderFrame { assets, form, kind, slug, frame, scale, out } => {
            init_tracing(false);
            let catalog = Catalog::load(&assets)?;
            let spec = spec_for(&catalog, form, &kind, &slug)?;
            let mut renderer = Renderer::new()?;
            renderer.render_png(&spec, scale, frame, &out)?;
            println!(
                "wrote {} ({}x{}, form {form} {kind} {slug} frame {frame})",
                out.display(),
                renderer.size().0,
                renderer.size().1
            );
            Ok(())
        }
        Command::RenderIcon { assets, out, size } => {
            init_tracing(false);
            render_icon(assets, out, size)
        }
        Command::Run { assets, trace, play } => run(assets, trace, play),
    }
}

fn run(assets: PathBuf, trace: bool, play: Option<String>) -> Result<()> {
    init_tracing(trace);
    let catalog = Catalog::load(&assets)
        .with_context(|| format!("loading assets from {}", assets.display()))?;
    let config_path = config::config_path();
    let config = Config::load(&config_path);
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
        render_icon(assets, out.clone(), 48).unwrap();
        let img = image::open(&out).unwrap();
        assert_eq!(img.width(), 48);
        assert!(img.to_rgba8().chunks(4).any(|px| px[3] > 0));
        let _ = std::fs::remove_file(out);
    }
}

//! 超能 NONO desktop pet — application shell.
//!
//! Responsibilities of this crate are deliberately thin: parse the CLI, own the
//! winit event loop, and translate state-machine changes into window geometry,
//! rendering and audio.  All behaviour lives in `nono-core`.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{bail, Context, Result};
use nono_core::{
    config, speed, trail, Behavior, Bus, Catalog, Config, Direction, Display, Event, Mode,
    PetState, Point, Rect, RoamArea,
};
use nono_platform::{
    build_tray, create_pet_window, create_presenter, AudioPlayer, Presenter, TrayMenu,
};
use nono_render::{compose_trail, strip_ground_shadow, GhostFrame, GlowSettings, Renderer};
use rand::Rng;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

/// How often the event loop wakes while an animation is playing.  Rendering
/// itself is throttled to the animation's own frame rate.
const ANIMATION_POLL: Duration = Duration::from_millis(16);
/// Click vs drag threshold, in physical pixels.
const DRAG_THRESHOLD: f32 = 5.0;
/// Maximum number of afterimage frames kept as a safety net; the configured
/// lifetime/interval normally keeps this far lower.
const TRAIL_MAX_SAMPLES: usize = 48;
/// While the trail is fading out after a walk, redraw at most at this rate.
const TRAIL_FADE_FRAME: Duration = Duration::from_millis(40);
/// Config hot-reload poll interval while the debug watcher is enabled.
const CONFIG_POLL: Duration = Duration::from_millis(500);
/// How often display hot-plug is polled (cheap; only reacts to id changes).
const DISPLAY_POLL: Duration = Duration::from_secs(5);
/// Glow margin used by the fixed walking stage, in canvas units.
///
/// This must be independent of the current walk direction.  Form 2 directions
/// carry 11 or 15 units of glow padding; using a constant covers both without
/// resizing the window when the pet turns.
const WALK_GLOW_MARGIN_UNITS: f32 = 24.0;

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(String),
    Bus,
}

struct DragState {
    /// Global logical cursor position when the button went down.
    press_global: Point,
    /// Pet feet position when the drag started.
    press_anchor: Point,
    /// Accumulated cursor travel (logical points), used to tell clicks from drags.
    distance: f32,
}

/// A rendered walk frame kept for the motion trail.
struct TrailSample {
    /// Premultiplied RGBA8 of the tight (canvas-sized) pet frame, with the
    /// ground shadow already removed so it does not leave its own trail.
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    /// Pet feet position when this frame was drawn (logical global points).
    anchor: Point,
    /// Physical pixel position of the feet inside `pixels`.  Frames from
    /// different walk directions can have different glow margins/sizes, so the
    /// ghost must be pinned by its own anchor instead of assuming it matches
    /// the current frame.
    anchor_in_frame: (f32, f32),
    /// When this afterimage was spawned; its age drives the fade.
    at: Instant,
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
    /// Connected displays, primary first (`NSScreen` usable frames in logical
    /// global points on macOS).
    displays: Vec<Display>,
    /// Platform ids of the displays the pet may roam on (never empty).
    selected_displays: Vec<u32>,
    /// Allowed roaming region derived from `selected_displays`.
    roam: RoamArea,
    /// Next hot-plug poll.
    display_poll_next: Instant,
    crop_buf: Vec<u8>,
    /// Transparent stage with room for the walking afterimages.
    trail_buf: Vec<u8>,
    trail_samples: VecDeque<TrailSample>,
    /// Padding of the last walking stage, kept while the ghosts fade out.
    trail_stage_pad: u32,
    /// Stable stage extents relative to the pet anchor (physical px).  Grown
    /// during a walk and only reset when the trail is gone, so samples expiring
    /// does not resize the window every interval.
    trail_extents: Option<Rect>,
    /// Last time a frame was rendered (used to throttle the fade-out).
    last_render_at: Instant,
    /// Last applied window size (physical px), to avoid redundant resizes.
    last_window_size: Option<(u32, u32)>,
    /// Last applied click-through state.
    last_interactive: Option<bool>,
    /// Debug tuning: poll the config file and hot-reload the trail fields.
    config_watch: bool,
    config_watch_next: Instant,
    config_mtime: Option<SystemTime>,
    last_anim_id: String,
    last_frame_key: i64,
    dirty: bool,
    visible: bool,
    render_count: u64,
    render_time_sum_ms: f64,
    /// 60 Hz cached trail presents, tracked separately from full renders.
    cached_present_count: u64,
    cached_present_sum_ms: f64,
    /// Debug/automation hook: an animation id to start right after setup.
    initial_play: Option<String>,
    /// Mouse-follow mode: when this expires, following stops.
    follow_until: Option<Instant>,
    follow_next: Instant,
    /// Delay before `initial_play` runs (debug/automation).
    play_delay: f32,
    scheduled_play: Option<(Instant, String)>,
}

/// Maps the user-facing glow settings onto the renderer's.
fn glow_settings(config: &Config) -> GlowSettings {
    // `NONO_GLOW_SIGMA` / `NONO_GLOW_ALPHA` override the config for quick A/B
    // tuning without editing `config.toml`.
    let env_f32 = |key: &str| std::env::var(key).ok().and_then(|v| v.parse::<f32>().ok());
    GlowSettings {
        enabled: config.glow,
        sigma_scale: env_f32("NONO_GLOW_SIGMA").unwrap_or(config.glow_sigma),
        alpha_scale: env_f32("NONO_GLOW_ALPHA").unwrap_or(config.glow_alpha),
        ..GlowSettings::default()
    }
}

fn fallback_display() -> Display {
    Display {
        id: 0,
        name: "显示器".into(),
        bounds: Rect::new(0.0, 0.0, 1440.0, 900.0),
        scale: 2.0,
        primary: true,
    }
}

fn distance_to_rect(point: Point, rect: Rect) -> f32 {
    let x = point.x.clamp(rect.x, rect.right());
    let y = point.y.clamp(rect.y, rect.bottom());
    ((point.x - x).powi(2) + (point.y - y).powi(2)).sqrt()
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Top-left of a ghost frame in global physical pixels.
///
/// `anchor` is the screen-space feet position in logical points and
/// `anchor_in_frame` is the feet pixel inside that particular frame.  Using the
/// frame's own anchor makes ghosts from differently-padded walk directions
/// line up correctly.
fn ghost_frame_origin(anchor: Point, anchor_in_frame: (f32, f32), dpi: f32) -> Point {
    Point::new(
        anchor.x * dpi - anchor_in_frame.0,
        anchor.y * dpi - anchor_in_frame.1,
    )
}

/// Whether `cursor` is inside the round body hit circle.
///
/// `hit` is `(dx, dy, radius)` in placement-scaled canvas units relative to the
/// pet anchor (the renderer's `body_hit` contract); `scale` is the user zoom.
fn body_hit_contains(anchor: Point, hit: (f32, f32, f32), scale: f32, cursor: Point) -> bool {
    let (dx, dy, radius) = hit;
    let s = scale.max(0.01);
    let cx = anchor.x + dx * s;
    let cy = anchor.y + dy * s;
    let rr = (radius * s).max(10.0);
    (cursor.x - cx).powi(2) + (cursor.y - cy).powi(2) <= rr * rr
}

#[derive(Clone, Copy)]
enum ScreenEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Whether the given edge of `display` touches another enabled display.
///
/// Shared edges must not get the pet-silhouette margin, otherwise the pet could
/// never cross the seam between two screens.
fn edge_has_neighbor(all: &[&Display], display: &Display, edge: ScreenEdge) -> bool {
    let bounds = display.bounds;
    // Sample along the edge because two displays can share only part of it.
    [0.1, 0.3, 0.5, 0.7, 0.9].iter().any(|fraction| {
        let probe = match edge {
            ScreenEdge::Left => Point::new(bounds.x - 1.0, bounds.y + bounds.h * fraction),
            ScreenEdge::Right => Point::new(bounds.right() + 1.0, bounds.y + bounds.h * fraction),
            ScreenEdge::Top => Point::new(bounds.x + bounds.w * fraction, bounds.y - 1.0),
            ScreenEdge::Bottom => Point::new(bounds.x + bounds.w * fraction, bounds.bottom() + 1.0),
        };
        all.iter()
            .any(|other| other.id != display.id && other.bounds.contains(probe))
    })
}

/// Builds the anchor region for the selected displays.
///
/// `margins` are the pet-silhouette extents in canvas units; shared display
/// edges get no margin so the two rectangles touch and the pet can cross.
fn compute_roam_rects(
    displays: &[Display],
    selected: &[u32],
    margins: (f32, f32, f32, f32),
    scale: f32,
) -> RoamArea {
    let selected_displays = displays
        .iter()
        .filter(|display| selected.contains(&display.id))
        .collect::<Vec<_>>();
    let (left, top, right, bottom) = margins;
    let mut rects = Vec::new();
    for display in &selected_displays {
        let bounds = display.bounds;
        let x0 = bounds.x
            + if edge_has_neighbor(&selected_displays, display, ScreenEdge::Left) {
                0.0
            } else {
                left * scale
            };
        let y0 = bounds.y
            + if edge_has_neighbor(&selected_displays, display, ScreenEdge::Top) {
                0.0
            } else {
                top * scale
            };
        let x1 = bounds.right()
            - if edge_has_neighbor(&selected_displays, display, ScreenEdge::Right) {
                0.0
            } else {
                right * scale
            };
        let y1 = bounds.bottom()
            - if edge_has_neighbor(&selected_displays, display, ScreenEdge::Bottom) {
                0.0
            } else {
                bottom * scale
            };
        rects.push(if x1 <= x0 || y1 <= y0 {
            bounds
        } else {
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        });
    }
    if rects.is_empty() {
        rects.push(
            displays
                .iter()
                .find(|display| display.primary)
                .or_else(|| displays.first())
                .map(|display| display.bounds)
                .unwrap_or_else(|| fallback_display().bounds),
        );
    }
    RoamArea::new(rects)
}

/// Enumerates displays with usable bounds in logical global points.
///
/// macOS exposes the exact per-screen frame; elsewhere winit's physical monitor
/// geometry is converted with each monitor's scale factor (presentation is not
/// implemented on those platforms yet anyway).
fn enumerate_displays(event_loop: &ActiveEventLoop) -> Vec<Display> {
    if let Some(displays) = nono_platform::displays() {
        if !displays.is_empty() {
            return displays;
        }
    }
    let primary = event_loop.primary_monitor();
    let mut displays = Vec::new();
    for (index, monitor) in event_loop.available_monitors().enumerate() {
        let scale = monitor.scale_factor() as f32;
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let pos = monitor.position();
        let size = monitor.size();
        displays.push(Display {
            id: index as u32,
            name: monitor
                .name()
                .unwrap_or_else(|| format!("显示器 {}", index + 1)),
            bounds: Rect::new(
                pos.x as f32 / scale,
                pos.y as f32 / scale,
                size.width as f32 / scale,
                size.height as f32 / scale,
            ),
            scale,
            primary: primary.as_ref() == Some(&monitor),
        });
    }
    if displays.is_empty() {
        displays.push(fallback_display());
    }
    if !displays.iter().any(|display| display.primary) {
        displays[0].primary = true;
    }
    displays.sort_by(|a, b| {
        b.primary
            .cmp(&a.primary)
            .then(
                a.bounds
                    .x
                    .partial_cmp(&b.bounds.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                a.bounds
                    .y
                    .partial_cmp(&b.bounds.y)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    displays
}

/// Keeps the configured ids that still exist; falls back to the primary display
/// (never an empty selection).
fn resolve_display_ids(configured: Option<&[u32]>, displays: &[Display]) -> Vec<u32> {
    let mut selected = Vec::new();
    if let Some(ids) = configured {
        for id in ids {
            if displays.iter().any(|display| display.id == *id) && !selected.contains(id) {
                selected.push(*id);
            }
        }
    }
    if selected.is_empty() {
        if let Some(primary) = displays.iter().find(|display| display.primary) {
            selected.push(primary.id);
        } else if let Some(first) = displays.first() {
            selected.push(first.id);
        }
    }
    selected
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
        let mut renderer = Renderer::new().context("initialising the ThorVG renderer")?;
        renderer.set_tint_cache_dir(Some(config::cache_dir()));
        renderer.set_glow(glow_settings(&config));
        let state = PetState::new(
            &catalog,
            config.form,
            Point::new(200.0, 200.0),
            config.scale,
            2.0,
            now,
        );
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
            displays: Vec::new(),
            selected_displays: Vec::new(),
            roam: RoamArea::default(),
            display_poll_next: now + DISPLAY_POLL,
            crop_buf: Vec::new(),
            trail_buf: Vec::new(),
            trail_samples: VecDeque::new(),
            trail_stage_pad: 0,
            trail_extents: None,
            last_render_at: now,
            last_window_size: None,
            last_interactive: None,
            config_watch: false,
            config_watch_next: now,
            config_mtime: None,
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
            cached_present_count: 0,
            cached_present_sum_ms: 0.0,
        })
    }

    // ── setup ───────────────────────────────────────────────────────

    fn setup(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        self.displays = enumerate_displays(event_loop);
        self.selected_displays =
            resolve_display_ids(self.config.display_ids.as_deref(), &self.displays);
        self.config.display_ids = Some(self.selected_displays.clone());

        let start_display = self
            .summon_display()
            .cloned()
            .unwrap_or_else(fallback_display);
        let now = Instant::now();
        // Start on the primary display's centre by default.  With
        // `start_at_center = false` the last saved anchor is restored instead
        // (if that point is still on an enabled display).  If the primary
        // display is disabled, the first enabled display is used.
        let anchor = start_display.bounds.center();
        self.state = PetState::new(
            &self.catalog,
            self.config.form,
            anchor,
            self.config.scale,
            start_display.scale,
            now,
        );
        self.roam = self.compute_roam_area();
        self.state.clamp_to(&self.roam);
        if !self.config.start_at_center {
            if let Some(saved) = self.config.anchor {
                if self
                    .selected_display_refs()
                    .any(|display| display.bounds.contains(saved))
                {
                    self.state.anchor = saved;
                    if let Some(display) = self.display_for_anchor() {
                        self.state.set_dpi(display.scale);
                    }
                    self.roam = self.compute_roam_area();
                    self.state.clamp_to(&self.roam);
                } else {
                    tracing::info!(
                        saved = ?(saved.x, saved.y),
                        "saved position is not on an enabled display, starting at the centre"
                    );
                }
            }
        }
        tracing::debug!(
            displays = self.displays.len(),
            selected = ?self.selected_displays,
            dpi = self.state.dpi,
            anchor = ?(self.state.anchor.x, self.state.anchor.y),
            start_at_center = self.config.start_at_center,
            roam = ?self.roam.rects(),
            "display geometry"
        );

        #[cfg(target_os = "macos")]
        nono_platform::macos::set_accessory_activation_policy();

        let rect = self.visible_rect();
        let window = create_pet_window(event_loop, rect).context("creating the pet window")?;
        #[cfg(target_os = "macos")]
        {
            nono_platform::macos::tune_window(&window);
            nono_platform::macos::log_geometry(&window);
        }

        // The display scale factor is authoritative, but the window may land on
        // a display with a different scale.
        let actual_dpi = window.scale_factor() as f32;
        if (actual_dpi - self.state.dpi).abs() > 0.01 {
            self.state.set_dpi(actual_dpi);
            let rect = self.visible_rect();
            let _ = window.request_inner_size(LogicalSize::new(
                rect.w.max(1.0) as f64,
                rect.h.max(1.0) as f64,
            ));
            window.set_outer_position(LogicalPosition::new(rect.x as f64, rect.y as f64));
        }

        let presenter = create_presenter(&window).context("creating the transparent presenter")?;
        self.window = Some(window);
        self.presenter = Some(presenter);

        self.install_tray();

        self.audio.set_enabled(self.config.sound);
        self.audio.set_volume(self.config.volume);
        self.renderer.set_tint(self.config.tint);
        self.renderer.set_glow(glow_settings(&self.config));

        self.bus.publish(Event::ChangeForm {
            form: self.state.form,
        });

        // Startup is a summon at the chosen position (primary centre by
        // default, or the remembered anchor).  Render the summon directly
        // instead of drawing one idle frame first, so launch does not flash the
        // idle pose before the animation appears.
        if self.config.summon_animation && self.catalog.has_action(self.state.form, "summon") {
            let anchor = self.state.anchor;
            let dpi = self.state.dpi;
            self.summon_at(now, anchor, dpi, "startup");
        } else {
            self.sync_animation(now, true, "setup");
            self.render_now(now);
        }

        if let Some(play) = self.initial_play.clone() {
            if self.play_delay > 0.0 {
                self.scheduled_play = Some((now + Duration::from_secs_f32(self.play_delay), play));
            } else {
                self.play_scripted(&play, now);
            }
        }
        Ok(())
    }

    /// Rebuilds the tray icon/menu (used at startup and when displays change).
    fn install_tray(&mut self) {
        let icon_path = self.assets_root.join("icon/nono-tray.png");
        let icon = nono_platform::icon::load_icon(&icon_path)
            .unwrap_or_else(|| nono_platform::icon::fallback_icon(32));
        let proxy = self.proxy.clone();
        match build_tray(
            &self.catalog,
            &self.config,
            &self.displays,
            &self.selected_displays,
            icon,
            move |id| {
                let _ = proxy.send_event(UserEvent::Menu(id));
            },
        ) {
            Ok(tray) => {
                let old = self.tray.take();
                tray.sync_checks(&self.config);
                tray.sync_displays(&self.selected_displays);
                self.tray = Some(tray);
                drop(old);
            }
            Err(err) => tracing::warn!(%err, "tray icon unavailable"),
        }
    }

    // ── displays / summoning ────────────────────────────────────────

    /// Summons the pet to the centre of the primary display and plays the
    /// current form's summon animation (when enabled).
    ///
    /// Used by the tray action and when the current display disappears.  The
    /// destination is the primary display by default; if the user disabled it,
    /// the first enabled display is used so the pet never lands on a forbidden
    /// screen.  Startup can instead restore the saved anchor via
    /// [`App::summon_at`].
    fn summon(&mut self, now: Instant, reason: &'static str) {
        let Some(target) = self.summon_display().cloned() else {
            return;
        };
        self.summon_at(now, target.bounds.center(), target.scale, reason);
    }

    /// Summons to an explicit logical anchor (used by startup when the user
    /// chose "remember last position").  `dpi` must be the backing scale of the
    /// display that anchor is on.
    fn summon_at(&mut self, now: Instant, anchor: Point, dpi: f32, reason: &'static str) {
        if self.follow_until.take().is_some() {
            if let Some(tray) = &self.tray {
                tray.follow.set_checked(false);
            }
        }
        if self.state.mode == Mode::Walk {
            self.state.stop_walk(&self.catalog, now);
        }
        self.state.anchor = anchor;
        self.roam = self.compute_roam_area();
        self.state.clamp_to(&self.roam);
        if (self.state.dpi - dpi).abs() > 0.001 {
            self.state.set_dpi(dpi);
        }
        // Old afterimages belong to the previous location.
        self.trail_samples.clear();
        self.trail_stage_pad = 0;
        self.trail_extents = None;
        self.last_window_size = None;
        self.behavior.delay(now);
        self.show_pet();

        if self.config.summon_animation && self.catalog.has_action(self.state.form, "summon") {
            self.state.play_action(&self.catalog, "summon", now);
        } else {
            self.state.idle(&self.catalog, now);
        }
        // Force the load: the summon may reuse the same clip id at a different
        // DPI, and the window has to grow to the action's canvas.
        self.sync_animation(now, true, reason);
        self.render_now(now);
        self.bus.publish(Event::ChangeState {
            state: self.state.mode.as_str().into(),
        });
        self.bus.publish(Event::Summoned);
        self.save_config();
        tracing::info!(
            form = self.state.form,
            animated = self.config.summon_animation,
            anchor = ?(anchor.x, anchor.y),
            "NONO summoned"
        );
    }

    fn show_pet(&mut self) {
        if self.visible {
            return;
        }
        self.visible = true;
        if let Some(window) = &self.window {
            window.set_visible(true);
        }
        if let Some(tray) = &self.tray {
            tray.hide.set_text("隐藏 NONO");
        }
    }

    /// Picks up a real backing-scale change when the pet walks/drags onto a
    /// display with a different DPI, and reloads the current frame.
    fn sync_active_display_dpi(&mut self, now: Instant) -> bool {
        let Some(scale) = self.display_for_anchor().map(|display| display.scale) else {
            return false;
        };
        if (self.state.dpi - scale).abs() <= 0.001 {
            return false;
        }
        tracing::debug!(
            from = self.state.dpi,
            to = scale,
            "active display DPI changed"
        );
        self.state.set_dpi(scale);
        self.reload_preserving_frame(now);
        true
    }

    fn toggle_display(&mut self, id: u32, now: Instant) {
        if !self.displays.iter().any(|display| display.id == id) {
            return;
        }
        let mut selected = self.selected_displays.clone();
        if let Some(position) = selected.iter().position(|selected_id| *selected_id == id) {
            if selected.len() <= 1 {
                tracing::info!("at least one display must stay enabled");
                if let Some(tray) = &self.tray {
                    tray.sync_displays(&self.selected_displays);
                }
                return;
            }
            selected.remove(position);
        } else {
            selected.push(id);
        }
        self.apply_display_selection(selected, now, "display-toggle");
    }

    fn apply_display_selection(&mut self, selected: Vec<u32>, now: Instant, reason: &'static str) {
        self.selected_displays = selected;
        self.config.display_ids = Some(self.selected_displays.clone());
        self.roam = self.compute_roam_area();
        if let Some(tray) = &self.tray {
            tray.sync_displays(&self.selected_displays);
        }
        self.save_config();
        self.bus.publish(Event::SetDisplays {
            display_ids: self.selected_displays.clone(),
        });
        if !self.roam.contains(self.state.anchor) {
            // The pet was on a display that just got disabled; summoning it
            // back to the primary centre is the least surprising recovery.
            self.summon(now, reason);
        } else {
            self.state.clamp_to(&self.roam);
            self.render_now(now);
        }
    }

    /// Re-enumerates displays (menu action or hot-plug poll).
    fn rescan_displays(&mut self, event_loop: &ActiveEventLoop, now: Instant, force_tray: bool) {
        let displays = enumerate_displays(event_loop);
        if displays.is_empty() {
            return;
        }
        let changed = force_tray
            || displays.len() != self.displays.len()
            || displays
                .iter()
                .zip(&self.displays)
                .any(|(a, b)| a.id != b.id || (a.scale - b.scale).abs() > 0.001);
        self.displays = displays;
        self.selected_displays =
            resolve_display_ids(self.config.display_ids.as_deref(), &self.displays);
        self.config.display_ids = Some(self.selected_displays.clone());
        self.roam = self.compute_roam_area();
        if !self.roam.contains(self.state.anchor) {
            self.summon(now, "display-rescan");
        } else {
            self.state.clamp_to(&self.roam);
            self.sync_active_display_dpi(now);
        }
        if changed {
            self.install_tray();
            tracing::info!(
                displays = self.displays.len(),
                selected = ?self.selected_displays,
                "display list refreshed"
            );
        }
        self.save_config();
    }

    /// Cheap hot-plug check: only reacts when the set of display ids changes.
    fn poll_displays(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        self.display_poll_next = now + DISPLAY_POLL;
        let Some(displays) = nono_platform::displays() else {
            return;
        };
        let mut current = self
            .displays
            .iter()
            .map(|display| display.id)
            .collect::<Vec<_>>();
        let mut latest = displays
            .iter()
            .map(|display| display.id)
            .collect::<Vec<_>>();
        current.sort_unstable();
        latest.sort_unstable();
        if current != latest {
            tracing::info!(?current, ?latest, "display set changed");
            self.rescan_displays(event_loop, now, true);
        }
    }

    // ── animation / rendering ───────────────────────────────────────

    fn sync_animation(&mut self, now: Instant, force: bool, reason: &'static str) {
        let anim_id = self.state.anim.id.clone();
        if !force && anim_id == self.last_anim_id {
            return;
        }
        let rect = self.visible_rect();
        let scale = self.state.scale * self.state.dpi;
        // Load BEFORE resizing the window: a cold Lottie parse on the first play
        // must not leave an empty, already-enlarged window on screen.
        let load_started = Instant::now();
        match self.renderer.load(&self.state.anim, scale) {
            Ok(()) => {
                let load_time = load_started.elapsed();
                // Pause the playback clock for the parse/draw time.  A tinted
                // cube action can take a second to load; without this the clip
                // would already be at (or past) its last frame before it appears.
                self.state.compensate_pause(load_time);
                // The renderer may have grown the canvas for the glow halo.
                self.state.set_glow_pad(self.renderer.glow_pad());
                tracing::debug!(
                    anim = %anim_id,
                    load_ms = load_time.as_secs_f64() * 1000.0,
                    body_hit = ?self.renderer.body_hit(),
                    "animation loaded"
                );
            }
            Err(err) => {
                tracing::warn!(%err, anim = %anim_id, "rendering the animation failed");
                return;
            }
        }
        // While a trail stage is active the window moves together with the
        // recomposition in `present_now`, never on its own.
        if self.trail_pad_px() == 0 {
            self.apply_window_geometry();
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

    /// Playback multiplier for the form-1 walk cycle (idle + all 8 directions).
    fn walk_animation_speed(&self) -> f32 {
        if self.state.form == 1 && self.state.anim.kind == nono_core::catalog::AnimationKind::Walk {
            self.config.form1_walk_anim_speed
        } else {
            1.0
        }
    }

    fn render_now(&mut self, now: Instant) {
        let started = Instant::now();
        let frame = self.state.frame_at(now, self.walk_animation_speed());
        if let Err(err) = self.renderer.render_frame(frame) {
            tracing::warn!(%err, frame, "render failed");
            return;
        }
        let (width, height) = self.present_now(now, true);

        self.last_frame_key = frame_key(frame);
        self.dirty = false;
        self.last_render_at = started;
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

    /// Rebuilds the trail stage at the current anchor and presents it.
    ///
    /// `advance_history` allows a new afterimage snapshot to be spawned; the
    /// cached 60 Hz presentations during a walk pass `false` so the sampling
    /// cadence stays tied to the 25 fps animation.
    fn present_now(&mut self, now: Instant, advance_history: bool) -> (u32, u32) {
        let walking = self.state.mode == Mode::Walk;
        let lifetime = Duration::from_millis(self.config.trail_lifetime_ms as u64);
        if !self.config.trail_enabled {
            self.trail_samples.clear();
            self.trail_extents = None;
        }
        // Retire afterimages whose lifetime is over (also runs during the
        // fade-out after the pet stopped walking).
        while self
            .trail_samples
            .front()
            .is_some_and(|sample| now.saturating_duration_since(sample.at) > lifetime)
        {
            self.trail_samples.pop_front();
        }
        if walking {
            // Remembered for the fade-out that continues after the walk stops.
            // Trail geometry is in physical pixels, so the logical walk speed
            // is converted with the current display's backing scale.
            self.trail_stage_pad = trail::pad_px(
                self.state.walk_speed() * self.state.dpi,
                self.config.trail_lifetime_ms,
                2.0,
            );
        }
        let trail_on = self.config.trail_enabled && (walking || !self.trail_samples.is_empty());
        self.update_trail_extents();
        self.apply_window_geometry();

        let (full_w, full_h) = self.renderer.size();
        let dpi_f = self.state.dpi.max(0.01);
        let tight = self.state.window_rect();
        // The stage is integer-aligned in logical points.  The pet and every
        // ghost are then placed relative to the same physical origin, so the
        // window cannot land on a different pixel grid than the contents.
        let trail_stage = trail_on.then(|| self.stage_rect());
        let (desired, stage_w, stage_h, visible, current_left, current_top) =
            if let Some(stage) = trail_stage {
                let desired = Rect::new(
                    stage.x * dpi_f,
                    stage.y * dpi_f,
                    (stage.w * dpi_f).round().max(1.0),
                    (stage.h * dpi_f).round().max(1.0),
                );
                let visible_logical = stage.intersect(self.active_display_bounds());
                let visible = Rect::new(
                    visible_logical.x * dpi_f,
                    visible_logical.y * dpi_f,
                    (visible_logical.w * dpi_f).round().max(1.0),
                    (visible_logical.h * dpi_f).round().max(1.0),
                );
                let left = ((tight.x - stage.x) * dpi_f).round() as i32;
                let top = ((tight.y - stage.y) * dpi_f).round() as i32;
                (
                    desired,
                    desired.w as u32,
                    desired.h as u32,
                    visible,
                    left,
                    top,
                )
            } else {
                let desired = Rect::new(
                    tight.x * dpi_f,
                    tight.y * dpi_f,
                    full_w.max(1) as f32,
                    full_h.max(1) as f32,
                );
                let visible = desired.intersect(self.active_display_bounds_px());
                (desired, desired.w as u32, desired.h as u32, visible, 0, 0)
            };
        let ox = ((visible.x - desired.x).round().max(0.0) as u32).min(stage_w.saturating_sub(1));
        let oy = ((visible.y - desired.y).round().max(0.0) as u32).min(stage_h.saturating_sub(1));
        let crop_w = (visible.w.round() as u32).min(stage_w - ox).max(1);
        let crop_h = (visible.h.round() as u32).min(stage_h - oy).max(1);
        let dpi = dpi_f as f64;
        let anchor = self.state.anchor;
        let lifetime_secs = self.config.trail_lifetime_ms as f32 / 1000.0;
        let opacity = self.config.trail_opacity;
        {
            // Split borrows: the renderer/history are read, the stage and crop
            // buffers are written, and only then is the presenter called.
            let Self {
                renderer,
                presenter,
                crop_buf,
                trail_buf,
                trail_samples,
                ..
            } = self;
            let full = renderer.rgba8();
            if trail_on {
                let mut ghosts = Vec::with_capacity(trail_samples.len());
                for sample in trail_samples.iter() {
                    let age = now.saturating_duration_since(sample.at).as_secs_f32();
                    let alpha = trail::alpha_at(age, lifetime_secs, opacity);
                    if alpha <= 0.002 {
                        continue;
                    }
                    // Pin each ghost by the anchor inside *its own* frame.  Walk
                    // directions have different glow margins/sizes; assuming the
                    // current frame's size dropped the whole trail on every
                    // direction change.
                    let ghost = ghost_frame_origin(sample.anchor, sample.anchor_in_frame, dpi_f);
                    ghosts.push(GhostFrame {
                        pixels: &sample.pixels,
                        width: sample.width,
                        height: sample.height,
                        left: (ghost.x - desired.x).round() as i32,
                        top: (ghost.y - desired.y).round() as i32,
                        alpha,
                    });
                }
                // `compose_trail` draws in order, so the newest snapshot ends
                // up over the older (fainter) ones.
                trail_buf.resize(stage_w as usize * stage_h as usize * 4, 0);
                compose_trail(
                    trail_buf,
                    stage_w,
                    stage_h,
                    full,
                    full_w,
                    full_h,
                    current_left,
                    current_top,
                    &ghosts,
                );
                Self::present_cropped(
                    presenter, crop_buf, trail_buf, stage_w, stage_h, ox, oy, crop_w, crop_h, dpi,
                );
            } else {
                Self::present_cropped(
                    presenter, crop_buf, full, full_w, full_h, ox, oy, crop_w, crop_h, dpi,
                );
            }
        }

        // Spawn a new afterimage at the configured interval while walking.
        if advance_history && self.config.trail_enabled && walking {
            let interval = Duration::from_millis(self.config.trail_interval_ms as u64);
            let due = self.trail_samples.back().map_or(true, |sample| {
                now.saturating_duration_since(sample.at) >= interval
            });
            let moved = self
                .trail_samples
                .back()
                .map_or(true, |sample| sample.anchor.distance(anchor) >= 0.5);
            if due && moved {
                let mut pixels = self.renderer.rgba8().to_vec();
                // The ground shadow is not part of the character; strip it so
                // the trail does not leave a second shadow smear.
                let anchor_in_frame = ((anchor.x - tight.x) * dpi_f, (anchor.y - tight.y) * dpi_f);
                strip_ground_shadow(&mut pixels, full_w, full_h, anchor_in_frame);
                self.trail_samples.push_back(TrailSample {
                    pixels,
                    width: full_w,
                    height: full_h,
                    anchor,
                    anchor_in_frame,
                    at: now,
                });
                while self.trail_samples.len() > TRAIL_MAX_SAMPLES {
                    self.trail_samples.pop_front();
                }
            }
        }

        (crop_w, crop_h)
    }

    /// Recomposites the cached frame at the current anchor between animation
    /// frames.  Without this, the window moves at ~60 Hz while the afterimages
    /// are only re-laid-out at 25 fps, which makes a long trail shiver.
    fn present_cached(&mut self, now: Instant) {
        let started = Instant::now();
        self.present_now(now, false);
        self.cached_present_count += 1;
        self.cached_present_sum_ms += started.elapsed().as_secs_f64() * 1000.0;
        if self.cached_present_count % 120 == 0 {
            tracing::debug!(
                avg_ms = self.cached_present_sum_ms / 120.0,
                "cached trail present stats"
            );
            self.cached_present_sum_ms = 0.0;
        }
    }

    /// Copies a (possibly clipped) part of `src` into the presenter.
    fn present_cropped(
        presenter: &mut Option<Box<dyn Presenter>>,
        crop_buf: &mut Vec<u8>,
        src: &[u8],
        src_w: u32,
        src_h: u32,
        ox: u32,
        oy: u32,
        crop_w: u32,
        crop_h: u32,
        dpi: f64,
    ) {
        let Some(presenter) = presenter.as_mut() else {
            return;
        };
        if ox == 0 && oy == 0 && crop_w == src_w && crop_h == src_h {
            if let Err(err) = presenter.present(src, src_w, src_h, dpi) {
                tracing::warn!(%err, "present failed");
            }
        } else {
            let (w, h) = Self::crop_into(src, src_w, ox, oy, crop_w, crop_h, crop_buf);
            if w == 0 || h == 0 {
                return;
            }
            if let Err(err) = presenter.present(crop_buf, w, h, dpi) {
                tracing::warn!(%err, "present failed");
            }
        }
    }

    /// Are there afterimages still fading out after the walk stopped?
    fn trail_fading(&self) -> bool {
        self.config.trail_enabled && self.state.mode != Mode::Walk && !self.trail_samples.is_empty()
    }

    /// Transparent margin (physical px) the stage needs for afterimages.
    fn trail_pad_px(&self) -> u32 {
        if !self.config.trail_enabled {
            return 0;
        }
        if self.state.mode == Mode::Walk {
            trail::pad_px(
                self.state.walk_speed() * self.state.dpi,
                self.config.trail_lifetime_ms,
                2.0,
            )
        } else if self.trail_samples.is_empty() {
            0
        } else {
            // Keep the walking stage size while the last ghosts fade out.
            self.trail_stage_pad
        }
    }

    /// Current frame + trail margin in logical points (no afterimages).
    fn current_stage_base(&self) -> Rect {
        let dpi = self.state.dpi.max(0.01);
        let pad = self.trail_pad_px() as f32 / dpi;
        self.state.window_rect().outset(pad)
    }

    /// Largest frame extent around the feet over every walk direction, plus
    /// the glow safety margin and the trail length, in logical points.
    ///
    /// This does not depend on the current direction, so the stage stays the
    /// same size for the whole walk.
    fn walk_stage_margins(&self) -> (f32, f32, f32, f32) {
        let scale = self.state.scale;
        let (mut left, mut top, mut right, mut bottom) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut max_k = 0.05f32;
        for dir in Direction::ALL {
            let Ok(spec) = self.catalog.walk_spec(self.state.form, dir) else {
                continue;
            };
            let k = spec.placement.scale.max(0.05);
            max_k = max_k.max(k);
            left = left.max(spec.pet_anchor.0 * k * scale);
            right = right.max((spec.canvas.0 * k - spec.pet_anchor.0 * k) * scale);
            top = top.max(spec.pet_anchor.1 * k * scale);
            bottom = bottom.max((spec.canvas.1 * k - spec.pet_anchor.1 * k) * scale);
        }
        // Constant margin: walk directions carry different glow layers, and
        // deriving it from the current one would resize the stage on every turn.
        let glow = WALK_GLOW_MARGIN_UNITS * max_k * scale;
        let pad = self.trail_pad_px() as f32 / self.state.dpi.max(0.01);
        (
            left + glow + pad,
            top + glow + pad,
            right + glow + pad,
            bottom + glow + pad,
        )
    }

    /// Anchor-relative extents of the fixed walking stage (logical points).
    fn fixed_walk_extents(&self) -> Rect {
        let (left, top, right, bottom) = self.walk_stage_margins();
        let left = left.ceil();
        let top = top.ceil();
        let right = right.ceil();
        let bottom = bottom.ceil();
        Rect::new(-left, -top, left + right, top + bottom)
    }

    /// Keeps the fixed walking stage while the trail lives.
    ///
    /// It is computed once per present from the walk catalog and the current
    /// trail settings, not from individual ghost frames.  That makes it
    /// independent of direction changes and of ghosts expiring.  During the
    /// short fade-out after an action starts, the current action frame is
    /// unioned in so a big action is never clipped.
    fn update_trail_extents(&mut self) {
        let walking = self.state.mode == Mode::Walk;
        if !self.config.trail_enabled || (!walking && self.trail_samples.is_empty()) {
            self.trail_extents = None;
            return;
        }
        let mut extents = self.fixed_walk_extents();
        if !walking {
            let anchor = self.state.anchor;
            let base = self.current_stage_base();
            let frame = Rect::new(base.x - anchor.x, base.y - anchor.y, base.w, base.h);
            extents = union_rect(extents, frame);
        }
        let x = extents.x.floor();
        let y = extents.y.floor();
        let right = extents.right().ceil();
        let bottom = extents.bottom().ceil();
        self.trail_extents = Some(Rect::new(x, y, (right - x).max(1.0), (bottom - y).max(1.0)));
    }

    /// Stage rect in **logical points, aligned to whole points** so the window
    /// origin and all layer offsets share one integer grid.
    ///
    /// The stage stays at a fixed size, relative to the feet, for the whole
    /// walk; only its position follows the pet.
    fn stage_rect(&self) -> Rect {
        if let Some(extents) = self.trail_extents {
            return Rect::new(
                (self.state.anchor.x + extents.x).round(),
                (self.state.anchor.y + extents.y).round(),
                extents.w.max(1.0),
                extents.h.max(1.0),
            );
        }
        let base = self.current_stage_base();
        Rect::new(
            base.x.round(),
            base.y.round(),
            base.w.ceil().max(1.0),
            base.h.ceil().max(1.0),
        )
    }

    /// Physical pixels of the usable area of the display the pet is on.
    fn active_display_bounds_px(&self) -> Rect {
        let bounds = self.active_display_bounds();
        let dpi = self.state.dpi.max(0.01);
        Rect::new(
            bounds.x * dpi,
            bounds.y * dpi,
            bounds.w * dpi,
            bounds.h * dpi,
        )
    }

    /// The union of the enabled displays' usable areas.
    ///
    /// The union (not just the display under the anchor) is used for window
    /// clipping so a pet straddling a shared seam is not cut in half for one
    /// frame while crossing.
    fn active_display_bounds(&self) -> Rect {
        let mut selected = self.selected_display_refs();
        let Some(first) = selected.next() else {
            return self
                .roam
                .union()
                .unwrap_or_else(|| fallback_display().bounds);
        };
        let mut rect = first.bounds;
        for display in selected {
            let bounds = display.bounds;
            let x0 = rect.x.min(bounds.x);
            let y0 = rect.y.min(bounds.y);
            let x1 = rect.right().max(bounds.right());
            let y1 = rect.bottom().max(bounds.bottom());
            rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
        }
        rect
    }

    /// The selected display containing the anchor, or the nearest selected one.
    fn display_for_anchor(&self) -> Option<&Display> {
        let anchor = self.state.anchor;
        self.selected_display_refs()
            .find(|display| display.bounds.contains(anchor))
            .or_else(|| {
                self.selected_display_refs().min_by(|a, b| {
                    distance_to_rect(anchor, a.bounds)
                        .partial_cmp(&distance_to_rect(anchor, b.bounds))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
            .or_else(|| self.primary_display())
            .or_else(|| self.displays.first())
    }

    fn selected_display_refs(&self) -> impl Iterator<Item = &Display> {
        self.displays
            .iter()
            .filter(|display| self.selected_displays.contains(&display.id))
    }

    fn primary_display(&self) -> Option<&Display> {
        self.displays.iter().find(|display| display.primary)
    }

    /// Where a summon places the pet: the primary display when enabled,
    /// otherwise the first enabled one.
    fn summon_display(&self) -> Option<&Display> {
        self.primary_display()
            .filter(|display| self.selected_displays.contains(&display.id))
            .or_else(|| self.selected_display_refs().next())
            .or_else(|| self.primary_display())
            .or_else(|| self.displays.first())
    }

    /// Applies the current stage rect to the window and the presenter layer.
    fn apply_window_geometry(&mut self) {
        let rect = self.visible_rect();
        // Whole logical points keep the window and every layer on one integer
        // grid (AppKit rounds window frames to points anyway).
        let logical_w = rect.w.round().max(1.0);
        let logical_h = rect.h.round().max(1.0);
        let pos_x = rect.x.round();
        let pos_y = rect.y.round();
        let dpi = self.state.dpi.max(0.01);
        let want_w = (logical_w * dpi).round().max(1.0) as u32;
        let want_h = (logical_h * dpi).round().max(1.0) as u32;
        if self.last_window_size != Some((want_w, want_h)) {
            if let Some(window) = &self.window {
                if window.inner_size() != PhysicalSize::new(want_w, want_h) {
                    let _ = window
                        .request_inner_size(LogicalSize::new(logical_w as f64, logical_h as f64));
                }
            }
            if let Some(presenter) = self.presenter.as_mut() {
                if let Err(err) = presenter.resize(want_w, want_h, dpi as f64) {
                    tracing::warn!(%err, "presenter resize failed");
                }
            }
            self.last_window_size = Some((want_w, want_h));
        }
        if let Some(window) = &self.window {
            // Compare in logical points: `outer_position` is physical and the
            // window scale can be mid-transition while crossing displays.
            let current = window.outer_position().ok().map(|position| {
                let scale = window.scale_factor() as f32;
                (position.x as f32 / scale, position.y as f32 / scale)
            });
            let on_target =
                current.is_some_and(|(x, y)| (x - pos_x).abs() < 0.01 && (y - pos_y).abs() < 0.01);
            if !on_target {
                window.set_outer_position(LogicalPosition::new(pos_x as f64, pos_y as f64));
            }
            // A trail-sized transparent window would swallow clicks far away
            // from the pet, so the stage stays click-through except when the
            // cursor is actually over the pet's tight rect.  That keeps the
            // pet clickable/draggable while walking with a trail.
            let trail_stage = self.trail_pad_px() > 0;
            let interactive = if self.drag.is_some() {
                true
            } else if self.state.mode == Mode::Action && self.state.anim.auto_end {
                false
            } else if self.renderer.body_hit().is_some() {
                // Only the body is clickable; the rest of the stage is
                // click-through, so the desktop stays usable around the pet.
                self.cursor_over_body()
            } else if trail_stage {
                self.cursor_over_pet()
            } else {
                true
            };
            if self.last_interactive != Some(interactive) {
                if let Err(err) = window.set_cursor_hittest(interactive) {
                    tracing::debug!(%err, "set_cursor_hittest failed");
                }
                self.last_interactive = Some(interactive);
            }
        }
    }

    /// Whether the global cursor is over the pet's tight animation rect.
    ///
    /// Only used while the transparent trail stage is active; outside it the
    /// window's normal click-through rules apply.
    fn cursor_over_pet(&self) -> bool {
        let Some(cursor) = nono_platform::cursor_position() else {
            return false;
        };
        self.state.window_rect().contains(cursor)
    }

    /// Whether the global cursor is inside the pet's round body.
    ///
    /// The click/drag area is the body only; ears, fins, props and the
    /// transparent corners stay click-through.  Uses the tinted body bounds
    /// reported by the renderer for the current frame.
    fn cursor_over_body(&self) -> bool {
        let Some(cursor) = nono_platform::cursor_position() else {
            return false;
        };
        let Some(hit) = self.renderer.body_hit() else {
            return self.cursor_over_pet();
        };
        body_hit_contains(self.state.anchor, hit, self.state.scale, cursor)
    }

    /// Global cursor in logical points.
    ///
    /// Falls back to deriving it from the window-local cursor when the platform
    /// cannot report the global position (the trail window is click-through
    /// until the cursor reaches the pet, so the local value can be stale).
    fn global_cursor(&self) -> Point {
        nono_platform::cursor_position().unwrap_or_else(|| {
            let rect = self.visible_rect();
            let dpi = self.state.dpi.max(0.01);
            Point::new(
                rect.x + self.cursor.x as f32 / dpi,
                rect.y + self.cursor.y as f32 / dpi,
            )
        })
    }

    fn position_window(&self) {
        let Some(window) = &self.window else { return };
        // With the trail stage, the window must only move together with a
        // recomposited trail; moving it independently makes ghosts shiver.
        if self.trail_pad_px() > 0 {
            return;
        }
        let rect = self.visible_rect();
        window.set_outer_position(LogicalPosition::new(
            rect.x.round() as f64,
            rect.y.round() as f64,
        ));
    }

    /// Visible part of the stage in **logical global points**.
    fn visible_rect(&self) -> Rect {
        self.stage_rect().intersect(self.active_display_bounds())
    }

    /// Largest extent of the pet's *silhouette* around its anchor, over all
    /// eight walk directions (left, top, right, bottom, in canvas units).
    fn pet_margins(&self) -> (f32, f32, f32, f32) {
        let (mut left, mut top, mut right, mut bottom) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for dir in Direction::ALL {
            let Ok(spec) = self.catalog.walk_spec(self.state.form, dir) else {
                continue;
            };
            let Some(bb) = spec.content_bbox else {
                continue;
            };
            left = left.max(spec.pet_anchor.0 - bb.0);
            top = top.max(spec.pet_anchor.1 - bb.1);
            right = right.max(bb.2 - spec.pet_anchor.0);
            bottom = bottom.max(bb.3 - spec.pet_anchor.1);
        }
        (left, top, right, bottom)
    }

    /// Roaming rects: every selected display minus the pet's silhouette
    /// margins, so the character never gets cut off at an outer edge.
    ///
    /// Edges shared with another enabled display get **no** margin; otherwise
    /// the two inset rectangles would leave a dead gap at the seam and the pet
    /// could not walk (or be dragged) from one screen to the other.
    fn compute_roam_area(&self) -> RoamArea {
        compute_roam_rects(
            &self.displays,
            &self.selected_displays,
            self.pet_margins(),
            self.state.scale,
        )
    }

    /// Copies a sub-rectangle of an RGBA frame (used when the window had to be
    /// clipped at a screen edge).
    /// Crops `src` into `out`, returning the size actually written.
    fn crop_into(
        src: &[u8],
        src_w: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        out: &mut Vec<u8>,
    ) -> (u32, u32) {
        out.clear();
        if src_w == 0 || w == 0 || h == 0 {
            return (0, 0);
        }
        // Clamp defensively: geometry changes between sizing and presentation
        // must never read past the frame.
        let x = x.min(src_w - 1);
        let w = w.min(src_w - x);
        let src_h = (src.len() / 4 / src_w as usize) as u32;
        let y = y.min(src_h.saturating_sub(1));
        let h = h.min(src_h.saturating_sub(y));
        if h == 0 {
            return (0, 0);
        }
        out.reserve((w as usize) * (h as usize) * 4);
        for row in y..y + h {
            let start = ((row as usize) * (src_w as usize) + (x as usize)) * 4;
            out.extend_from_slice(&src[start..start + (w as usize) * 4]);
        }
        (w, h)
    }

    /// Reloads the renderer at the current scale while keeping the animation
    /// phase (used for real scale-factor changes and menu zoom changes).
    fn reload_preserving_frame(&mut self, now: Instant) {
        self.roam = self.compute_roam_area();
        self.state.clamp_to(&self.roam);
        // History frames have the old pixel size and would be misaligned.
        self.trail_samples.clear();
        self.trail_stage_pad = 0;
        self.trail_extents = None;
        self.last_window_size = None;
        let scale = self.state.scale * self.state.dpi;
        let load_started = Instant::now();
        if let Err(err) = self.renderer.load(&self.state.anim, scale) {
            tracing::warn!(%err, "reloading the animation failed");
        } else {
            // Keep the current phase across the load instead of letting the
            // clock advance while the Lottie scene is being parsed.
            self.state.compensate_pause(load_started.elapsed());
        }
        self.apply_window_geometry();
        self.last_anim_id = self.state.anim.id.clone();
        self.last_frame_key = i64::MIN;
        self.dirty = true;
        self.render_now(now);
    }

    fn frame_due(&self, now: Instant) -> bool {
        let key = frame_key(self.state.frame_at(now, self.walk_animation_speed()));
        key != self.last_frame_key || self.dirty
    }

    // ── interactions ────────────────────────────────────────────────

    fn on_click(&mut self, now: Instant) {
        self.bus.publish(Event::UserClick {
            button: "left".into(),
        });
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
        self.bus.publish(Event::UserClick {
            button: "right".into(),
        });
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
            ["summon"] => self.summon(now, "play-scripted"),
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
        self.bus.publish(Event::ChangeState {
            state: "Follow".into(),
        });
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
        self.bus.publish(Event::ChangeState {
            state: self.state.mode.as_str().into(),
        });
    }

    /// One follow step: re-target the cursor every ~150 ms so the pet keeps
    /// walking towards it (and faces it when close enough).
    fn follow_step(&mut self, now: Instant) {
        let Some(cursor) = nono_platform::cursor_position() else {
            return;
        };
        let target = self.roam.clamp(cursor);
        let distance = target.distance(self.state.anchor);
        let close_enough = 60.0;
        if distance > close_enough {
            let speed = self.config.effective_follow_speed();
            self.state.start_walk(&self.catalog, target, speed, now);
        } else {
            if self.state.mode == Mode::Walk {
                self.state.idle(&self.catalog, now);
                // Drop the trail stage even when idle animation is turned off.
                self.dirty = true;
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
        if self.config.tint == tint {
            // The clicked check item toggled itself; restore single-selection.
            if let Some(tray) = &self.tray {
                tray.sync_checks(&self.config);
            }
            return;
        }
        self.config.tint = tint;
        self.renderer.set_tint(tint);
        // Old colour caches are unused now; drop them instead of accumulating
        // 10+ MB per animation colour on disk.
        self.renderer.clear_tint_cache();
        // The tint is baked into the Lottie layers at load time, so reload the
        // current animation (keeping its playback position).
        self.reload_preserving_frame(now);
        // Rebuild the other animations' tinted copies in the background, so the
        // next first play does not pay for it.
        if tint.is_some() {
            spawn_glow_warmer(
                config::cache_dir(),
                self.assets_root.clone(),
                tint,
                self.config.form,
            );
        }
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
        } else {
            // Our own write should not look like an external edit to the
            // hot-reload watcher.
            self.config_mtime = Self::config_mtime(&self.config_path);
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
                    tray.hide.set_text(if self.visible {
                        "隐藏 NONO"
                    } else {
                        "显示 NONO"
                    });
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
            MENU_SUMMON => self.summon(now, "menu-summon"),
            MENU_SUMMON_ANIMATION => {
                self.config.summon_animation = !self.config.summon_animation;
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                self.save_config();
                tracing::info!(enabled = self.config.summon_animation, "summon animation");
            }
            MENU_START_AT_CENTER => {
                self.config.start_at_center = !self.config.start_at_center;
                // Remember where the pet is right now, so switching this off
                // has a meaningful saved position for the next launch.
                self.save_config();
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                tracing::info!(
                    start_at_center = self.config.start_at_center,
                    "startup position"
                );
            }
            MENU_DISPLAY_REFRESH => self.rescan_displays(event_loop, now, true),
            MENU_TRAIL_ENABLED => self.set_trail_enabled(!self.config.trail_enabled, now),
            MENU_TRAIL_PREVIEW => self.preview_trail(now),
            MENU_SPEED_MOVE_DOWN => self.nudge_move_speed(-speed::STEP, now),
            MENU_SPEED_MOVE_UP => self.nudge_move_speed(speed::STEP, now),
            MENU_SPEED_FOLLOW_DOWN => self.nudge_follow_speed(-speed::STEP, now),
            MENU_SPEED_FOLLOW_UP => self.nudge_follow_speed(speed::STEP, now),
            MENU_SPEED_IDLE_DOWN => self.nudge_idle_speed(-speed::STEP, now),
            MENU_SPEED_IDLE_UP => self.nudge_idle_speed(speed::STEP, now),
            MENU_SPEED_FOLLOW_GLOBAL => self.toggle_follow_global_speed(now),
            MENU_SPEED_IDLE_GLOBAL => self.toggle_idle_global_speed(now),
            MENU_SPEED_RESET => self.reset_speeds(now),
            MENU_TRAIL_INTERVAL_DOWN => {
                self.nudge_trail_interval(-(trail::INTERVAL_STEP_MS as i32), now)
            }
            MENU_TRAIL_INTERVAL_UP => {
                self.nudge_trail_interval(trail::INTERVAL_STEP_MS as i32, now)
            }
            MENU_TRAIL_LIFETIME_DOWN => {
                self.nudge_trail_lifetime(-(trail::LIFETIME_STEP_MS as i32), now)
            }
            MENU_TRAIL_LIFETIME_UP => {
                self.nudge_trail_lifetime(trail::LIFETIME_STEP_MS as i32, now)
            }
            MENU_TRAIL_OPACITY_DOWN => self.nudge_trail_opacity(-trail::OPACITY_STEP, now),
            MENU_TRAIL_OPACITY_UP => self.nudge_trail_opacity(trail::OPACITY_STEP, now),
            MENU_TRAIL_RESET => self.reset_trail(now),
            MENU_TRAIL_RELOAD => {
                self.reload_tuning_from_disk(now);
            }
            MENU_TRAIL_OPEN_CONFIG => self.open_config_file(now),
            MENU_TRAIL_WATCH => self.toggle_trail_watch(now),
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
                } else if let Some(display) = id.strip_prefix(MENU_DISPLAY_PREFIX) {
                    if let Ok(display) = display.parse::<u32>() {
                        self.toggle_display(display, now);
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
        // A different form has different walk canvases; old snapshots would not
        // line up and are simply dropped.
        self.trail_samples.clear();
        self.trail_stage_pad = 0;
        self.trail_extents = None;
        self.roam = self.compute_roam_area();
        self.state.clamp_to(&self.roam);
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
        self.bus.publish(Event::SetScale {
            scale: self.config.scale,
        });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
    }

    /// Walks a short distance so the current trail settings can be judged
    /// without waiting for the next wander tick.
    fn preview_trail(&mut self, now: Instant) {
        if self.follow_until.is_some() {
            self.stop_follow(now);
        }
        let rect = self
            .roam
            .rect_at(self.state.anchor)
            .or_else(|| self.roam.nearest_rect(self.state.anchor))
            .unwrap_or_else(|| self.active_display_bounds());
        let distance = 180.0;
        let dir = if self.state.anchor.x + distance + 4.0 <= rect.right() {
            1.0
        } else {
            -1.0
        };
        let target =
            Point::new(self.state.anchor.x + distance * dir, self.state.anchor.y).clamp(rect);
        if target.distance(self.state.anchor) < 24.0 {
            tracing::debug!("no room to preview the trail");
            return;
        }
        self.state.start_walk(
            &self.catalog,
            target,
            self.config.effective_idle_speed(),
            now,
        );
        self.sync_animation(now, false, "trail-preview");
        self.render_now(now);
    }

    /// Nudges the global movement speed (logical pt/s).
    fn nudge_move_speed(&mut self, delta: f32, now: Instant) {
        let next = speed::sanitize(self.config.move_speed + delta);
        if (next - self.config.move_speed).abs() < 1e-4 {
            return;
        }
        self.config.move_speed = next;
        self.finish_speed_change(now, true);
    }

    /// Nudges the follow speed, creating an override if it was using global.
    fn nudge_follow_speed(&mut self, delta: f32, now: Instant) {
        let next = speed::sanitize(self.config.effective_follow_speed() + delta);
        if (next - self.config.effective_follow_speed()).abs() < 1e-4 {
            return;
        }
        self.config.follow_speed = Some(next);
        self.finish_speed_change(now, true);
    }

    /// Nudges the idle/free-roam speed, creating an override if needed.
    fn nudge_idle_speed(&mut self, delta: f32, now: Instant) {
        let next = speed::sanitize(self.config.effective_idle_speed() + delta);
        if (next - self.config.effective_idle_speed()).abs() < 1e-4 {
            return;
        }
        self.config.idle_speed = Some(next);
        self.finish_speed_change(now, true);
    }

    /// Toggles "follow uses the global speed"; unchecking starts an override at
    /// the current effective value so the pet does not jump.
    fn toggle_follow_global_speed(&mut self, now: Instant) {
        self.config.follow_speed = if self.config.follow_speed.is_none() {
            Some(self.config.effective_follow_speed())
        } else {
            None
        };
        self.finish_speed_change(now, true);
    }

    fn toggle_idle_global_speed(&mut self, now: Instant) {
        self.config.idle_speed = if self.config.idle_speed.is_none() {
            Some(self.config.effective_idle_speed())
        } else {
            None
        };
        self.finish_speed_change(now, true);
    }

    fn reset_speeds(&mut self, now: Instant) {
        self.config.move_speed = speed::DEFAULT_GLOBAL;
        self.config.follow_speed = speed::DEFAULT_FOLLOW;
        self.config.idle_speed = speed::DEFAULT_IDLE;
        self.finish_speed_change(now, true);
    }

    /// Sanitises, applies and optionally persists the three speed values.
    fn finish_speed_change(&mut self, _now: Instant, persist: bool) {
        self.config.move_speed = speed::sanitize(self.config.move_speed);
        self.config.follow_speed = speed::sanitize_override(self.config.follow_speed);
        self.config.idle_speed = speed::sanitize_override(self.config.idle_speed);
        self.behavior
            .set_wander_speed(self.config.effective_idle_speed());
        if self.state.mode == Mode::Walk {
            let speed = if self.follow_until.is_some() {
                self.config.effective_follow_speed()
            } else {
                self.config.effective_idle_speed()
            };
            self.state.set_walk_speed(speed);
        }
        if persist {
            self.save_config();
        }
        self.bus.publish(Event::SetSpeed {
            global: self.config.move_speed,
            follow: self.config.follow_speed,
            idle: self.config.idle_speed,
        });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
        tracing::debug!(
            global = self.config.move_speed,
            follow = ?self.config.follow_speed,
            idle = ?self.config.idle_speed,
            "movement speed changed"
        );
    }

    /// Turns the walking afterimage on or off.
    fn set_trail_enabled(&mut self, enabled: bool, now: Instant) {
        if self.config.trail_enabled == enabled {
            return;
        }
        self.config.trail_enabled = enabled;
        if !enabled {
            self.trail_samples.clear();
            self.trail_stage_pad = 0;
            self.trail_extents = None;
        }
        self.finish_trail_change(now, true);
    }

    /// Nudges the spawn interval by `delta_ms` (negative = denser).
    fn nudge_trail_interval(&mut self, delta_ms: i32, now: Instant) {
        let next = (self.config.trail_interval_ms as i64 + delta_ms as i64)
            .clamp(trail::MIN_INTERVAL_MS as i64, trail::MAX_INTERVAL_MS as i64)
            as u32;
        if next == self.config.trail_interval_ms {
            return;
        }
        self.config.trail_interval_ms = next;
        self.finish_trail_change(now, true);
    }

    /// Nudges the lifetime by `delta_ms` (negative = shorter).
    fn nudge_trail_lifetime(&mut self, delta_ms: i32, now: Instant) {
        let next = (self.config.trail_lifetime_ms as i64 + delta_ms as i64)
            .clamp(trail::MIN_LIFETIME_MS as i64, trail::MAX_LIFETIME_MS as i64)
            as u32;
        if next == self.config.trail_lifetime_ms {
            return;
        }
        self.config.trail_lifetime_ms = next;
        self.finish_trail_change(now, true);
    }

    /// Nudges the peak opacity (negative = fainter).
    fn nudge_trail_opacity(&mut self, delta: f32, now: Instant) {
        let next = trail::sanitize_opacity(self.config.trail_opacity + delta);
        if (next - self.config.trail_opacity).abs() < 1e-4 {
            return;
        }
        self.config.trail_opacity = next;
        self.finish_trail_change(now, true);
    }

    /// Restores the built-in default trail (enabled, 120/640 ms, 22%).
    fn reset_trail(&mut self, now: Instant) {
        self.config.trail_enabled = trail::DEFAULT_ENABLED;
        self.config.trail_interval_ms = trail::DEFAULT_INTERVAL_MS;
        self.config.trail_lifetime_ms = trail::DEFAULT_LIFETIME_MS;
        self.config.trail_opacity = trail::DEFAULT_OPACITY;
        self.finish_trail_change(now, true);
    }

    /// Re-sanitises the values, applies the new stage size immediately and
    /// (when `persist`) writes them to the config file.
    fn finish_trail_change(&mut self, now: Instant, persist: bool) {
        let (interval, lifetime) =
            trail::sanitize(self.config.trail_interval_ms, self.config.trail_lifetime_ms);
        self.config.trail_interval_ms = interval;
        self.config.trail_lifetime_ms = lifetime;
        self.config.trail_opacity =
            (trail::sanitize_opacity(self.config.trail_opacity) * 100.0).round() / 100.0;
        if !self.config.trail_enabled {
            self.trail_samples.clear();
            self.trail_stage_pad = 0;
            self.trail_extents = None;
        }
        if self.state.mode == Mode::Walk {
            self.trail_stage_pad = trail::pad_px(
                self.state.walk_speed() * self.state.dpi,
                self.config.trail_lifetime_ms,
                2.0,
            );
        }
        self.dirty = true;
        self.render_now(now);
        if persist {
            self.save_config();
        }
        self.bus.publish(Event::SetTrail {
            enabled: self.config.trail_enabled,
            interval_ms: self.config.trail_interval_ms,
            lifetime_ms: self.config.trail_lifetime_ms,
            opacity: self.config.trail_opacity,
        });
        if let Some(tray) = &self.tray {
            tray.sync_checks(&self.config);
        }
        tracing::debug!(
            enabled = self.config.trail_enabled,
            interval_ms = self.config.trail_interval_ms,
            lifetime_ms = self.config.trail_lifetime_ms,
            opacity = self.config.trail_opacity,
            "trail changed"
        );
    }

    /// Reads the latest trail fields from `config.toml` without touching the
    /// other settings.
    /// Reads the latest trail and speed fields from `config.toml` without
    /// touching the other settings.
    fn reload_tuning_from_disk(&mut self, now: Instant) -> bool {
        match Config::load_strict(&self.config_path) {
            Ok(loaded) => {
                self.config.trail_enabled = loaded.trail_enabled;
                self.config.trail_interval_ms = loaded.trail_interval_ms;
                self.config.trail_lifetime_ms = loaded.trail_lifetime_ms;
                self.config.trail_opacity = loaded.trail_opacity;
                self.config.move_speed = loaded.move_speed;
                self.config.follow_speed = loaded.follow_speed;
                self.config.idle_speed = loaded.idle_speed;
                self.config.form1_walk_anim_speed = loaded.form1_walk_anim_speed;
                let glow_changed = (
                    self.config.glow,
                    self.config.glow_sigma,
                    self.config.glow_alpha,
                ) != (loaded.glow, loaded.glow_sigma, loaded.glow_alpha);
                self.config.glow = loaded.glow;
                self.config.glow_sigma = loaded.glow_sigma;
                self.config.glow_alpha = loaded.glow_alpha;
                self.config.summon_animation = loaded.summon_animation;
                self.config.start_at_center = loaded.start_at_center;
                self.config_mtime = Self::config_mtime(&self.config_path);
                if let Some(tray) = &self.tray {
                    tray.sync_checks(&self.config);
                }
                self.finish_trail_change(now, false);
                self.finish_speed_change(now, false);
                if glow_changed {
                    // Blur radius/opacity are baked into the glow passes when an
                    // animation loads, so pick them up by reloading the clip.
                    self.renderer.set_glow(glow_settings(&self.config));
                    self.reload_preserving_frame(now);
                }
                true
            }
            Err(err) => {
                tracing::warn!(%err, path = %self.config_path.display(), "tuning reload failed, keeping current values");
                false
            }
        }
    }

    /// Enables/disables polling of the config file for live tuning.
    fn toggle_trail_watch(&mut self, now: Instant) {
        self.config_watch = !self.config_watch;
        if self.config_watch {
            self.config_mtime = Self::config_mtime(&self.config_path);
            self.config_watch_next = now + CONFIG_POLL;
        }
        if let Some(tray) = &self.tray {
            tray.trail_watch.set_checked(self.config_watch);
        }
        tracing::info!(enabled = self.config_watch, "config hot reload");
    }

    /// Polls the config file and applies changed trail values.
    fn poll_config_watch(&mut self, now: Instant) {
        self.config_watch_next = now + CONFIG_POLL;
        let Some(mtime) = Self::config_mtime(&self.config_path) else {
            return;
        };
        if Some(mtime) == self.config_mtime {
            return;
        }
        self.config_mtime = Some(mtime);
        if self.reload_tuning_from_disk(now) {
            tracing::info!(
                form1_walk_anim_speed = self.config.form1_walk_anim_speed,
                form = self.state.form,
                active = self.state.form == 1
                    && self.state.anim.kind == nono_core::catalog::AnimationKind::Walk,
                "trail/speed settings reloaded from config"
            );
        }
    }

    fn config_mtime(path: &Path) -> Option<SystemTime> {
        std::fs::metadata(path).ok()?.modified().ok()
    }

    /// Opens `config.toml` in the default editor and starts the hot-reload
    /// watcher so edits show up live.
    fn open_config_file(&mut self, now: Instant) {
        self.save_config();
        let path = self.config_path.clone();
        let spawned = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&path).spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&path)
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&path).spawn()
        };
        if let Err(err) = spawned {
            tracing::warn!(%err, path = %path.display(), "could not open the config file");
        }
        if !self.config_watch {
            self.toggle_trail_watch(now);
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
        let fading = self.trail_fading();
        let hover_poll = self.renderer.body_hit().is_some() && !animating;
        let next = if self.follow_until.is_some() {
            now + Duration::from_millis(50)
        } else if animating || fading {
            now + ANIMATION_POLL
        } else if hover_poll {
            // Keep the body click area accurate while the pet is static.
            now + Duration::from_millis(100)
        } else if self.config_watch {
            now + CONFIG_POLL
        } else if self.state.mode == Mode::Action && !self.state.anim.auto_end {
            // Holding the last frame of charge/shutdown: sleep until the user
            // interacts (menu, click, drag) — all of which wake the loop.
            now + Duration::from_secs(3600)
        } else {
            self.behavior.next_wake()
        };
        // Display hot-plug is checked occasionally even while the pet sleeps.
        let next = next.min(self.display_poll_next);
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
                self.cursor = position;
                if let Some(drag) = &self.drag {
                    let cursor = self.global_cursor();
                    let dx = cursor.x - drag.press_global.x;
                    let dy = cursor.y - drag.press_global.y;
                    let distance = (dx * dx + dy * dy).sqrt();
                    let desired = Point::new(drag.press_anchor.x + dx, drag.press_anchor.y + dy);
                    if let Some(drag) = self.drag.as_mut() {
                        drag.distance = distance;
                    }
                    // Move the anchor by the cursor delta rather than the window
                    // origin: at a screen edge the window gets clipped, and the
                    // old approach would lose the pointer offset.
                    self.state.anchor = desired;
                    self.state.clamp_to(&self.roam);
                    let now = Instant::now();
                    self.sync_active_display_dpi(now);
                    self.apply_window_geometry();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    // The window may still be interactive for a frame after the
                    // cursor leaves the body; ignore presses outside it.
                    if self.renderer.body_hit().is_some() && !self.cursor_over_body() {
                        return;
                    }
                    let now = Instant::now();
                    if self.follow_until.is_some() {
                        self.stop_follow(now);
                    }
                    // A pending wander decision must not start a walk in the
                    // middle of a drag; it would use a target near the drag path
                    // and make the pet walk back there after release.
                    self.behavior.delay(now);
                    // Use the live global cursor rather than the possibly stale
                    // last CursorMoved coordinate (the trail window may have
                    // been click-through until just now).
                    self.drag = Some(DragState {
                        press_global: self.global_cursor(),
                        press_anchor: self.state.anchor,
                        distance: 0.0,
                    });
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
                            self.state.clamp_to(&self.roam);
                            // Defensive: no walk may survive the drag (e.g. one
                            // started by an old behavior decision).  Otherwise
                            // the pet would wander back towards the drag path.
                            if self.state.mode == Mode::Walk {
                                self.state.stop_walk(&self.catalog, now);
                                self.sync_animation(now, false, "drag-release");
                                self.render_now(now);
                            } else {
                                self.sync_active_display_dpi(now);
                                self.apply_window_geometry();
                            }
                            self.behavior.delay(now);
                            self.save_config();
                            self.bus.publish(Event::DragEnded);
                        }
                        None => {}
                    }
                    self.schedule(event_loop);
                }
                (MouseButton::Right, ElementState::Released) => {
                    self.on_secondary_click(Instant::now())
                }
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
        if now >= self.display_poll_next {
            self.poll_displays(event_loop, now);
        }
        // Refresh the body click-through test even when nothing animates.
        self.apply_window_geometry();

        if self.config_watch && now >= self.config_watch_next {
            self.poll_config_watch(now);
        }

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
            self.sync_active_display_dpi(now);
            self.position_window();
            if self.state.mode == Mode::Idle {
                // Start the trail fade-out and force one render even when idle
                // animation is turned off (otherwise the window would stay big).
                self.dirty = true;
            }
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
            && self.drag.is_none()
            && self
                .behavior
                .update(&mut self.state, &self.catalog, &self.roam, now)
        {
            self.sync_animation(now, false, "state-change");
        }
        self.sync_animation(now, false, "state-change");

        // The window moved on this tick.  With a trail stage, recomposite at
        // tick rate so the ghosts stay pinned to their historical positions
        // instead of riding along with the window between 25 fps frames.
        let trail_walking = self.config.trail_enabled
            && self.state.mode == Mode::Walk
            && tick.moved
            && !self.trail_samples.is_empty();
        if trail_walking && !self.dirty && !self.frame_due(now) {
            self.present_cached(now);
        }

        let idle = self.state.mode == Mode::Idle;
        let animating = self.state.is_animating() && (!idle || self.config.idle_animation);
        let fading = self.trail_fading();
        let fade_due =
            fading && now.saturating_duration_since(self.last_render_at) >= TRAIL_FADE_FRAME;
        if animating || fading || self.dirty {
            if fade_due || self.frame_due(now) {
                self.render_now(now);
            }
        }
        self.schedule(event_loop);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Pick up a config edit made just before quitting instead of writing
        // the old trail values back over it.
        if self.config_watch {
            self.poll_config_watch(Instant::now());
        }
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
                                           summon、follow）
  nono --check [--assets DIR]              校验素材目录并退出
  nono --list                              列出素材动作/表情
  nono --list-displays                     列出显示器（逻辑坐标与缩放）
  nono --render-frame <form> <kind> <slug> <frame> <scale> <out.png>
                                           渲染单帧为 PNG（kind: walk|action|expression）
  nono --render-icon <out.png> [size]      生成托盘图标
环境变量:
  NONO_ASSETS                              覆盖素材目录
";

enum Command {
    Run {
        assets: PathBuf,
        trace: bool,
        play: Option<String>,
        play_delay: f32,
        tint: Option<[u8; 3]>,
    },
    Check {
        assets: PathBuf,
    },
    List {
        assets: PathBuf,
    },
    ListDisplays,
    RenderFrame {
        assets: PathBuf,
        form: u8,
        kind: String,
        slug: String,
        frame: f32,
        scale: f32,
        out: PathBuf,
        tint: Option<[u8; 3]>,
    },
    RenderIcon {
        assets: PathBuf,
        out: PathBuf,
        size: u32,
        tint: Option<[u8; 3]>,
    },
}

fn parse_args() -> Result<Command> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut assets = config::default_asset_root();
    let mut trace = false;
    let mut play: Option<String> = None;
    let mut play_delay = 0.0f32;
    let mut tint: Option<[u8; 3]> = None;
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
                play = Some(
                    args.get(i)
                        .context("--play needs an id like action:1:cube")?
                        .clone(),
                );
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
            "--check" => {
                command = Some(Command::Check {
                    assets: assets.clone(),
                })
            }
            "--list" => {
                command = Some(Command::List {
                    assets: assets.clone(),
                })
            }
            "--list-displays" => command = Some(Command::ListDisplays),
            "--render-frame" => {
                if i + 6 >= args.len() {
                    bail!("--render-frame needs <form> <kind> <slug> <frame> <scale> <out.png>");
                }
                let form = args[i + 1]
                    .parse::<u8>()
                    .context("form must be 1, 2 or 3")?;
                let kind = args[i + 2].clone();
                let slug = args[i + 3].clone();
                let frame = args[i + 4]
                    .parse::<f32>()
                    .context("frame must be a number")?;
                let scale = args[i + 5]
                    .parse::<f32>()
                    .context("scale must be a number")?;
                let out = PathBuf::from(&args[i + 6]);
                command = Some(Command::RenderFrame {
                    assets: assets.clone(),
                    form,
                    kind,
                    slug,
                    frame,
                    scale,
                    out,
                    tint,
                });
                i += 6;
            }
            "--render-icon" => {
                if i + 1 >= args.len() {
                    bail!("--render-icon needs <out.png> [size]");
                }
                let out = PathBuf::from(&args[i + 1]);
                let size = args
                    .get(i + 2)
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(64);
                command = Some(Command::RenderIcon {
                    assets: assets.clone(),
                    out,
                    size,
                    tint,
                });
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

    Ok(command.unwrap_or(Command::Run {
        assets,
        trace,
        play,
        play_delay,
        tint,
    }))
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

fn spec_for(
    catalog: &Catalog,
    form: u8,
    kind: &str,
    slug: &str,
) -> Result<nono_core::AnimationSpec> {
    let spec = match kind {
        "walk" => {
            let dir =
                Direction::from_key(slug).with_context(|| format!("unknown direction {slug}"))?;
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
        Command::ListDisplays => {
            let Some(displays) = nono_platform::displays() else {
                bail!("this platform does not expose display geometry");
            };
            for display in displays {
                println!(
                    "{}id={} name={:?} bounds=({:.0}, {:.0}, {:.0}×{:.0}) scale={}",
                    if display.primary { "[主] " } else { "     " },
                    display.id,
                    display.name,
                    display.bounds.x,
                    display.bounds.y,
                    display.bounds.w,
                    display.bounds.h,
                    display.scale
                );
            }
            Ok(())
        }
        Command::Check { assets } => {
            init_tracing(false);
            let catalog =
                Catalog::load(&assets).with_context(|| format!("loading {}", assets.display()))?;
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
            println!(
                "ok: {} animations in {} (assets v{})",
                total,
                assets.display(),
                catalog.asset_version().unwrap_or("?")
            );
            Ok(())
        }
        Command::List { assets } => {
            init_tracing(false);
            let catalog = Catalog::load(&assets)?;
            for form in catalog.forms() {
                println!("{}", form.name);
                println!(
                    "  动作: {}",
                    form.actions
                        .iter()
                        .map(|a| a.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ")
                );
                println!(
                    "  表情: {}",
                    form.expressions
                        .iter()
                        .map(|e| e.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ")
                );
            }
            Ok(())
        }
        Command::RenderFrame {
            assets,
            form,
            kind,
            slug,
            frame,
            scale,
            out,
            tint,
        } => {
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
        Command::RenderIcon {
            assets,
            out,
            size,
            tint,
        } => {
            init_tracing(false);
            render_icon(assets, out, size, tint)
        }
        Command::Run {
            assets,
            trace,
            play,
            play_delay,
            tint,
        } => run(assets, trace, play, play_delay, tint),
    }
}

/// Builds the glow passes for every animation in the background, so playing a
/// clip for the first time does not pause on generation.
fn spawn_glow_warmer(cache: PathBuf, assets: PathBuf, tint: Option<[u8; 3]>, form: u8) {
    std::thread::spawn(move || {
        // Let the window and the first animation settle before using the CPU.
        std::thread::sleep(Duration::from_millis(1500));
        let mut files = Vec::new();
        if let Ok(dirs) = std::fs::read_dir(&assets) {
            for dir in dirs.flatten().map(|entry| entry.path()) {
                if !dir.is_dir() {
                    continue;
                }
                if let Ok(items) = std::fs::read_dir(&dir) {
                    for item in items.flatten() {
                        let path = item.path();
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if path.extension().and_then(|e| e.to_str()) == Some("json")
                            && name != "manifest.json"
                            && name != "meta.json"
                        {
                            files.push(path);
                        }
                    }
                }
            }
        }
        // Warm the form the user is looking at first: clicking an action of the
        // current form is the common case.
        let prefix = format!("nono-form{form}-");
        files.sort_by_key(|path| !path.to_string_lossy().contains(&prefix));
        tracing::debug!(files = files.len(), ?tint, form, "warming glow/tint cache");
        nono_render::warm_cache_with_tint(&cache, &files, tint);
        tracing::debug!("glow/tint cache warm");
    });
}

fn run(
    assets: PathBuf,
    trace: bool,
    play: Option<String>,
    play_delay: f32,
    tint: Option<[u8; 3]>,
) -> Result<()> {
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
        form1_walk_anim_speed = config.form1_walk_anim_speed,
        asset_version = catalog.asset_version().unwrap_or("?"),
        "starting 超能 NONO"
    );

    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("creating the winit event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    spawn_glow_warmer(
        config::cache_dir(),
        assets.clone(),
        config.tint,
        config.form,
    );
    let mut app = App::new(assets, catalog, config, config_path, proxy)?;
    app.initial_play = play;
    app.play_delay = play_delay;
    event_loop
        .run_app(&mut app)
        .context("running the event loop")?;
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

    fn display(id: u32, primary: bool, x: f32, w: f32) -> Display {
        Display {
            id,
            name: format!("display-{id}"),
            bounds: Rect::new(x, 25.0, w, 1000.0),
            scale: 2.0,
            primary,
        }
    }

    #[test]
    fn display_selection_defaults_to_the_primary() {
        let displays = vec![
            display(7, true, 0.0, 1600.0),
            display(9, false, 1600.0, 1200.0),
        ];
        assert_eq!(resolve_display_ids(None, &displays), vec![7]);
        assert_eq!(resolve_display_ids(Some(&[9]), &displays), vec![9]);
        // Unknown ids are dropped; if nothing valid remains, the primary wins.
        assert_eq!(resolve_display_ids(Some(&[42]), &displays), vec![7]);
        assert_eq!(resolve_display_ids(Some(&[42, 9]), &displays), vec![9]);
    }

    #[test]
    fn shared_display_edges_are_detected() {
        let left = display(1, true, 0.0, 1600.0);
        let right = display(2, false, 1600.0, 1200.0);
        let all = vec![&left, &right];
        assert!(edge_has_neighbor(&all, &left, ScreenEdge::Right));
        assert!(edge_has_neighbor(&all, &right, ScreenEdge::Left));
        assert!(!edge_has_neighbor(&all, &left, ScreenEdge::Left));
        assert!(!edge_has_neighbor(&all, &left, ScreenEdge::Top));
        assert!(!edge_has_neighbor(&all, &left, ScreenEdge::Bottom));

        // A gap between the screens means no crossing edge.
        let far = display(3, false, 2000.0, 1200.0);
        let all = vec![&left, &far];
        assert!(!edge_has_neighbor(&all, &left, ScreenEdge::Right));
    }

    #[test]
    fn adjacent_enabled_displays_form_a_continuous_roam_area() {
        let displays = vec![
            display(1, true, 0.0, 1600.0),
            display(2, false, 1600.0, 1200.0),
        ];
        let margins = (20.0, 30.0, 20.0, 30.0);
        let area = compute_roam_rects(&displays, &[1, 2], margins, 1.0);
        assert_eq!(area.rects().len(), 2);
        // The two roam rects touch at the seam, so the pet can cross it.
        assert!(area.contains(Point::new(1600.0, 500.0)));
        assert!(area.segment_inside(Point::new(1580.0, 500.0), Point::new(1620.0, 500.0)));
        // Outer edges keep the silhouette margin.
        assert!(!area.contains(Point::new(10.0, 500.0)));

        // With only the primary enabled the same edge is an outer edge again.
        let single = compute_roam_rects(&displays, &[1], margins, 1.0);
        assert_eq!(single.rects().len(), 1);
        assert!(!single.contains(Point::new(1590.0, 500.0)));
    }

    #[test]
    fn body_hit_uses_the_reported_body_centre() {
        // Real form-1 idle values at scale 1.5: the feet anchor is the screen
        // position; the body circle floats a little above it.
        let anchor = Point::new(1280.0, 693.0);
        let hit = (0.611, -29.067, 5.333);
        let center = Point::new(1280.0 + 0.611 * 1.5, 693.0 - 29.067 * 1.5);
        assert!(body_hit_contains(anchor, hit, 1.5, center));
        assert!(body_hit_contains(
            anchor,
            hit,
            1.5,
            Point::new(center.x + 9.0, center.y)
        ));
        // Outside the radius (which has a 10 pt minimum for usability).
        assert!(!body_hit_contains(
            anchor,
            hit,
            1.5,
            Point::new(center.x + 11.0, center.y)
        ));
        // The old buggy implementation shifted the circle up-left by the glow
        // margin; a click clearly up-left of the body must not count.
        assert!(!body_hit_contains(
            anchor,
            hit,
            1.5,
            Point::new(center.x - 20.0, center.y - 20.0)
        ));
    }

    #[test]
    fn ghost_frames_with_different_glow_pads_pin_to_their_own_anchor() {
        let dpi = 2.0;
        let anchor = Point::new(1000.0, 500.0);
        // Same screen anchor, but two walk directions with different margins.
        let small = ghost_frame_origin(anchor, (10.0, 20.0), dpi);
        let large = ghost_frame_origin(anchor, (14.0, 26.0), dpi);
        assert!((small.x + 10.0 - anchor.x * dpi).abs() < 1e-3);
        assert!((small.y + 20.0 - anchor.y * dpi).abs() < 1e-3);
        assert!((large.x + 14.0 - anchor.x * dpi).abs() < 1e-3);
        assert!((large.y + 26.0 - anchor.y * dpi).abs() < 1e-3);
        assert_ne!(
            small, large,
            "different frame paddings need different origins"
        );
    }

    #[test]
    fn stage_union_keeps_mixed_size_ghosts_inside() {
        let current = Rect::new(100.0, 100.0, 294.0, 315.0);
        let ghost = Rect::new(80.0, 90.0, 318.0, 339.0);
        let union = union_rect(current, ghost);
        assert!(union.x <= ghost.x && union.y <= ghost.y);
        assert!(union.right() >= ghost.right());
        assert!(union.bottom() >= ghost.bottom());
        assert_eq!(union, Rect::new(80.0, 90.0, 318.0, 339.0));
    }

    #[test]
    fn crop_into_clamps_out_of_bounds() {
        let src = vec![255u8; 4 * 4 * 4];
        let mut out = Vec::new();
        // The window stage can be one pixel larger than the tight frame after
        // rounding; the crop must clamp instead of panicking.
        assert_eq!(App::crop_into(&src, 4, 0, 0, 5, 5, &mut out), (4, 4));
        assert_eq!(out.len(), 4 * 4 * 4);
        assert_eq!(App::crop_into(&src, 4, 3, 3, 9, 9, &mut out), (1, 1));
        assert_eq!(out.len(), 4);
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

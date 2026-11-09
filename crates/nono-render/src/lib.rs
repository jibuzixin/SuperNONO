//! ThorVG-backed renderer for the vector Lottie assets.
//!
//! The software canvas (`SwCanvas`) is used on purpose: it keeps the process
//! free of a GPU context and is fast enough for a 25 fps pet that only redraws
//! while an animation is playing.
//!
//! Pixel layout: ThorVG's `ABGR8888` is premultiplied and, on little-endian
//! targets, lays out bytes as `R, G, B, A` — exactly what CoreGraphics wants for
//! a `kCGImageAlphaPremultipliedLast` image.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use nono_core::AnimationSpec;
use thorvg::{ColorSpace, EngineOption, LottieAnimation, Paint, Picture, SwCanvas, Thorvg};

mod glow;
pub use glow::GlowSettings;

// `tvg_paint_ref` is not exposed by the safe wrapper on purpose (it hands out
// raw refcount control), but it is the documented way to add an *existing*
// picture to a canvas.  We need that because `Paint::duplicate()` is a deep
// copy: frames driven on the original animation would not reach the copy.
// Sharing the animation's own picture is what the C API examples do.
extern "C" {
    fn tvg_paint_ref(paint: *mut std::ffi::c_void) -> u16;
    fn tvg_paint_unref(paint: *mut std::ffi::c_void, free: bool) -> u16;
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("thorvg error: {0}")]
    Thorvg(#[from] thorvg::Error),
    #[error("no animation loaded")]
    NotLoaded,
    #[error("invalid target size {0}x{1}")]
    InvalidSize(u32, u32),
    #[error("image encoding failed: {0}")]
    Image(#[from] image::ImageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("lottie json error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Returns freed-but-still-resident allocator pages to the OS.
///
/// After a large action (e.g. the 877x480 form-2 ball) the Lottie scene and the
/// pixel buffer are released, but the allocator keeps the pages.  macOS exposes
/// a pressure-relief call for exactly this; on other platforms it is a no-op.
/// Fades the outermost `feather` pixels to transparent so a hard canvas-edge
/// clip reads as a soft border instead of a rectangle.
fn soften_edges(buffer: &mut [u32], width: u32, height: u32, feather: u32) {
    let (w, h) = (width as usize, height as usize);
    let f = feather as usize;
    if f == 0 || w == 0 || h == 0 {
        return;
    }
    for y in 0..h {
        let dy = y.min(h - 1 - y).min(f);
        for x in 0..w {
            let dx = x.min(w - 1 - x).min(f);
            let d = dx.min(dy);
            if d >= f {
                continue;
            }
            let index = y * w + x;
            let px = buffer[index];
            let a = (px >> 24) & 0xff;
            if a == 0 {
                continue;
            }
            let mul = d as f32 / f as f32;
            let na = (a as f32 * mul).round() as u32;
            let r = (((px >> 16) & 0xff) as f32 * mul).round() as u32;
            let g = (((px >> 8) & 0xff) as f32 * mul).round() as u32;
            let b = ((px & 0xff) as f32 * mul).round() as u32;
            buffer[index] = (na << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

fn release_allocator_memory() {
    #[cfg(target_os = "macos")]
    unsafe {
        extern "C" {
            fn malloc_default_zone() -> *mut std::ffi::c_void;
            fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
        }
        let _ = malloc_zone_pressure_relief(malloc_default_zone(), 0);
    }
}

/// A single-animation renderer.  Loading a new animation releases the previous
/// one, which keeps steady-state memory proportional to the currently playing
/// clip rather than to the whole 34 MB asset pack.
pub struct Renderer {
    // Field order matters: canvas/anim borrow the engine and must be dropped
    // before the engine guard terminates ThorVG and frees its global caches.
    canvas: Option<SwCanvas<'static>>,
    anim: Option<LottieAnimation<'static>>,
    buffer: Vec<u32>,
    width: u32,
    height: u32,
    loaded_id: Option<String>,
    placement: nono_core::catalog::Placement,
    applied_offset: (f32, f32),
    base_px: (f32, f32),
    pixel_scale: f32,
    frame_count: u32,
    tint: Option<[u8; 3]>,
    spec_anchor: (f32, f32),
    body_hit: Option<(f32, f32, f32)>,
    /// Where generated tinted Lottie copies are cached.
    tint_cache_dir: Option<PathBuf>,
    /// Runtime glow settings (soft halo rebuilt from `nono.glowShapes`).
    glow: GlowSettings,
    /// One blurred, offscreen pass per (colour, blur, strength) group.
    glow_passes: Vec<GlowPass>,
    /// Transparent margin added on every side so the halo is not cut by the
    /// window edge (canvas units; 0 when the clip has no glow).
    glow_pad: f32,
    glow_fade: Option<[f32; 2]>,
    engine: Box<Thorvg>,
}

/// One blurred, low-resolution pass that is composited under the main frame.
struct GlowPass {
    anim: LottieAnimation<'static>,
    canvas: SwCanvas<'static>,
    buffer: Vec<u32>,
    width: u32,
    height: u32,
    /// Glow buffer pixels per canvas pixel (1.0 = full resolution).
    scale: f32,
    sigma: f32,
    opacity: u8,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("loaded_id", &self.loaded_id)
            .finish()
    }
}

impl Renderer {
    pub fn new() -> Result<Self, RenderError> {
        let engine = Box::new(Thorvg::init()?);
        Ok(Self {
            canvas: None,
            anim: None,
            buffer: Vec::new(),
            width: 0,
            height: 0,
            loaded_id: None,
            placement: Default::default(),
            applied_offset: (0.0, 0.0),
            base_px: (0.0, 0.0),
            pixel_scale: 1.0,
            frame_count: 1,
            tint: None,
            spec_anchor: (0.0, 0.0),
            body_hit: None,
            tint_cache_dir: None,
            glow: GlowSettings::default(),
            glow_passes: Vec::new(),
            glow_pad: 0.0,
            glow_fade: None,
            engine,
        })
    }

    /// The engine reference used to create canvases/animations.
    ///
    /// SAFETY: the `Box<Thorvg>` lives in `self`, so its heap address is stable
    /// while the renderer exists.  `canvas`/`anim` are declared before `engine`
    /// and are therefore dropped first; no `'static` handle created here ever
    /// escapes the renderer.
    fn engine(&self) -> &'static Thorvg {
        // SAFETY: the Box lives inside `self`, so the engine's heap address is
        // stable across moves of the Renderer.  The borrowed canvas/animation
        // fields are declared before `engine` and dropped first; the 'static
        // handle never escapes this type.
        unsafe { &*(&*self.engine as *const Thorvg) }
    }

    /// Recolours the character's white body parts.  The assets are untouched;
    /// `None` restores the original white.
    pub fn set_tint(&mut self, tint: Option<[u8; 3]>) {
        // Pure white means "no tint", exactly like the original assets.
        self.tint = tint.filter(|t| *t != [255, 255, 255]);
    }

    /// Enables the on-disk cache for tinted Lottie copies.
    pub fn set_tint_cache_dir(&mut self, dir: Option<PathBuf>) {
        self.tint_cache_dir = dir;
    }

    /// Enables/adjusts the runtime glow.  Takes effect on the next animation
    /// load, so callers should reload the current clip after changing it.
    pub fn set_glow(&mut self, settings: GlowSettings) {
        self.glow = settings;
    }

    /// Number of glow passes currently active (0 when the clip has no glow).
    pub fn glow_passes(&self) -> usize {
        self.glow_passes.len()
    }

    /// Transparent margin (canvas units) that [`Renderer::load`] added around
    /// the canvas so the glow fits.  Window geometry must grow by the same
    /// amount, otherwise the halo is cut at the window edge.
    pub fn glow_pad(&self) -> f32 {
        self.glow_pad
    }

    /// Builds (or reuses) the glow passes for `file` and loads them into
    /// offscreen canvases.
    fn load_glow(
        &mut self,
        _file: &Path,
        specs: Vec<glow::GlowPassSpec>,
        inner_w: u32,
        inner_h: u32,
        pad: f32,
        dpi: f32,
    ) -> Result<(), RenderError> {
        self.glow_passes.clear();
        // `NONO_GLOW_OFF=1` disables the effect for A/B screenshots and for
        // users whose GPU-less CPU cannot afford the extra blur.
        if !self.glow.enabled || std::env::var_os("NONO_GLOW_OFF").is_some() {
            return Ok(());
        }
        if specs.is_empty() {
            return Ok(());
        }
        let scale = self.glow.glow_scale(specs.len(), inner_w, inner_h);
        let pad = pad * scale;
        let w = ((inner_w as f32 * scale).round() as u32).max(1) + (pad as u32) * 2;
        let h = ((inner_h as f32 * scale).round() as u32).max(1) + (pad as u32) * 2;
        let mut passes = Vec::with_capacity(specs.len());
        for spec in specs {
            let mut anim = self.engine().lottie_animation()?;
            let path = spec.path.to_str().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 glow path")
            })?;
            anim.load_file(path)?;
            anim.set_size(
                (w as f32 - pad * 2.0).max(1.0),
                (h as f32 - pad * 2.0).max(1.0),
            )?;
            anim.picture_mut().translate(pad, pad)?;
            // `dpi` is canvas units → buffer pixels; `scale` is the glow
            // buffer's own resolution relative to the main buffer.
            let sigma = (spec.blur * self.glow.sigma_scale * dpi * scale).max(0.1);
            // Flash: glow alpha = blurred alpha × strength, and the layer's
            // animated opacity is already part of the blurred alpha.
            let opacity = (spec.strength.clamp(0.0, 1.0) * self.glow.alpha_scale * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;

            let mut canvas = self.engine().sw_canvas(EngineOption::Default)?;
            let mut buffer = vec![0u32; (w * h) as usize];
            // SAFETY: `buffer` is owned by the pass and outlives the canvas.
            unsafe {
                canvas.set_target(&mut buffer, w, w, h, ColorSpace::ABGR8888)?;
            }
            let mut scene = self.engine().scene()?;
            let raw = anim.picture().raw();
            // SAFETY: same ref-count dance as `load` — see the comment there.
            unsafe { tvg_paint_ref(raw as *mut std::ffi::c_void) };
            let picture = unsafe { <Picture as Paint>::from_raw_paint(raw) };
            scene.add(picture)?;
            // `scene.add` refs the paint (same as `canvas.add`); release the
            // reference we took above so switching animations cannot leak.
            unsafe { tvg_paint_unref(raw as *mut std::ffi::c_void, false) };
            scene.set_opacity(opacity)?;
            scene.add_gaussian_blur_effect(
                thorvg::GaussianBlur::new()
                    .sigma(f64::from(sigma))
                    .direction(thorvg::BlurDirection::Both)
                    .border(thorvg::BlurBorder::Duplicate)
                    .quality(self.glow.quality),
            )?;
            canvas.add(scene)?;
            passes.push(GlowPass {
                anim,
                canvas,
                buffer,
                width: w,
                height: h,
                scale,
                sigma,
                opacity,
            });
        }
        tracing::debug!(passes = passes.len(), w, h, "glow passes ready");
        self.glow_passes = passes;
        Ok(())
    }

    /// Renders every glow pass for `frame` (or the pass's current frame when
    /// `None`) and composites it under the main frame.
    fn draw_glow(&mut self, frame: Option<f32>) {
        if self.glow_passes.is_empty() {
            return;
        }
        let fade = match (self.glow_fade, frame) {
            (Some([start, end]), Some(frame)) if end > start => {
                let t = ((frame - start) / (end - start)).clamp(0.0, 1.0);
                1.0 - t
            }
            _ => 1.0,
        };
        if fade <= 0.0 {
            return;
        }
        let (base_px, applied, pixel_scale, placement_scale) = (
            self.base_px,
            self.applied_offset,
            self.pixel_scale,
            self.placement.scale,
        );
        let Self {
            buffer,
            width,
            height,
            glow_passes,
            ..
        } = self;
        let main = unsafe {
            std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 4)
        };
        for pass in glow_passes.iter_mut() {
            let _ = pass.anim.picture_mut().translate(
                base_px.0 + applied.0 * pixel_scale * placement_scale,
                base_px.1 + applied.1 * pixel_scale * placement_scale,
            );
            if let Some(frame) = frame {
                let _ = pass.anim.set_frame(frame);
            }
            if pass.canvas.update().is_err()
                || pass.canvas.draw(true).is_err()
                || pass.canvas.sync().is_err()
            {
                continue;
            }
            let glow = unsafe {
                std::slice::from_raw_parts(pass.buffer.as_ptr() as *const u8, pass.buffer.len() * 4)
            };
            glow::composite_under(
                main,
                *width as usize,
                *height as usize,
                glow,
                pass.width as usize,
                pass.height as usize,
                pass.scale,
                fade,
            );
        }
        tracing::trace!(sigma = ?glow_passes.iter().map(|p| p.sigma).collect::<Vec<_>>(),
                        opacity = ?glow_passes.iter().map(|p| p.opacity).collect::<Vec<_>>(),
                        "glow frame");
    }

    /// Removes every generated tinted copy.  Called when the palette changes so
    /// old colours do not accumulate on disk.
    pub fn clear_tint_cache(&mut self) {
        if let Some(dir) = &self.tint_cache_dir {
            let _ = std::fs::remove_dir_all(dir.join("tinted"));
        }
    }

    /// Body hit circle `(dx, dy, radius)` in **placement-scaled canvas units**
    /// relative to the pet anchor.  Multiply by the user zoom to get logical
    /// screen points.  Computed from the tinted body pixels, so it follows the
    /// current pose and ignores transparent areas/fins/props.
    pub fn body_hit(&self) -> Option<(f32, f32, f32)> {
        self.body_hit
    }

    fn update_body_hit(&mut self) {
        self.body_hit = None;
        let w = self.width as usize;
        let h = self.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let tn = match self.tint {
            Some(t) => {
                let m = t[0].max(t[1]).max(t[2]) as f32;
                if m < 32.0 {
                    return;
                }
                Some([t[0] as f32 / m, t[1] as f32 / m, t[2] as f32 / m])
            }
            // No tint: use the white body (the solid/ear-root fills are white).
            None => None,
        };
        let bytes = self.rgba8();
        let stride = 4usize;
        let gw = (w + stride - 1) / stride;
        let gh = (h + stride - 1) / stride;
        let mut mask = vec![false; gw * gh];
        let mut any = false;
        for gy in 0..gh {
            let y = gy * stride;
            if y >= h {
                break;
            }
            let row = y * w * 4;
            for gx in 0..gw {
                let x = gx * stride;
                if x >= w {
                    break;
                }
                let i = row + x * 4;
                if bytes[i + 3] < 200 {
                    continue;
                }
                let r = bytes[i] as f32;
                let g = bytes[i + 1] as f32;
                let b = bytes[i + 2] as f32;
                let m = r.max(g).max(b);
                let hit = if let Some(tn) = tn {
                    if m < 24.0 {
                        false
                    } else {
                        let p = [r / m, g / m, b / m];
                        p[0] * tn[0] + p[1] * tn[1] + p[2] * tn[2] >= 0.985
                    }
                } else {
                    // Untinted: white body + grey shading + grey outline, so
                    // fragmented pure-white highlights don't split the body.
                    let mn = r.min(g).min(b);
                    let mx = r.max(g).max(b);
                    mx >= 80.0 && (mx - mn) < 14.0
                };
                if hit {
                    mask[gy * gw + gx] = true;
                    any = true;
                }
            }
        }
        if !any {
            return;
        }
        // Largest connected component near the pet anchor (the body); effects,
        // bubbles and props are either smaller or farther away.
        //
        // Units: `spec_anchor` is in (placement-scaled) canvas units and
        // `scale` converts those to pixels.  The glow margin is *pixels* and
        // must be removed before dividing, otherwise the whole hit circle is
        // shifted up/left by the margin (most visible at Retina scale).
        let scale = self.pixel_scale.max(0.01);
        let pad_px = self.base_px.0;
        let anchor = self.spec_anchor;
        let mut seen = vec![false; gw * gh];
        let mut best: Option<(usize, (usize, usize, usize, usize))> = None;
        for gy in 0..gh {
            for gx in 0..gw {
                let idx = gy * gw + gx;
                if !mask[idx] || seen[idx] {
                    continue;
                }
                let mut queue = std::collections::VecDeque::new();
                queue.push_back((gx, gy));
                seen[idx] = true;
                let (mut min_x, mut min_y, mut max_x, mut max_y) = (gx, gy, gx, gy);
                let mut count = 0usize;
                while let Some((cx, cy)) = queue.pop_front() {
                    count += 1;
                    min_x = min_x.min(cx);
                    min_y = min_y.min(cy);
                    max_x = max_x.max(cx);
                    max_y = max_y.max(cy);
                    for (dx, dy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                        let nx = cx as isize + dx;
                        let ny = cy as isize + dy;
                        if nx < 0 || ny < 0 || nx >= gw as isize || ny >= gh as isize {
                            continue;
                        }
                        let n = ny as usize * gw + nx as usize;
                        if mask[n] && !seen[n] {
                            seen[n] = true;
                            queue.push_back((nx as usize, ny as usize));
                        }
                    }
                }
                let ccx = ((min_x + max_x) as f32 * stride as f32 * 0.5 - pad_px) / scale;
                let ccy = ((min_y + max_y) as f32 * stride as f32 * 0.5 - pad_px) / scale;
                let dist = ((ccx - anchor.0).powi(2) + (ccy - anchor.1).powi(2)).sqrt();
                if dist > 110.0 {
                    continue;
                }
                if best.map(|(area, _)| count > area).unwrap_or(true) {
                    best = Some((count, (min_x, min_y, max_x, max_y)));
                }
            }
        }
        let Some((_, (min_x, min_y, max_x, max_y))) = best else {
            return;
        };
        let cx = ((min_x + max_x) as f32 * stride as f32 * 0.5 - pad_px) / scale - anchor.0;
        let cy = ((min_y + max_y) as f32 * stride as f32 * 0.5 - pad_px) / scale - anchor.1;
        let rw = (max_x - min_x) as f32 * stride as f32 / scale;
        let rh = (max_y - min_y) as f32 * stride as f32 / scale;
        let radius = 0.5 * rw.min(rh);
        if radius >= 4.0 {
            self.body_hit = Some((cx, cy, radius));
        }
    }

    pub fn loaded_id(&self) -> Option<&str> {
        self.loaded_id.as_deref()
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Cache path for a tinted copy of `spec`.
    ///
    /// The key includes the source path, size and mtime, the tint colour and a
    /// cache-format version, so editing the assets or changing the palette
    /// invalidates old files automatically.
    fn tinted_cache_path(&self, spec: &AnimationSpec, tint: [u8; 3]) -> Option<PathBuf> {
        tinted_cache_path(self.tint_cache_dir.as_deref()?, &spec.file, tint)
    }

    /// Loads `spec` and rasterises it at `canvas * scale` pixels.
    ///
    /// `scale` must already include the display scale factor (user zoom × dpi).
    pub fn load(&mut self, spec: &AnimationSpec, scale: f32) -> Result<(), RenderError> {
        // The per-animation tuning scale is applied as a canvas zoom (not just
        // an in-canvas picture scale), so actions that need to be enlarged do
        // not get clipped by the original canvas bounds.
        let k = spec.placement.scale.max(0.05);
        let inner_w = (spec.canvas.0 * scale * k).round().max(1.0) as u32;
        let inner_h = (spec.canvas.1 * scale * k).round().max(1.0) as u32;

        // Glow needs a transparent margin: the halo is wider than the character
        // and would otherwise be cut by the window edge (a visible straight
        // line).  The margin is derived from the largest blur radius.
        let specs = if self.glow.enabled && std::env::var_os("NONO_GLOW_OFF").is_none() {
            glow::passes(&spec.file, self.tint_cache_dir.as_deref()).unwrap_or_default()
        } else {
            Vec::new()
        };
        self.glow_fade = spec.glow_fade;
        let max_blur = specs.iter().map(|s| s.blur).fold(0.0f32, f32::max);
        let pad_units = if max_blur > 0.0 {
            (max_blur * self.glow.sigma_scale * 2.6)
                .ceil()
                .clamp(1.0, 64.0)
        } else {
            0.0
        };
        let pad = (pad_units * scale * k).round();
        self.glow_pad = pad_units;

        let width = inner_w + (pad as u32) * 2;
        let height = inner_h + (pad as u32) * 2;
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(RenderError::InvalidSize(width, height));
        }

        if self.canvas.is_none() {
            self.canvas = Some(self.engine().sw_canvas(EngineOption::Default)?);
        }
        // A canvas cannot be retargeted while a draw is in flight.
        if let Some(canvas) = self.canvas.as_mut() {
            let _ = canvas.sync();
        }

        let previous_pixels = (self.width as usize) * (self.height as usize);
        let new_pixels = (width as usize) * (height as usize);
        let shrinking = previous_pixels > 0 && new_pixels * 2 < previous_pixels;

        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.buffer.resize(new_pixels, 0);
            if shrinking {
                // Don't keep the huge action-sized allocation around at idle.
                self.buffer.shrink_to_fit();
            }
            let canvas = self.canvas.as_mut().expect("canvas created above");
            // SAFETY: the buffer is owned by `self`, is not moved while the
            // canvas exists (`Vec`'s heap allocation stays put when the struct
            // moves), and is at least `width * height` long.  It is replaced
            // only after `set_target` is called again.
            unsafe {
                canvas.set_target(&mut self.buffer, width, width, height, ColorSpace::ABGR8888)?;
            }
        }

        // Drop the old animation and clear the canvas before adding the new one.
        if let Some(canvas) = self.canvas.as_mut() {
            canvas.clear()?;
        }
        self.anim = None;
        if shrinking {
            release_allocator_memory();
        }

        let feather = spec.feather.unwrap_or(0);
        let mut anim = self.engine().lottie_animation()?;
        if let Some(tint) = self.tint {
            // Recolour the tagged body layers in a copy of the Lottie JSON; the
            // file on disk is untouched.  The result is cached on disk because
            // generating it for the multi-MB cube actions takes seconds.
            let cache_path = self.tinted_cache_path(spec, tint);
            let mut loaded = false;
            if let Some(path) = cache_path.as_deref() {
                if path.is_file() {
                    match path.to_str().map(|path| anim.load_file(path)) {
                        Some(Ok(())) => {
                            loaded = true;
                            tracing::debug!(cache = %path.display(), "tinted animation cache hit");
                        }
                        Some(Err(err)) => {
                            tracing::warn!(%err, cache = %path.display(), "bad tint cache, regenerating");
                            let _ = std::fs::remove_file(path);
                            anim = self.engine().lottie_animation()?;
                        }
                        None => {}
                    }
                }
            }
            if !loaded {
                let bytes = std::fs::read(&spec.file)?;
                let tinted = tint_lottie_json(&bytes, tint)?;
                let mut loaded_from_file = false;
                if let Some(path) = cache_path.as_deref() {
                    if let Some(parent) = path.parent() {
                        if let Err(err) = std::fs::create_dir_all(parent) {
                            tracing::warn!(%err, dir = %parent.display(), "cannot create tint cache dir");
                        }
                    }
                    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
                    match std::fs::write(&tmp, &tinted) {
                        Ok(()) => {
                            if let Err(err) = std::fs::rename(&tmp, path) {
                                tracing::warn!(%err, cache = %path.display(), "cannot store tint cache");
                                let _ = std::fs::remove_file(&tmp);
                            }
                        }
                        Err(err) => {
                            tracing::warn!(%err, cache = %tmp.display(), "cannot write tint cache");
                        }
                    }
                    if path.is_file() {
                        match path.to_str().map(|path| anim.load_file(path)) {
                            Some(Ok(())) => loaded_from_file = true,
                            Some(Err(err)) => {
                                tracing::warn!(%err, cache = %path.display(), "tint cache failed to load");
                                let _ = std::fs::remove_file(path);
                                anim = self.engine().lottie_animation()?;
                            }
                            None => {}
                        }
                    }
                }
                if !loaded_from_file {
                    anim.load_data(&tinted)?;
                }
                // The JSON tree used for tinting is large; return its pages.
                release_allocator_memory();
            }
        } else {
            anim.load_file(spec.file.to_str().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 path")
            })?)?;
        }
        // Per-animation placement correction (see assets/tuning.json).
        // Scaling happens around the pet anchor so the feet stay put.  A start
        // offset can differ from the end offset; it is interpolated per frame.
        let placement = spec.placement;
        // Size the composition to the *inner* canvas, then shift it by the glow
        // margin; the margin itself stays transparent.
        anim.set_size(inner_w as f32, inner_h as f32)?;
        let start = placement.offset_start.unwrap_or(placement.offset);
        let base_x = pad;
        let base_y = pad;
        anim.picture_mut()
            .translate(pad + start.0 * scale * k, pad + start.1 * scale * k)?;
        self.placement = placement;
        self.applied_offset = start;
        self.base_px = (base_x, base_y);
        self.pixel_scale = scale;
        self.frame_count = spec.effective_frames();
        // ThorVG reports `InsufficientCondition` when the requested frame equals
        // the current one, so frame 0 (the loader default) is not set here.

        let raw = anim.picture().raw();
        // Give the canvas its own reference to the animation's picture so that
        // `set_frame` keeps driving what is on screen.
        // SAFETY: `raw` is a live `Tvg_Paint` owned by `anim`; bumping the
        // refcount before adding transfers exactly one reference to the canvas.
        unsafe { tvg_paint_ref(raw as *mut std::ffi::c_void) };
        let picture = unsafe { <Picture as Paint>::from_raw_paint(raw) };
        let canvas = self.canvas.as_mut().expect("canvas created above");
        canvas.add(picture)?;
        // `canvas.add` takes its own reference (Scene::insert calls ref()); the
        // extra reference we created above for the borrowed Picture wrapper is
        // not consumed by it.  Release it explicitly, otherwise every loaded
        // animation leaks its whole scene (~17 MB for the cube).
        unsafe { tvg_paint_unref(raw as *mut std::ffi::c_void, false) };
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;
        if feather > 0 {
            soften_edges(&mut self.buffer, self.width, self.height, feather);
        }
        // Body-hit math wants the anchor in placement-scaled canvas units; the
        // glow margin is handled separately in `update_body_hit`.
        self.spec_anchor = (spec.pet_anchor.0 * k, spec.pet_anchor.1 * k);
        if let Err(err) = self.load_glow(&spec.file, specs, inner_w, inner_h, pad, scale * k) {
            tracing::warn!(%err, file = %spec.file.display(), "glow passes unavailable");
        }
        self.update_body_hit();
        self.anim = Some(anim);
        self.loaded_id = Some(spec.id.clone());
        release_allocator_memory();
        Ok(())
    }

    /// Seeks the loaded animation and redraws the frame.
    ///
    /// `LottieLoader::frame` returns `false` (surfaced as
    /// `InsufficientCondition`) when the requested frame is within 0.0009 of the
    /// current one, so that case is skipped instead of treated as an error.
    pub fn render_frame(&mut self, frame: f32) -> Result<(), RenderError> {
        let target = frame.max(0.0);
        // Interpolate the placement offset so animations whose start and end
        // need different corrections (form-2 boot) stay aligned at both ends.
        let start = self.placement.offset_start.unwrap_or(self.placement.offset);
        let end = self.placement.offset;
        let t = if self.frame_count > 1 {
            (target / (self.frame_count - 1) as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let desired = (
            start.0 + (end.0 - start.0) * t,
            start.1 + (end.1 - start.1) * t,
        );
        let delta = (
            (desired.0 - self.applied_offset.0).abs(),
            (desired.1 - self.applied_offset.1).abs(),
        );
        let anim = self.anim.as_mut().ok_or(RenderError::NotLoaded)?;
        if delta.0 > 0.001 || delta.1 > 0.001 {
            // `Picture::translate` sets the translation (it does not accumulate),
            // so always pass the absolute position in canvas pixels.
            anim.picture_mut().translate(
                self.base_px.0 + desired.0 * self.pixel_scale * self.placement.scale,
                self.base_px.1 + desired.1 * self.pixel_scale * self.placement.scale,
            )?;
            self.applied_offset = desired;
        }
        let current = anim.frame().unwrap_or(-1.0);
        if (current - target).abs() > 0.0009 {
            anim.set_frame(target)?;
        }
        let canvas = self.canvas.as_mut().ok_or(RenderError::NotLoaded)?;
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;
        // The body mask must be measured on the character only: the halo would
        // otherwise grow the clickable circle.
        self.update_body_hit();
        self.draw_glow(Some(target));
        Ok(())
    }

    /// Re-renders the current frame (used after a window resize).
    pub fn redraw(&mut self) -> Result<(), RenderError> {
        let canvas = self.canvas.as_mut().ok_or(RenderError::NotLoaded)?;
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;
        self.update_body_hit();
        self.draw_glow(None);
        Ok(())
    }

    /// Premultiplied RGBA8 pixels, `width * height * 4` bytes.
    pub fn rgba8(&self) -> &[u8] {
        // SAFETY: `u32` has no padding and the buffer is always initialised;
        // the returned slice borrows self for as long as the renderer lives.
        unsafe {
            std::slice::from_raw_parts(
                self.buffer.as_ptr() as *const u8,
                self.buffer.len() * std::mem::size_of::<u32>(),
            )
        }
    }

    /// Renders a single frame of a Lottie file to a PNG.  Used by
    /// `nono --render-frame` and by the icon/asset tooling.
    pub fn render_png(
        &mut self,
        spec: &AnimationSpec,
        scale: f32,
        frame: f32,
        out: &Path,
    ) -> Result<(), RenderError> {
        self.load(spec, scale)?;
        self.render_frame(frame)?;
        let (w, h) = self.size();
        let image = image::RgbaImage::from_raw(w, h, self.rgba8().to_vec())
            .ok_or(RenderError::InvalidSize(w, h))?;
        image.save(out)?;
        Ok(())
    }
}

// ── motion trail ────────────────────────────────────────────────────

/// Erases the pet's ground shadow from a rendered frame **in place**.
///
/// Walk canvases draw a separate soft shadow on the ground under the pet
/// anchor.  Afterimages should only repeat the character, not that shadow.  We
/// flood-fill the opaque component that contains the anchor (the asset contract
/// puts the anchor in the shadow) and clear it.
///
/// If the component is large or reaches far above the anchor, it is the pet
/// itself (or touches it), so nothing is erased.
pub fn strip_ground_shadow(rgba: &mut [u8], width: u32, height: u32, anchor: (f32, f32)) {
    if width == 0 || height == 0 {
        return;
    }
    let w = width as usize;
    let h = height as usize;
    if rgba.len() != w * h * 4 {
        return;
    }
    let opaque = |idx: usize| rgba[idx * 4 + 3] > 0;

    // The shadow is centred on the anchor; start from the nearest drawn pixel.
    let (ax, ay) = anchor;
    let mut seed: Option<(usize, usize, f32)> = None;
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if !opaque(idx) {
                continue;
            }
            let distance = (x as f32 - ax).powi(2) + (y as f32 - ay).powi(2);
            if seed.map_or(true, |(_, _, best)| distance < best) {
                seed = Some((x, y, distance));
            }
        }
    }
    let Some((sx, sy, _)) = seed else { return };

    // Flood-fill the connected component and remember its bounds.
    let mut stack = vec![(sx, sy)];
    let mut seen = vec![false; w * h];
    seen[sy * w + sx] = true;
    let mut component = Vec::new();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (sx, sy, sx, sy);
    while let Some((x, y)) = stack.pop() {
        component.push(y * w + x);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let idx = ny * w + nx;
            if !seen[idx] && opaque(idx) {
                seen[idx] = true;
                stack.push((nx, ny));
            }
        }
    }

    // The ground shadow is small and sits around the anchor.  A component that
    // covers a large part of the canvas, or reaches well above the anchor, is
    // the pet: keep it rather than erasing the whole afterimage.
    let component_h = max_y - min_y + 1;
    if component.len() * 5 > w * h || component_h * 3 > h {
        return;
    }
    let top_guard = (h as f32 * 0.20) as usize;
    if min_y + top_guard < sy {
        return;
    }

    for idx in component {
        let byte = idx * 4;
        rgba[byte..byte + 4].fill(0);
    }
}

/// One pre-rendered historical frame, positioned in stage pixels.
///
/// `pixels` is premultiplied RGBA8, exactly what [`Renderer::rgba8`] returns.
/// `alpha` scales the whole layer before it is composited, which turns a past
/// frame into a translucent afterimage.
pub struct GhostFrame<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
    pub alpha: f32,
}

/// Composites `ghosts` (oldest first) onto a transparent stage, then draws the
/// current frame on top.
///
/// This is how the walking afterimage is built: the pet window is temporarily
/// enlarged by the trail margin, past frames are redrawn at their old positions
/// with decaying opacity, and the live frame sits above them.  Everything is
/// premultiplied RGBA8, so blending is `src + dst * (1 - src_alpha)`.
pub fn compose_trail(
    stage: &mut [u8],
    stage_w: u32,
    stage_h: u32,
    current: &[u8],
    current_w: u32,
    current_h: u32,
    current_left: i32,
    current_top: i32,
    ghosts: &[GhostFrame<'_>],
) {
    debug_assert_eq!(stage.len(), stage_w as usize * stage_h as usize * 4);
    debug_assert_eq!(current.len(), current_w as usize * current_h as usize * 4);
    if stage.len() != stage_w as usize * stage_h as usize * 4
        || current.len() != current_w as usize * current_h as usize * 4
    {
        return;
    }
    stage.fill(0);
    for ghost in ghosts {
        blend_layer(
            stage,
            stage_w,
            stage_h,
            ghost.pixels,
            ghost.width,
            ghost.height,
            ghost.left,
            ghost.top,
            ghost.alpha,
        );
    }
    blend_layer(
        stage,
        stage_w,
        stage_h,
        current,
        current_w,
        current_h,
        current_left,
        current_top,
        1.0,
    );
}

/// Blends one premultiplied layer into `dst`, clipping it to the stage.
#[allow(clippy::too_many_arguments)]
fn blend_layer(
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
    src: &[u8],
    src_w: u32,
    src_h: u32,
    left: i32,
    top: i32,
    alpha: f32,
) {
    debug_assert_eq!(dst.len(), dst_w as usize * dst_h as usize * 4);
    debug_assert_eq!(src.len(), src_w as usize * src_h as usize * 4);
    if dst.len() != dst_w as usize * dst_h as usize * 4
        || src.len() != src_w as usize * src_h as usize * 4
    {
        return;
    }
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 || src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return;
    }
    let x0 = left.max(0);
    let y0 = top.max(0);
    let x1 = (left + src_w as i32).min(dst_w as i32);
    let y1 = (top + src_h as i32).min(dst_h as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for y in y0..y1 {
        for x in x0..x1 {
            let si = (((y - top) as u32 * src_w + (x - left) as u32) * 4) as usize;
            let di = ((y as u32 * dst_w + x as u32) * 4) as usize;
            let sa = src[si + 3] as f32 / 255.0 * alpha;
            if sa <= 0.0 {
                continue;
            }
            let inv = 1.0 - sa;
            for c in 0..4 {
                dst[di + c] = (src[si + c] as f32 * alpha + dst[di + c] as f32 * inv).round() as u8;
            }
        }
    }
}

/// Recolours every fill tagged with `nono.dye` in a Lottie document.
///
/// - `solid`: the body / ear-root white.  Static fills become the exact tint;
///   colour-keyframed fills keep their brightness animation and are tinted
///   (a fade stays a fade in the selected colour).
/// - `shade`: the body's grey shading; tinted by its original brightness so the
///   shading levels stay distinct (and darker than the body).
/// - untagged fills (accent/eye/prop/effect/other) are never touched.
///
/// The document is parsed into an **order-preserving** tree.  ThorVG's Lottie
/// parser becomes extremely slow (seconds instead of milliseconds on the large
/// cube actions) when object keys are alphabetically sorted, which is what
/// `serde_json::Value`'s default `BTreeMap` would do.  Keeping the original key
/// order is therefore required, not just cosmetic.
/// Cache path for the tinted copy of `file` (kept in sync with the renderer's
/// own lookup so the background warmer can pre-generate them).
fn tinted_cache_path(dir: &Path, file: &Path, tint: [u8; 3]) -> Option<PathBuf> {
    let metadata = std::fs::metadata(file).ok()?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut hasher = DefaultHasher::new();
    file.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    mtime.hash(&mut hasher);
    tint.hash(&mut hasher);
    "nono-tint-v1".hash(&mut hasher);
    Some(
        dir.join("tinted")
            .join(format!("{:016x}.json", hasher.finish())),
    )
}

/// Writes the tinted copy for `file` if the cache does not have it yet.
fn ensure_tinted(cache_dir: &Path, file: &Path, tint: [u8; 3]) {
    let Some(path) = tinted_cache_path(cache_dir, file, tint) else {
        return;
    };
    if path.is_file() {
        return;
    }
    let Ok(bytes) = std::fs::read(file) else {
        return;
    };
    let Ok(tinted) = tint_lottie_json(&bytes, tint) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    if std::fs::write(&tmp, &tinted).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Pre-generates the glow passes and (optionally) the tinted Lottie copies for
/// `files`.  Runs on a background thread after startup: doing this lazily is
/// what makes the first click on an action stall.
pub fn warm_cache_with_tint(cache_root: &Path, files: &[PathBuf], tint: Option<[u8; 3]>) {
    glow::warm_cache(cache_root, files);
    let Some(tint) = tint.filter(|color| *color != [255, 255, 255]) else {
        return;
    };
    for file in files {
        ensure_tinted(cache_root, file, tint);
    }
}

fn tint_lottie_json(bytes: &[u8], tint: [u8; 3]) -> Result<Vec<u8>, serde_json::Error> {
    let mut root: JsonTree = serde_json::from_slice(bytes)?;
    walk_dye(&mut root, tint);
    serde_json::to_vec(&root)
}

/// Minimal order-preserving JSON tree (objects are `Vec<(key, value)>`).
#[derive(Debug, Clone)]
enum JsonTree {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<JsonTree>),
    Object(Vec<(String, JsonTree)>),
}

impl JsonTree {
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        match self {
            Self::Object(entries) => entries.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(number) => number.as_i64(),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(number) => number.as_f64(),
            _ => None,
        }
    }

    fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    fn number(value: f32) -> Self {
        // Colors are finite; fall back to zero rather than panicking.
        Self::Number(serde_json::Number::from_f64(value as f64).unwrap_or_else(|| 0.into()))
    }
}

impl<'de> serde::Deserialize<'de> for JsonTree {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TreeVisitor;
        impl<'de> serde::de::Visitor<'de> for TreeVisitor {
            type Value = JsonTree;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value")
            }

            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<JsonTree, E> {
                Ok(JsonTree::Bool(value))
            }

            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<JsonTree, E> {
                Ok(JsonTree::Number(value.into()))
            }

            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<JsonTree, E> {
                Ok(JsonTree::Number(value.into()))
            }

            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<JsonTree, E> {
                serde_json::Number::from_f64(value)
                    .map(JsonTree::Number)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<JsonTree, E> {
                Ok(JsonTree::String(value.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<JsonTree, E> {
                Ok(JsonTree::String(value))
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<JsonTree, E> {
                Ok(JsonTree::Null)
            }

            fn visit_none<E: serde::de::Error>(self) -> Result<JsonTree, E> {
                Ok(JsonTree::Null)
            }

            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<JsonTree, D::Error> {
                <JsonTree as serde::Deserialize>::deserialize(deserializer)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<JsonTree, A::Error> {
                let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(value) = seq.next_element()? {
                    values.push(value);
                }
                Ok(JsonTree::Array(values))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<JsonTree, A::Error> {
                let mut entries = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((key, value)) = map.next_entry::<String, JsonTree>()? {
                    entries.push((key, value));
                }
                Ok(JsonTree::Object(entries))
            }
        }
        deserializer.deserialize_any(TreeVisitor)
    }
}

impl serde::Serialize for JsonTree {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Number(value) => value.serialize(serializer),
            Self::String(value) => serializer.serialize_str(value),
            Self::Array(values) => {
                let mut seq = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Self::Object(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

fn walk_dye(node: &mut JsonTree, tint: [u8; 3]) {
    match node {
        JsonTree::Object(entries) => {
            let is_fill = entries
                .iter()
                .find(|(key, _)| key == "ty")
                .and_then(|(_, value)| value.as_str())
                .is_some_and(|value| matches!(value, "fl" | "st"));
            let dye = entries
                .iter()
                .find(|(key, _)| key == "nono")
                .and_then(|(_, value)| value.get("dye"))
                .and_then(JsonTree::as_str)
                .map(str::to_owned);
            if is_fill {
                if let Some(dye) = dye.as_deref() {
                    if let Some(c) = entries
                        .iter_mut()
                        .find(|(key, _)| key == "c")
                        .map(|(_, value)| value)
                    {
                        let animated = c.get("a").and_then(JsonTree::as_i64).unwrap_or(0) == 1;
                        if let Some(k) = c.get_mut("k") {
                            recolor_color_field(k, tint, dye, animated);
                        }
                    }
                }
            }
            for (_, value) in entries.iter_mut() {
                walk_dye(value, tint);
            }
        }
        JsonTree::Array(values) => {
            for value in values {
                walk_dye(value, tint);
            }
        }
        _ => {}
    }
}

fn recolor_color_field(k: &mut JsonTree, tint: [u8; 3], dye: &str, animated: bool) {
    match k {
        JsonTree::Array(values) => {
            if values.len() >= 3 && values.iter().take(3).all(JsonTree::is_number) {
                recolor_rgb(values, tint, dye, animated);
            } else {
                for value in values {
                    recolor_color_field(value, tint, dye, animated);
                }
            }
        }
        JsonTree::Object(entries) => {
            for key in ["s", "e"] {
                if let Some(value) = entries
                    .iter_mut()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value)
                {
                    recolor_color_field(value, tint, dye, animated);
                }
            }
        }
        _ => {}
    }
}

fn recolor_rgb(components: &mut [JsonTree], tint: [u8; 3], dye: &str, animated: bool) {
    let r = components[0].as_f64().unwrap_or(1.0) as f32;
    let g = components[1].as_f64().unwrap_or(1.0) as f32;
    let b = components[2].as_f64().unwrap_or(1.0) as f32;
    let brightness = r.max(g).max(b).clamp(0.0, 1.0);
    // A static solid fill is exactly the chosen colour; everything else keeps
    // its original brightness so the shading and keyframed fades survive.
    let factor = if dye == "solid" && !animated {
        1.0
    } else {
        brightness
    };
    components[0] = JsonTree::number(tint[0] as f32 / 255.0 * factor);
    components[1] = JsonTree::number(tint[1] as f32 / 255.0 * factor);
    components[2] = JsonTree::number(tint[2] as f32 / 255.0 * factor);
}

#[cfg(test)]
mod tests {
    use super::*;
    use nono_core::{Catalog, Direction};
    use std::path::PathBuf;

    // ThorVG's global engine state is not safe to initialise concurrently; the
    // application is single-threaded, so tests simply serialise here.
    static THORVG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Tests sample exact pixels, so they must not pay for the glow margin
    /// (which shifts every pixel by the halo padding).
    fn test_renderer() -> Renderer {
        let mut renderer = Renderer::new().unwrap();
        renderer.set_glow(GlowSettings {
            enabled: false,
            ..GlowSettings::default()
        });
        renderer
    }

    fn assets_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn software_canvas_uses_rgba_byte_order() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let engine = Thorvg::init().unwrap();
        let mut canvas = engine.sw_canvas(EngineOption::Default).unwrap();
        let mut buffer = vec![0u32; 8 * 8];
        unsafe {
            canvas
                .set_target(&mut buffer, 8, 8, 8, ColorSpace::ABGR8888)
                .unwrap()
        };
        let mut shape = engine.shape().unwrap();
        shape
            .append_rect(thorvg::Rect::new(0.0, 0.0, 8.0, 8.0))
            .unwrap();
        shape
            .set_fill_color(thorvg::Rgba::new(255, 0, 0, 255))
            .unwrap();
        canvas.add(shape).unwrap();
        canvas.update().unwrap();
        canvas.draw(true).unwrap();
        canvas.sync().unwrap();

        let bytes =
            unsafe { std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len() * 4) };
        assert_eq!(
            &bytes[0..4],
            &[255, 0, 0, 255],
            "expected RGBA memory order"
        );
    }

    #[test]
    fn lottie_frames_actually_change() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let spec = catalog.walk_spec(1, Direction::Down).unwrap();
        let mut renderer = test_renderer();
        renderer.load(&spec, 3.0).unwrap();
        renderer.render_frame(0.0).unwrap();
        let first = renderer.rgba8().to_vec();
        assert!(first.chunks(4).any(|px| px[3] > 0), "walk frame 0 is empty");

        renderer.render_frame(6.0).unwrap();
        let second = renderer.rgba8().to_vec();
        assert_ne!(first, second, "frame 6 should differ from frame 0");

        // The pet occupies a small part of the canvas but it must be visible.
        let opaque = second.chunks(4).filter(|px| px[3] > 8).count();
        assert!(opaque > 100, "only {opaque} opaque pixels rendered");
    }

    #[test]
    fn tint_recolours_the_white_body_only() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let spec = catalog.walk_spec(1, Direction::Down).unwrap();
        let mut renderer = test_renderer();
        renderer.set_tint(Some([255, 0, 0]));
        renderer.load(&spec, 2.5).unwrap();
        renderer.render_frame(0.0).unwrap();
        let after = renderer.rgba8().to_vec();
        let red = after
            .chunks_exact(4)
            .filter(|p| p[3] > 240 && p[0] > 200 && p[1] < 40 && p[2] < 40)
            .count();
        assert!(red > 50, "expected red-tinted body pixels, got {red}");
        // Cyan ears/fins are not body-white, so they keep their colour.
        let cyan = after
            .chunks_exact(4)
            .filter(|p| p[3] > 200 && p[0] < 120 && p[1] > 150 && p[2] > 150)
            .count();
        assert!(cyan > 100, "cyan parts must survive tinting, got {cyan}");
    }

    #[test]
    fn tint_preserves_object_key_order() {
        // Alphabetical key order makes ThorVG's Lottie parser pathologically
        // slow; keeping the source order is a performance requirement.
        let input = br#"{"z":1,"a":2,"layers":[{"ty":"fl","nono":{"dye":"solid"},"c":{"a":0,"k":[1,1,1,1]}}]}"#;
        let out = tint_lottie_json(input, [255, 0, 0]).unwrap();
        let text = std::str::from_utf8(&out).unwrap();
        let z = text.find("\"z\"").unwrap();
        let a = text.find("\"a\"").unwrap();
        let layers = text.find("\"layers\"").unwrap();
        assert!(z < a && a < layers, "key order changed: {text}");
        assert!(text.contains("\"k\":[1.0,0.0,0.0"), "{text}");
    }

    #[test]
    fn tint_uses_nono_dye_tags() {
        let root = assets_root();
        let surprise_path = root.join("nono-form1-expressions/nono1_exp_surprise.json");
        if !surprise_path.is_file() {
            return;
        }
        let bytes = std::fs::read(&surprise_path).unwrap();
        let before: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tinted = tint_lottie_json(&bytes, [255, 0, 0]).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&tinted).unwrap();
        fn first_static_fill(node: &serde_json::Value, dye: Option<&str>) -> Option<Vec<f64>> {
            match node {
                serde_json::Value::Object(map) => {
                    let is_fill = matches!(
                        map.get("ty").and_then(|v| v.as_str()),
                        Some("fl") | Some("st")
                    );
                    if is_fill {
                        let tag = map
                            .get("nono")
                            .and_then(|v| v.as_object())
                            .and_then(|n| n.get("dye"))
                            .and_then(|v| v.as_str());
                        let c = map.get("c").and_then(|v| v.as_object());
                        let static_color = c.and_then(|c| {
                            if c.get("a").and_then(|v| v.as_i64()).unwrap_or(0) == 0 {
                                c.get("k")
                            } else {
                                None
                            }
                        });
                        if let Some(k) = static_color.and_then(|v| v.as_array()) {
                            if k.len() >= 3 && tag == dye {
                                return Some(k.iter().take(3).filter_map(|v| v.as_f64()).collect());
                            }
                        }
                    }
                    for v in map.values() {
                        if let Some(c) = first_static_fill(v, dye) {
                            return Some(c);
                        }
                    }
                    None
                }
                serde_json::Value::Array(arr) => {
                    for v in arr {
                        if let Some(c) = first_static_fill(v, dye) {
                            return Some(c);
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        let _solid_before = first_static_fill(&before, Some("solid")).expect("a solid-tagged fill");
        let solid_after = first_static_fill(&after, Some("solid")).expect("a solid-tagged fill");
        assert_eq!(
            solid_after,
            vec![1.0, 0.0, 0.0],
            "solid fills become the exact tint"
        );
        let untagged_before = first_static_fill(&before, None).expect("an untagged fill");
        let untagged_after = first_static_fill(&after, None).expect("an untagged fill");
        assert_eq!(
            untagged_before, untagged_after,
            "untagged fills must not be touched"
        );
    }

    #[test]
    fn body_hit_tracks_the_tinted_body() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(root).unwrap();
        let spec = catalog.walk_spec(2, Direction::Down).unwrap();
        let mut renderer = test_renderer();
        renderer.set_tint(Some([255, 96, 150]));
        renderer.load(&spec, 3.0).unwrap();
        renderer.render_frame(0.0).unwrap();
        let (dx, dy, radius) = renderer.body_hit().expect("tinted body hit");
        assert!(radius > 5.0 && radius < 80.0, "radius {radius}");
        assert!(dy < 0.0, "body sits above the feet");
        assert!(dx.abs() < 30.0, "body near the anchor X");
    }

    #[test]
    fn body_hit_works_without_a_tint() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(root).unwrap();
        let spec = catalog.walk_spec(2, Direction::Down).unwrap();
        let mut renderer = test_renderer();
        renderer.load(&spec, 3.0).unwrap();
        renderer.render_frame(0.0).unwrap();
        let (dx, dy, radius) = renderer.body_hit().expect("white body hit");
        assert!(radius > 5.0 && radius < 80.0, "radius {radius}");
        assert!(dy < 0.0, "body sits above the feet");
        assert!(dx.abs() < 30.0, "body near the anchor X");
    }

    #[test]
    fn body_hit_is_independent_of_the_glow_margin() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(root).unwrap();
        let spec = catalog.walk_spec(2, Direction::Down).unwrap();

        let mut plain = test_renderer();
        plain.load(&spec, 3.0).unwrap();
        plain.render_frame(0.0).unwrap();
        let plain_hit = plain.body_hit().expect("plain body hit");

        let mut glow = Renderer::new().unwrap();
        glow.set_glow(GlowSettings {
            enabled: true,
            ..GlowSettings::default()
        });
        glow.load(&spec, 3.0).unwrap();
        assert!(
            glow.glow_pad() > 0.0,
            "this animation should have a glow margin"
        );
        glow.render_frame(0.0).unwrap();
        let glow_hit = glow.body_hit().expect("glowing body hit");

        // The halo margin is transparent padding; it must not shift or resize
        // the clickable body circle.
        assert!(
            (plain_hit.0 - glow_hit.0).abs() < 1.5,
            "{plain_hit:?} vs {glow_hit:?}"
        );
        assert!(
            (plain_hit.1 - glow_hit.1).abs() < 1.5,
            "{plain_hit:?} vs {glow_hit:?}"
        );
        assert!(
            (plain_hit.2 - glow_hit.2).abs() < 2.0,
            "{plain_hit:?} vs {glow_hit:?}"
        );
    }

    #[test]
    fn tint_keeps_side_fins_white() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let spec = catalog.expression_spec(2, "happy").unwrap();
        let mut renderer = test_renderer();
        renderer.set_tint(Some([255, 0, 0]));
        renderer.load(&spec, 3.0).unwrap();
        renderer.render_frame(15.0).unwrap();
        let buf = renderer.rgba8().to_vec();
        let width = renderer.size().0 as usize;
        let white_in = |x0: usize, x1: usize, y0: usize, y1: usize| {
            let mut n = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * width + x) * 4;
                    if buf[i] > 220 && buf[i + 1] > 220 && buf[i + 2] > 220 {
                        n += 1;
                    }
                }
            }
            n
        };
        let left = white_in(20, 75, 130, 165);
        let right = white_in(175, 225, 85, 130);
        assert!(left > 10, "left fin should stay white ({left})");
        assert!(right > 10, "right fin should stay white ({right})");
        let red = buf
            .chunks_exact(4)
            .filter(|p| p[3] > 240 && p[0] > 200 && p[1] < 40 && p[2] < 40)
            .count();
        assert!(red > 100, "body should be tinted, got {red}");
    }

    #[test]
    fn switching_animations_reuses_the_renderer() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let mut renderer = test_renderer();
        let walk = catalog.walk_spec(1, Direction::Down).unwrap();
        renderer.load(&walk, 2.0).unwrap();
        let walk_size = renderer.size();

        let happy = catalog.expression_spec(1, "happy").unwrap();
        renderer.load(&happy, 2.0).unwrap();
        assert_ne!(renderer.size(), walk_size);
        assert_eq!(renderer.loaded_id(), Some(happy.id.as_str()));
        renderer.render_frame(10.0).unwrap();
        assert!(renderer.rgba8().chunks(4).any(|px| px[3] > 8));
    }

    #[test]
    fn trail_compositing_fades_ghosts_under_the_current_frame() {
        // 1x1 premultiplied red ghost, 1x1 opaque green current frame on a 4x1
        // transparent stage.  The ghost lands at x=0, the pet at x=2.
        let ghost = [128u8, 0, 0, 128];
        let current = [0u8, 255, 0, 255];
        let mut stage = vec![0u8; 4 * 1 * 4];
        compose_trail(
            &mut stage,
            4,
            1,
            &current,
            1,
            1,
            2,
            0,
            &[GhostFrame {
                pixels: &ghost,
                width: 1,
                height: 1,
                left: 0,
                top: 0,
                alpha: 0.5,
            }],
        );

        // 50% of a 50%-opaque red => 25% red, still premultiplied.
        assert_eq!(&stage[0..4], &[64, 0, 0, 64]);
        // Untouched transparent pixel between ghost and pet.
        assert_eq!(&stage[4..8], &[0, 0, 0, 0]);
        // The current frame is always fully composited (not faded).
        assert_eq!(&stage[8..12], &[0, 255, 0, 255]);
        // Stage was cleared rather than accumulating.
        assert_eq!(&stage[12..16], &[0, 0, 0, 0]);
    }

    #[test]
    fn trail_compositing_clips_ghosts_at_the_stage_edge() {
        let ghost = [255u8, 0, 0, 255];
        let current = [0u8, 0, 255, 255];
        let mut stage = vec![0u8; 2 * 2 * 4];
        compose_trail(
            &mut stage,
            2,
            2,
            &current,
            1,
            1,
            0,
            0,
            &[GhostFrame {
                pixels: &ghost,
                width: 1,
                height: 1,
                left: -5,
                top: -5,
                alpha: 1.0,
            }],
        );
        // The off-stage ghost is fully clipped; only the pet remains.
        assert_eq!(&stage[0..4], &[0, 0, 255, 255]);
        assert!(stage[4..].iter().all(|b| *b == 0));
    }

    fn put(frame: &mut [u8], width: usize, x: usize, y: usize, alpha: u8) {
        let i = (y * width + x) * 4;
        frame[i..i + 4].copy_from_slice(&[alpha, alpha, alpha, alpha]);
    }

    fn alpha_at(frame: &[u8], width: usize, x: usize, y: usize) -> u8 {
        frame[(y * width + x) * 4 + 3]
    }

    #[test]
    fn trail_frames_keep_the_pet_but_drop_the_ground_shadow() {
        // 8x8 canvas: pet blob on top, separate ground shadow around the anchor.
        let mut frame = vec![0u8; 8 * 8 * 4];
        for (x, y) in [(2, 1), (3, 1), (2, 2), (3, 2)] {
            put(&mut frame, 8, x, y, 255);
        }
        for (x, y) in [(3, 5), (4, 5), (3, 6), (4, 6), (2, 6), (5, 6)] {
            put(&mut frame, 8, x, y, 255);
        }
        // A faint antialiased shadow edge pixel must go too.
        put(&mut frame, 8, 2, 5, 3);
        strip_ground_shadow(&mut frame, 8, 8, (4.0, 6.0));

        for (x, y) in [(2, 1), (3, 1), (2, 2), (3, 2)] {
            assert_eq!(
                alpha_at(&frame, 8, x, y),
                255,
                "pet pixel ({x},{y}) was erased"
            );
        }
        for (x, y) in [(3, 5), (4, 5), (3, 6), (4, 6), (2, 6), (5, 6), (2, 5)] {
            assert_eq!(
                alpha_at(&frame, 8, x, y),
                0,
                "shadow pixel ({x},{y}) survived"
            );
        }
    }

    #[test]
    fn shadow_stripping_does_not_erase_a_component_connected_to_the_pet() {
        // A tall blob containing the anchor: the guard must keep everything.
        let mut frame = vec![0u8; 8 * 8 * 4];
        for y in 1..8 {
            for x in 1..7 {
                put(&mut frame, 8, x, y, 255);
            }
        }
        strip_ground_shadow(&mut frame, 8, 8, (4.0, 6.0));
        assert_eq!(alpha_at(&frame, 8, 4, 6), 255);
        assert_eq!(alpha_at(&frame, 8, 4, 2), 255);
    }

    #[test]
    fn shadow_stripping_ignores_empty_and_malformed_frames() {
        let mut empty = vec![0u8; 4 * 4 * 4];
        strip_ground_shadow(&mut empty, 4, 4, (2.0, 2.0));
        assert!(empty.iter().all(|b| *b == 0));

        let mut short = vec![0u8; 3];
        strip_ground_shadow(&mut short, 4, 4, (2.0, 2.0));
        assert_eq!(short, vec![0, 0, 0]);
    }

    #[test]
    fn real_walk_frame_loses_its_ground_shadow_but_keeps_the_pet() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let spec = catalog.walk_spec(1, Direction::Right).unwrap();
        let scale = 2.0;
        let mut renderer = test_renderer();
        renderer.load(&spec, scale).unwrap();
        renderer.render_frame(0.0).unwrap();
        let (w, h) = renderer.size();
        let mut frame = renderer.rgba8().to_vec();
        let anchor = (spec.pet_anchor.0 * scale, spec.pet_anchor.1 * scale);

        let near_anchor = |frame: &[u8]| {
            let mut count = 0;
            for y in 0..h {
                for x in 0..w {
                    let dx = x as f32 - anchor.0;
                    let dy = y as f32 - anchor.1;
                    if dx * dx + dy * dy <= 30.0 * 30.0
                        && alpha_at(frame, w as usize, x as usize, y as usize) > 0
                    {
                        count += 1;
                    }
                }
            }
            count
        };
        let opaque = |frame: &[u8]| frame.chunks_exact(4).filter(|px| px[3] > 0).count();

        let before_near = near_anchor(&frame);
        let before_total = opaque(&frame);
        strip_ground_shadow(&mut frame, w, h, anchor);
        let after_near = near_anchor(&frame);
        let after_total = opaque(&frame);

        assert!(
            before_near > 0,
            "expected a ground shadow around the anchor"
        );
        assert_eq!(after_near, 0, "the ground shadow component must be erased");
        assert!(
            after_total > 100 && after_total < before_total,
            "pet should remain ({before_total} -> {after_total})"
        );
    }
}

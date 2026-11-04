//! ThorVG-backed renderer for the vector Lottie assets.
//!
//! The software canvas (`SwCanvas`) is used on purpose: it keeps the process
//! free of a GPU context and is fast enough for a 25 fps pet that only redraws
//! while an animation is playing.
//!
//! Pixel layout: ThorVG's `ABGR8888` is premultiplied and, on little-endian
//! targets, lays out bytes as `R, G, B, A` — exactly what CoreGraphics wants for
//! a `kCGImageAlphaPremultipliedLast` image.

use std::path::Path;

use nono_core::AnimationSpec;
use thorvg::{ColorSpace, EngineOption, LottieAnimation, Paint, Picture, SwCanvas, Thorvg};

// `tvg_paint_ref` is not exposed by the safe wrapper on purpose (it hands out
// raw refcount control), but it is the documented way to add an *existing*
// picture to a canvas.  We need that because `Paint::duplicate()` is a deep
// copy: frames driven on the original animation would not reach the copy.
// Sharing the animation's own picture is what the C API examples do.
extern "C" {
    fn tvg_paint_ref(paint: *mut std::ffi::c_void) -> u16;
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
    engine: &'static Thorvg,
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
        // The engine is intentionally leaked: canvases and animations borrow it
        // for 'static, and a desktop pet runs for the lifetime of the login
        // session.  This is the simplest way to keep the borrow checker happy
        // without a self-referential struct.
        let engine: &'static Thorvg = Box::leak(Box::new(Thorvg::init()?));
        Ok(Self {
            engine,
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
        })
    }

    /// Recolours the character's white body parts.  The assets are untouched;
    /// `None` restores the original white.
    pub fn set_tint(&mut self, tint: Option<[u8; 3]>) {
        // Pure white means "no tint", exactly like the original assets.
        self.tint = tint.filter(|t| *t != [255, 255, 255]);
    }

    pub fn loaded_id(&self) -> Option<&str> {
        self.loaded_id.as_deref()
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Loads `spec` and rasterises it at `canvas * scale` pixels.
    ///
    /// `scale` must already include the display scale factor (user zoom × dpi).
    pub fn load(&mut self, spec: &AnimationSpec, scale: f32) -> Result<(), RenderError> {
        let width = (spec.canvas.0 * scale).round().max(1.0) as u32;
        let height = (spec.canvas.1 * scale).round().max(1.0) as u32;
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(RenderError::InvalidSize(width, height));
        }

        if self.canvas.is_none() {
            self.canvas = Some(self.engine.sw_canvas(EngineOption::Default)?);
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
                canvas.set_target(
                    &mut self.buffer,
                    width,
                    width,
                    height,
                    ColorSpace::ABGR8888,
                )?;
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

        let mut anim = self.engine.lottie_animation()?;
        if let Some(tint) = self.tint {
            // Recolour the body/ear-root layers in a copy of the Lottie JSON;
            // the file on disk is untouched.
            let bytes = std::fs::read(&spec.file)?;
            let tinted = tint_lottie_json(&bytes, tint, spec.pet_anchor, spec.form)?;
            anim.load_data(&tinted)?;
        } else {
            anim.load_file(
                spec.file
                    .to_str()
                    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 path"))?,
            )?;
        }
        // Per-animation placement correction (see assets/tuning.json).
        // Scaling happens around the pet anchor so the feet stay put.  A start
        // offset can differ from the end offset; it is interpolated per frame.
        let placement = spec.placement;
        let k = placement.scale.max(0.05);
        anim.set_size(width as f32 * k, height as f32 * k)?;
        let start = placement.offset_start.unwrap_or(placement.offset);
        let base_x = spec.pet_anchor.0 * scale * (1.0 - k);
        let base_y = spec.pet_anchor.1 * scale * (1.0 - k);
        anim.picture_mut()
            .translate(base_x + start.0 * scale, base_y + start.1 * scale)?;
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
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;

        self.anim = Some(anim);
        self.loaded_id = Some(spec.id.clone());
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
        let start = self
            .placement
            .offset_start
            .unwrap_or(self.placement.offset);
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
                self.base_px.0 + desired.0 * self.pixel_scale,
                self.base_px.1 + desired.1 * self.pixel_scale,
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
        Ok(())
    }

    /// Re-renders the current frame (used after a window resize).
    pub fn redraw(&mut self) -> Result<(), RenderError> {
        let canvas = self.canvas.as_mut().ok_or(RenderError::NotLoaded)?;
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;
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
                dst[di + c] =
                    (src[si + c] as f32 * alpha + dst[di + c] as f32 * inv).round() as u8;
            }
        }
    }
}

/// True when a colour is a mid-tone body shade (grey/beige), used to identify
/// the character's body layers.  Pure white is not a shade.
fn is_body_shade(c: (f32, f32, f32)) -> bool {
    let (r, g, b) = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if !(0.5..=0.94).contains(&max) || max - min > 0.09 {
        return false;
    }
    b <= r + 0.03
}

/// Colour that may be recoloured inside a body layer: white, neutral grey or
/// warm beige/cream shading.  Greys become a darker shade of the tint; bluish
/// greys, cyan and black stay untouched.
fn is_tintable(c: (f32, f32, f32), blue_ok: bool) -> bool {
    let (r, g, b) = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    max >= 0.5 && max - min <= 0.09 && b <= r + if blue_ok { 0.06 } else { 0.015 }
}

fn opacity_static(layer: &serde_json::Value) -> bool {
    layer
        .get("ks")
        .and_then(|k| k.get("o"))
        .and_then(|o| o.get("a"))
        .and_then(|a| a.as_i64())
        .unwrap_or(0)
        == 0
}

struct LayerColors {
    has_white: bool,
    has_shade: bool,
    has_gray: bool,
    has_saturated: bool,
}

fn layer_colors(node: &serde_json::Value) -> LayerColors {
    let mut out = LayerColors { has_white: false, has_shade: false, has_gray: false, has_saturated: false };
    fn walk(node: &serde_json::Value, out: &mut LayerColors) {
        match node {
            serde_json::Value::Object(map) => {
                if matches!(map.get("ty").and_then(|v| v.as_str()), Some("fl") | Some("st")) {
                    if let Some(c) = map.get("c").and_then(|v| v.as_object()) {
                        if c.get("a").and_then(|v| v.as_i64()).unwrap_or(0) == 0 {
                            if let Some(k) = c.get("k").and_then(|v| v.as_array()) {
                                if k.len() >= 3 {
                                    let r = k[0].as_f64().unwrap_or(0.0) as f32;
                                    let g = k[1].as_f64().unwrap_or(0.0) as f32;
                                    let b = k[2].as_f64().unwrap_or(0.0) as f32;
                                    let max = r.max(g).max(b);
                                    let min = r.min(g).min(b);
                                    if min >= 0.985 {
                                        out.has_white = true;
                                    }
                                    if is_body_shade((r, g, b)) {
                                        out.has_shade = true;
                                    }
                                    if (0.5..=0.95).contains(&max) && max - min <= 0.09 {
                                        out.has_gray = true;
                                    }
                                    if max - min > 0.15 && max > 0.1 {
                                        out.has_saturated = true;
                                    }
                                }
                            }
                        }
                    }
                }
                for v in map.values() {
                    walk(v, out);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    walk(v, out);
                }
            }
            _ => {}
        }
    }
    walk(node, &mut out);
    out
}

/// The white ball and the ear roots contain pure white *and* the body's neutral
/// shading.  White-only layers (hands/fins, ear glows, sweat, bubbles, props)
/// are not body layers.
fn is_body_layer(layer: &serde_json::Value) -> bool {
    let c = layer_colors(layer);
    c.has_white && c.has_shade && !c.has_saturated
}

/// A plain-white body layer: large (both shape bounds > 18 units), no shading
/// and no saturated colours.  This catches parts of the body that are drawn as
/// pure white (e.g. form-3's helmet) without touching the small white flashes on
/// the ears/hands.
fn is_large_white_layer(layer: &serde_json::Value) -> bool {
    let c = layer_colors(layer);
    if !c.has_white || c.has_shade || c.has_saturated {
        return false;
    }
    // Flickering white flashes on the ears/hands animate their opacity; the
    // body/helmet does not.  Only static-opacity layers are body candidates.
    if !opacity_static(layer) {
        return false;
    }
    let Some((w, h)) = shape_bounds(layer) else { return false };
    w > 18.0 && h > 18.0
}

fn shape_bounds(layer: &serde_json::Value) -> Option<(f32, f32)> {
    fn walk(node: &serde_json::Value, b: &mut (f64, f64, f64, f64)) {
        match node {
            serde_json::Value::Object(map) => {
                if map.get("ty").and_then(|v| v.as_str()) == Some("sh") {
                    if let Some(k) = map.get("ks").and_then(|v| v.get("k")) {
                        if let Some(vs) = k.get("v").and_then(|v| v.as_array()) {
                            for p in vs {
                                if let Some(a) = p.as_array() {
                                    if let (Some(x), Some(y)) = (a.first().and_then(|v| v.as_f64()), a.get(1).and_then(|v| v.as_f64())) {
                                        b.0 = b.0.min(x);
                                        b.1 = b.1.min(y);
                                        b.2 = b.2.max(x);
                                        b.3 = b.3.max(y);
                                    }
                                }
                            }
                        }
                    }
                }
                for v in map.values() {
                    walk(v, b);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    walk(v, b);
                }
            }
            _ => {}
        }
    }
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    walk(layer, &mut b);
    if b.0 > b.2 {
        None
    } else {
        Some(((b.2 - b.0) as f32, (b.3 - b.1) as f32))
    }
}

fn layer_position(layer: &serde_json::Value) -> Option<(f32, f32)> {
    let p = layer.get("ks")?.get("p")?;
    let k = p.get("k")?;
    let values = if let Some(arr) = k.as_array() {
        arr
    } else {
        return None;
    };
    let pair = if values.len() == 2 && values.iter().all(|v| v.is_number()) {
        values
    } else if let Some(first) = values.first().and_then(|v| v.as_object()) {
        first.get("s").and_then(|v| v.as_array())?
    } else {
        return None;
    };
    Some((pair.first()?.as_f64()? as f32, pair.get(1)?.as_f64()? as f32))
}

/// Recolours the body and ear-root layers of a Lottie document.
///
/// Eligible layers:
/// - layers that contain pure white plus the body's neutral/beige shading
///   (the ball shell, ear roots, their grey shading -> darker tint);
/// - large plain-white layers near the body centre (parts of the body drawn
///   without shading, e.g. form-3's helmet).
///
/// Small white-only layers (the white flashes on cyan ears/hands, sweat drops,
/// eye highlights, bubbles, props) and saturated layers stay untouched.
fn tint_lottie_json(
    bytes: &[u8],
    tint: [u8; 3],
    anchor: (f32, f32),
    form: u8,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut root: serde_json::Value = serde_json::from_slice(bytes)?;
    let Some(layers) = root.get_mut("layers").and_then(|v| v.as_array_mut()) else {
        return Ok(bytes.to_vec());
    };
    // Form 2 gets a few conservative extra rules (see below); form 1/3 keep the
    // plain body-layer rule.
    let form2 = form == 2;
    let blue_ok = form2;
    let body: Vec<bool> = layers.iter().map(is_body_layer).collect();
    let body_positions: Vec<(f32, f32)> = layers
        .iter()
        .zip(&body)
        .filter(|(_, b)| **b)
        .filter_map(|(l, _)| layer_position(l))
        .collect();
    // Form 2: the body sits just above the feet anchor; this is much more
    // reliable than averaging transform origins (flame/effects have far ones).
    let center = if form2 {
        (anchor.0, anchor.1 - 16.0)
    } else if body_positions.is_empty() {
        (anchor.0, anchor.1 - 35.0)
    } else {
        let n = body_positions.len() as f32;
        (
            body_positions.iter().map(|p| p.0).sum::<f32>() / n,
            body_positions.iter().map(|p| p.1).sum::<f32>() / n,
        )
    };
    let near = |layer: &serde_json::Value, max: f32| -> bool {
        layer_position(layer)
            .map(|(x, y)| ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt() <= max)
            .unwrap_or(false)
    };
    let any_body = body.iter().any(|b| *b);
    for (i, layer) in layers.iter_mut().enumerate() {
        let mut keep = body[i];
        if form2 {
            if !keep {
                let c = layer_colors(layer);
                // Narrow white ring (e.g. the white ring around the body in
                // side/up walk frames) and the normal large plain-white body
                // parts.  Flat fin bars (34x18) are deliberately excluded.
                if c.has_white && !c.has_shade && !c.has_saturated && opacity_static(layer) {
                    if let Some((w, h)) = shape_bounds(layer) {
                        let ring = w < 18.0 && h >= 18.0;
                        let body_part = w >= 18.0 && h > 18.0;
                        keep = (ring || body_part) && near(layer, 32.0);
                    }
                }
                // Grey/blue-grey body details (the two grey ovals on the back of
                // form 2) become a darker shade of the tint.
                if !keep && !c.has_white && !c.has_saturated && c.has_gray && opacity_static(layer) {
                    keep = near(layer, 32.0);
                }
            }
        } else if !keep && is_large_white_layer(layer) {
            keep = near(layer, 30.0);
        }
        if !any_body && !keep && !form2 {
            // No shaded body layer at all (some walk directions): fall back to
            // the large plain-white layers near the body.
            keep = is_large_white_layer(layer) && near(layer, 30.0);
        }
        if !keep {
            continue;
        }
        tint_layer(layer, tint, blue_ok);
    }
    serde_json::to_vec(&root)
}

fn tint_layer(layer: &mut serde_json::Value, tint: [u8; 3], blue_ok: bool) {
    match layer {
        serde_json::Value::Object(map) => {
            if matches!(map.get("ty").and_then(|v| v.as_str()), Some("fl") | Some("st")) {
                if let Some(c) = map.get_mut("c").and_then(|v| v.as_object_mut()) {
                    if c.get("a").and_then(|v| v.as_i64()).unwrap_or(0) == 0 {
                        if let Some(k) = c.get_mut("k").and_then(|v| v.as_array_mut()) {
                            if k.len() >= 3 {
                                let r = k[0].as_f64().unwrap_or(0.0) as f32;
                                let g = k[1].as_f64().unwrap_or(0.0) as f32;
                                let b = k[2].as_f64().unwrap_or(0.0) as f32;
                                if is_tintable((r, g, b), blue_ok) {
                                    let brightness = r.max(g).max(b);
                                    k[0] = serde_json::json!(tint[0] as f32 / 255.0 * brightness);
                                    k[1] = serde_json::json!(tint[1] as f32 / 255.0 * brightness);
                                    k[2] = serde_json::json!(tint[2] as f32 / 255.0 * brightness);
                                }
                            }
                        }
                    }
                }
            }
            for v in map.values_mut() {
                tint_layer(v, tint, blue_ok);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                tint_layer(v, tint, blue_ok);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nono_core::{Catalog, Direction};
    use std::path::PathBuf;

    // ThorVG's global engine state is not safe to initialise concurrently; the
    // application is single-threaded, so tests simply serialise here.
    static THORVG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn assets_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    #[test]
    fn software_canvas_uses_rgba_byte_order() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let engine = Thorvg::init().unwrap();
        let mut canvas = engine.sw_canvas(EngineOption::Default).unwrap();
        let mut buffer = vec![0u32; 8 * 8];
        unsafe { canvas.set_target(&mut buffer, 8, 8, 8, ColorSpace::ABGR8888).unwrap() };
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

        let bytes = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr() as *const u8, buffer.len() * 4)
        };
        assert_eq!(&bytes[0..4], &[255, 0, 0, 255], "expected RGBA memory order");
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
        let mut renderer = Renderer::new().unwrap();
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
        let mut renderer = Renderer::new().unwrap();
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
    fn tint_skips_effect_layers() {
        let root = assets_root();
        let surprise_path = root.join("nono-form1-expressions/nono1_exp_surprise.json");
        if !surprise_path.is_file() {
            return;
        }
        let bytes = std::fs::read(&surprise_path).unwrap();
        let before: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tinted = tint_lottie_json(&bytes, [255, 0, 0], (0.0, 0.0), 1).unwrap();
        let after: serde_json::Value = serde_json::from_slice(&tinted).unwrap();
        // Layer 0 is a white-only sweat drop; layer 45 is the white/grey shell.
        assert_eq!(before["layers"][0], after["layers"][0], "sweat must stay white");
        assert_ne!(before["layers"][45], after["layers"][45], "body must be tinted");

        // The cube's own white faces are not body layers either.
        let cube_path = root.join("nono-form1-actions/nono1_action_cube.json");
        if cube_path.is_file() {
            let bytes = std::fs::read(&cube_path).unwrap();
            let tinted = tint_lottie_json(&bytes, [255, 0, 0], (0.0, 0.0), 1).unwrap();
            let before: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let after: serde_json::Value = serde_json::from_slice(&tinted).unwrap();
            assert_eq!(before["layers"][0], after["layers"][0], "prop layers must stay untouched");
        }
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
        let mut renderer = Renderer::new().unwrap();
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
        let mut renderer = Renderer::new().unwrap();
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
            assert_eq!(alpha_at(&frame, 8, x, y), 255, "pet pixel ({x},{y}) was erased");
        }
        for (x, y) in [(3, 5), (4, 5), (3, 6), (4, 6), (2, 6), (5, 6), (2, 5)] {
            assert_eq!(alpha_at(&frame, 8, x, y), 0, "shadow pixel ({x},{y}) survived");
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
        let mut renderer = Renderer::new().unwrap();
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
                    if dx * dx + dy * dy <= 30.0 * 30.0 && alpha_at(frame, w as usize, x as usize, y as usize) > 0 {
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

        assert!(before_near > 0, "expected a ground shadow around the anchor");
        assert_eq!(after_near, 0, "the ground shadow component must be erased");
        assert!(
            after_total > 100 && after_total < before_total,
            "pet should remain ({before_total} -> {after_total})"
        );
    }
}

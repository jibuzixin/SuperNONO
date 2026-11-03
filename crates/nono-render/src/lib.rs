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
    walk_animation: bool,
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
            walk_animation: false,
            tint: None,
        })
    }

    /// Recolours the character's white body parts.  The assets are untouched;
    /// `None` restores the original white.
    pub fn set_tint(&mut self, tint: Option<[u8; 3]>) {
        // Pure white means "no tint", exactly like the original assets.
        self.tint = tint.filter(|t| *t != [255, 255, 255]);
    }

    /// Tints the character's white body pixels only.
    ///
    /// The body is found as the large white connected regions around the black
    /// visor.  Sweat drops are tiny components, thought bubbles / lightbulbs are
    /// far from the visor, and props such as the cube are not pure white
    /// (their whites are ~0.97), so none of them are touched.  Cyan parts and
    /// grey/beige shading are not near-white and stay as they are.
    fn apply_tint(&mut self) {
        let Some(tint) = self.tint else { return };
        let w = self.width as usize;
        let h = self.height as usize;
        if w == 0 || h == 0 {
            return;
        }
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(
                self.buffer.as_mut_ptr() as *mut u8,
                self.buffer.len() * std::mem::size_of::<u32>(),
            )
        };
        const STRIDE: usize = 4;
        let dw = w.div_ceil(STRIDE);
        let dh = h.div_ceil(STRIDE);
        let mut white = vec![false; dw * dh];
        let mut dark = vec![false; dw * dh];

        // Down-sampled masks: near-white body candidates and dark visor pixels.
        for y in 0..h {
            let row = y * w * 4;
            let cell_row = (y / STRIDE) * dw;
            for x in 0..w {
                let i = row + x * 4;
                let a = bytes[i + 3];
                if a <= 8 {
                    continue;
                }
                let r = bytes[i] as u32;
                let g = bytes[i + 1] as u32;
                let b = bytes[i + 2] as u32;
                let mx = r.max(g).max(b);
                let mn = r.min(g).min(b);
                let cell = cell_row + x / STRIDE;
                if mn * 100 >= a as u32 * 99 && mx - mn <= 10 {
                    white[cell] = true;
                }
                if a > 200 && mx < 60 {
                    dark[cell] = true;
                }
            }
        }

        let (white_labels, white_stats) = label_mask(&white, dw, dh);
        let (_, dark_stats) = label_mask(&dark, dw, dh);
        let visor = dark_stats
            .iter()
            .max_by_key(|s| s.cells)
            .map(|s| (s.sum_x as f32 / s.cells as f32, s.sum_y as f32 / s.cells as f32));

        let scale = self.pixel_scale.max(0.01);
        let mut selected = vec![false; white_stats.len() + 1];
        for (i, stat) in white_stats.iter().enumerate() {
            let area_units =
                stat.cells as f32 * (STRIDE * STRIDE) as f32 / (scale * scale);
            if area_units < 20.0 {
                continue;
            }
            let keep = if self.walk_animation {
                true
            } else if let Some((vx, vy)) = visor {
                let cx = stat.sum_x as f32 / stat.cells as f32;
                let cy = stat.sum_y as f32 / stat.cells as f32;
                let dist = ((cx - vx).powi(2) + (cy - vy).powi(2)).sqrt() * STRIDE as f32 / scale;
                dist <= 45.0
            } else {
                true
            };
            if keep {
                selected[i + 1] = true;
            }
        }

        // Tint only pixels whose connected region was selected.
        for y in 0..h {
            let row = y * w * 4;
            let cell_row = (y / STRIDE) * dw;
            for x in 0..w {
                let i = row + x * 4;
                let a = bytes[i + 3];
                if a <= 8 {
                    continue;
                }
                let r = bytes[i] as u32;
                let g = bytes[i + 1] as u32;
                let b = bytes[i + 2] as u32;
                let mx = r.max(g).max(b);
                let mn = r.min(g).min(b);
                if mn * 100 < a as u32 * 99 || mx - mn > 10 {
                    continue;
                }
                let cell = cell_row + x / STRIDE;
                if !selected[white_labels[cell] as usize] {
                    continue;
                }
                let k = mx;
                bytes[i] = (tint[0] as u32 * k / 255) as u8;
                bytes[i + 1] = (tint[1] as u32 * k / 255) as u8;
                bytes[i + 2] = (tint[2] as u32 * k / 255) as u8;
            }
        }
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
        anim.load_file(
            spec.file
                .to_str()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 path"))?,
        )?;
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
        self.walk_animation = spec.kind == nono_core::AnimationKind::Walk;
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
        self.apply_tint();

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
        self.apply_tint();
        Ok(())
    }

    /// Re-renders the current frame (used after a window resize).
    pub fn redraw(&mut self) -> Result<(), RenderError> {
        let canvas = self.canvas.as_mut().ok_or(RenderError::NotLoaded)?;
        canvas.update()?;
        canvas.draw(true)?;
        canvas.sync()?;
        self.apply_tint();
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



#[derive(Default, Clone, Copy)]
struct RegionStat {
    cells: u32,
    sum_x: u64,
    sum_y: u64,
}

/// 4-connected component labelling on a down-sampled mask.
fn label_mask(mask: &[bool], dw: usize, dh: usize) -> (Vec<u32>, Vec<RegionStat>) {
    let mut labels = vec![0u32; mask.len()];
    let mut stats: Vec<RegionStat> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..mask.len() {
        if !mask[start] || labels[start] != 0 {
            continue;
        }
        let id = (stats.len() + 1) as u32;
        stats.push(RegionStat::default());
        labels[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let x = i % dw;
            let y = i / dw;
            let stat = &mut stats[(id - 1) as usize];
            stat.cells += 1;
            stat.sum_x += x as u64;
            stat.sum_y += y as u64;
            if x > 0 {
                let j = i - 1;
                if mask[j] && labels[j] == 0 { labels[j] = id; stack.push(j); }
            }
            if x + 1 < dw {
                let j = i + 1;
                if mask[j] && labels[j] == 0 { labels[j] = id; stack.push(j); }
            }
            if y > 0 {
                let j = i - dw;
                if mask[j] && labels[j] == 0 { labels[j] = id; stack.push(j); }
            }
            if y + 1 < dh {
                let j = i + dw;
                if mask[j] && labels[j] == 0 { labels[j] = id; stack.push(j); }
            }
        }
    }
    (labels, stats)
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
    fn tint_skips_sweat_drops() {
        let _guard = THORVG_LOCK.lock().unwrap();
        let root = assets_root();
        if !root.join("catalog.json").is_file() {
            return;
        }
        let catalog = Catalog::load(&root).unwrap();
        let spec = catalog.expression_spec(1, "surprise").unwrap();
        let mut renderer = Renderer::new().unwrap();
        renderer.load(&spec, 3.0).unwrap();
        renderer.render_frame(20.0).unwrap();
        let before = renderer.rgba8().to_vec();
        let width = renderer.size().0 as usize;
        // A sweat droplet sits near canvas (11.3, 46.7); sample its tiny area.
        let white_near_sweat = |buf: &[u8]| {
            let mut n = 0;
            for y in 133..147 {
                for x in 27..41 {
                    let i = (y * width + x) * 4;
                    if buf[i] > 220 && buf[i + 1] > 220 && buf[i + 2] > 220 {
                        n += 1;
                    }
                }
            }
            n
        };
        let sweat_before = white_near_sweat(&before);
        assert!(sweat_before > 4, "sweat droplet should be white ({sweat_before})");

        renderer.set_tint(Some([255, 0, 0]));
        renderer.render_frame(20.0).unwrap();
        let after = renderer.rgba8().to_vec();
        assert_eq!(
            white_near_sweat(&after),
            sweat_before,
            "sweat drops must stay white"
        );
        let red = after
            .chunks_exact(4)
            .filter(|p| p[3] > 240 && p[0] > 200 && p[1] < 40 && p[2] < 40)
            .count();
        assert!(red > 100, "the body shell should be tinted, got {red}");
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
}

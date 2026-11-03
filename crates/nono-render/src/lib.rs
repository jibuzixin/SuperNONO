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
        })
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
        // Scaling happens around the pet anchor so the feet stay put.
        let k = spec.placement.scale.max(0.05);
        anim.set_size(width as f32 * k, height as f32 * k)?;
        let origin_x =
            spec.pet_anchor.0 * scale * (1.0 - k) + spec.placement.offset.0 * scale;
        let origin_y =
            spec.pet_anchor.1 * scale * (1.0 - k) + spec.placement.offset.1 * scale;
        anim.picture_mut().translate(origin_x, origin_y)?;
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
        let anim = self.anim.as_mut().ok_or(RenderError::NotLoaded)?;
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

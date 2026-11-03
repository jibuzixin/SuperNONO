//! Tray icon loading (and a procedural fallback so the pet always has an icon).

use std::path::Path;

use tray_icon::Icon;

pub fn load_icon(path: &Path) -> Option<Icon> {
    let image = image::open(path).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

/// A small cyan NONO face, used when no icon file was generated yet.
pub fn fallback_icon(size: u32) -> Icon {
    let size = size.max(16);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    let radius = size as f32 * 0.44;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((y * size + x) * 4) as usize;
            if dist > radius {
                continue;
            }
            let edge = ((radius - dist) / 1.5).clamp(0.0, 1.0);
            let alpha = (edge * 255.0) as u8;
            // Body: NONO cyan; belly: lighter; eyes: near-black.
            let (r, g, b) = if dist < radius * 0.38 && dy > 0.0 {
                (240, 252, 255)
            } else {
                (60, 198, 226)
            };
            let eye = ((dx.abs() - size as f32 * 0.13).abs() < size as f32 * 0.035)
                && (dy + size as f32 * 0.05).abs() < size as f32 * 0.045;
            let (r, g, b) = if eye { (24, 40, 52) } else { (r, g, b) };
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = alpha;
        }
    }
    Icon::from_rgba(rgba, size, size).expect("procedural icon is well formed")
}

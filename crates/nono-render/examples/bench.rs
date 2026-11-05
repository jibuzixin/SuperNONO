//! Times the real render path (load + per-frame) for one animation.
//!
//! Usage: bench <assets dir> <form> <kind> <slug> <scale> <frames>
//! Set `NONO_GLOW_OFF=1` to measure without glow.

use std::time::Instant;

use nono_core::{Catalog, Direction};
use nono_render::Renderer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 7 {
        eprintln!("usage: bench <assets> <form> <kind> <slug> <scale> <frames>");
        std::process::exit(2);
    }
    let assets = std::path::PathBuf::from(&args[1]);
    let form: u8 = args[2].parse()?;
    let kind = args[3].as_str();
    let slug = args[4].as_str();
    let scale: f32 = args[5].parse()?;
    let frames: u32 = args[6].parse()?;

    let catalog = Catalog::load(&assets)?;
    let spec = match kind {
        "walk" => catalog.walk_spec(form, Direction::from_key(slug).ok_or("bad direction")?)?,
        "action" => catalog.action_spec(form, slug)?,
        "expression" => catalog.expression_spec(form, slug)?,
        other => return Err(format!("unknown kind {other}").into()),
    };

    let mut renderer = Renderer::new()?;
    renderer.set_tint_cache_dir(Some(std::env::temp_dir().join("nono-bench")));
    let load_start = Instant::now();
    renderer.load(&spec, scale)?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let passes = renderer.glow_passes();

    // warm up on a few frames, then time a full cycle
    let total = spec.effective_frames().max(1);
    for i in 0..3 {
        renderer.render_frame((i % total) as f32)?;
    }
    let start = Instant::now();
    for i in 0..frames {
        renderer.render_frame((i % total) as f32)?;
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / frames as f64;
    let (w, h) = renderer.size();
    println!(
        "{slug:>10} form{form} {kind:<10} {w}x{h} glow_passes={passes} load={load_ms:.0} ms  frame={ms:.2} ms"
    );
    Ok(())
}

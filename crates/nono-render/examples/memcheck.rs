//! Switches between animations repeatedly and reports RSS, to catch leaks in
//! the load/release path (glow passes included).
//!
//! Usage: memcheck <assets> <rounds>

use std::time::Instant;

use nono_core::{Catalog, Direction};
use nono_render::Renderer;

#[cfg(target_os = "macos")]
fn rss_mb() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct TimeValue {
        seconds: i32,
        microseconds: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct TaskBasicInfo {
        virtual_size: u64,
        resident_size: u64,
        resident_size_max: u64,
        user_time: TimeValue,
        system_time: TimeValue,
        policy: i32,
        suspend_count: i32,
    }
    extern "C" {
        fn mach_task_self() -> u32;
        fn task_info(task: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
    }
    let mut info = TaskBasicInfo::default();
    let mut count = (std::mem::size_of::<TaskBasicInfo>() / std::mem::size_of::<i32>()) as u32;
    let kr = unsafe {
        task_info(
            mach_task_self(),
            20, // MACH_TASK_BASIC_INFO
            &mut info as *mut TaskBasicInfo as *mut i32,
            &mut count,
        )
    };
    if kr == 0 {
        info.resident_size as f64 / (1024.0 * 1024.0)
    } else {
        -1.0
    }
}

#[cfg(not(target_os = "macos"))]
fn rss_mb() -> f64 {
    -1.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: memcheck <assets> <rounds>");
        std::process::exit(2);
    }
    let assets = std::path::PathBuf::from(&args[1]);
    let rounds: u32 = args[2].parse()?;
    let catalog = Catalog::load(&assets)?;

    let mut specs = Vec::new();
    specs.push(catalog.walk_spec(4, Direction::Down)?);
    specs.push(catalog.action_spec(1, "boot")?);
    specs.push(catalog.action_spec(2, "battery")?);
    specs.push(catalog.action_spec(3, "cube")?);
    specs.push(catalog.action_spec(1, "summon")?);

    let mut renderer = Renderer::new()?;
    renderer.set_tint_cache_dir(Some(std::env::temp_dir().join("nono-bench")));
    println!("start rss {:.1} MB", rss_mb());

    for round in 0..rounds {
        for spec in &specs {
            let t = Instant::now();
            renderer.load(spec, 2.0)?;
            renderer.render_frame(3.0)?;
            println!(
                "round {round} {:>20} glow={} load={:.0}ms rss={:.1} MB",
                spec.id,
                renderer.glow_passes(),
                t.elapsed().as_secs_f64() * 1000.0,
                rss_mb()
            );
        }
    }
    println!("end rss {:.1} MB", rss_mb());
    Ok(())
}

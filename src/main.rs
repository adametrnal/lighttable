mod app;
mod branding;
mod cache;
mod demo;
mod grid;
mod removal;
mod sidebar;

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--version" || s == "-V") {
        println!("Lighttable {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "Lighttable 0.1.0 — Linux Lightroom catalog companion\n\nUsage: lighttable [CATALOG.lrcat] [--photo-root DIR] [--data-dir DIR]\n       lighttable --inspect CATALOG.lrcat [--verify-previews]\n       lighttable --benchmark CATALOG.lrcat [COUNT]\n       lighttable CATALOG.lrcat --smoke-test OUTPUT_DIRECTORY\n       lighttable CATALOG.lrcat --scroll-demo NEW_DIRECTORY [--demo-start INDEX] [--demo-collection PARENT/NAME]\n\nThe catalog is read through a private snapshot. Selections are stored separately.\nKeyboard: G/E grid/loupe, arrows, 0–5 stars, P/X/U flags, Ctrl+Z undo, Delete review."
        );
        return Ok(());
    }
    if let Some(i) = args.iter().position(|s| s == "--inspect") {
        return lighttable::catalog::inspect(
            &PathBuf::from(args.get(i + 1).context("Missing catalog path")?),
            args.iter().any(|s| s == "--verify-previews"),
        );
    }
    if let Some(i) = args.iter().position(|s| s == "--benchmark") {
        let catalog = lighttable::catalog::load(
            &PathBuf::from(args.get(i + 1).context("Missing catalog path")?),
            None,
        )?;
        let count = args
            .get(i + 2)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(100_000usize);
        if catalog.photos.is_empty() {
            bail!("Benchmark needs at least one photo");
        }
        let mut photos = Vec::with_capacity(count);
        for i in 0..count {
            let mut p = catalog.photos[i % catalog.photos.len()].clone();
            p.id = i as i64;
            p.rating = (i % 6) as i8;
            p.pick = (i % 3) as i8 - 1;
            photos.push(p);
        }
        let mut times = Vec::new();
        let mut matches = 0;
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let result = lighttable::model::filtered(
                &photos,
                &lighttable::model::Filter {
                    minimum_rating: 3,
                    pick: Some(1),
                    ..Default::default()
                },
            );
            matches = result.len();
            std::hint::black_box(result);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{count} synthetic metadata records; {matches} filter matches; filter p50 {:.2} ms / p95 {:.2} ms. Source load {} ms. This is not a real-catalog image/scroll benchmark.",
            times[15], times[28], catalog.load_ms
        );
        return Ok(());
    }
    let mut path = None;
    let mut root = None;
    let mut data = None;
    let mut smoke = None;
    let mut demo_output = None;
    let mut demo_start = None;
    let mut demo_collection = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--photo-root" | "--data-dir" | "--smoke-test" | "--scroll-demo" => {
                let value = PathBuf::from(args.get(i + 1).context("Option requires a path")?);
                match args[i].as_str() {
                    "--photo-root" => root = Some(value),
                    "--data-dir" => data = Some(value),
                    "--scroll-demo" => demo_output = Some(value),
                    _ => smoke = Some(value),
                }
                i += 2;
            }
            "--demo-start" => {
                demo_start = Some(
                    args.get(i + 1)
                        .context("--demo-start requires a photo position")?
                        .parse()?,
                );
                i += 2;
            }
            "--demo-collection" => {
                demo_collection = Some(
                    args.get(i + 1)
                        .context("--demo-collection requires a collection path")?
                        .clone(),
                );
                i += 2;
            }
            s if !s.starts_with('-') && path.is_none() => {
                path = Some(PathBuf::from(s));
                i += 1;
            }
            s => bail!("Unknown option {s}; run --help"),
        }
    }
    if demo_output.is_some() && (smoke.is_some() || path.is_none()) {
        bail!("--scroll-demo requires a catalog and cannot be combined with --smoke-test");
    }
    if (demo_start.is_some() || demo_collection.is_some()) && demo_output.is_none() {
        bail!("--demo-start and --demo-collection require --scroll-demo");
    }
    let demo = demo_output
        .map(|dir| {
            data = Some(dir.join("state"));
            demo::ScrollDemo::new(dir, demo_start.unwrap_or(0), demo_collection)
        })
        .transpose()?;
    if let Some(dir) = &smoke {
        std::fs::create_dir_all(dir)?;
        data = Some(dir.join("state"));
    }
    let data = data.unwrap_or_else(lighttable::store::data_dir);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 940.0])
            .with_min_inner_size([960.0, 640.0])
            .with_app_id("lighttable-catalog")
            .with_icon(branding::window_icon().context("Could not decode the app icon")?),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Lighttable",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, path, root, data, smoke, demo)))),
    )
    .map_err(|e| anyhow::anyhow!("Could not start the desktop window: {e}"))
}

//! Compares BAE renders with After Effects renders of the same cases
//! (`crates/bae-render/tests/ae/cases.json`).
//!
//! ```text
//! cargo run -p bae-render --example ae_compare -- prepare [ASSETS]      # footage for both sides
//! tools/ae/aectl run crates/bae-render/tests/ae/render_cases.jsx         # AE → /private/tmp/bae-04-ae/ae
//! cargo run -p bae-render --example ae_compare -- compare [AE_DIR] [OUT] # BAE render + diff table
//! ```
//!
//! Both sides output premultiplied 8-bit RGBA (AE's `saveFrameToPng`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{PrimPath, Time};
use bae_render::png_out::{read_rgba8, write_rgba8};
use bae_render::{AnimEvaluator, CompEvaluator, OutputAlpha, RenderRequest, Renderer};

#[path = "../tests/ae/cases.rs"]
mod cases;

use cases::{build, diff, prepare};

const DEFAULT_ASSETS: &str = "/private/tmp/bae-04-ae/assets";
const DEFAULT_AE: &str = "/private/tmp/bae-04-ae/ae";
const DEFAULT_OUT: &str = "/private/tmp/bae-04-ae/bae";

fn compare(ae_dir: &Path, out: &Path, assets: &Path) {
    std::fs::create_dir_all(out).expect("out dir");
    let (builder, names) = build(assets);
    let stage = builder.into_stage();
    let gpu: Arc<GpuContext> =
        GpuContext::new_headless(GpuContextDescriptor::default()).expect("GPU");
    let mut renderer = Renderer::new(gpu);
    let mut eval = AnimEvaluator::new(None);
    println!("| case | max Δ | mean Δ | px Δ>1 | px Δ>3 | worst pixel (x,y): BAE vs AE |");
    println!("|---|---:|---:|---:|---:|---|");
    for (name, _) in names {
        let comp = PrimPath::new(&format!("/comps/{name}")).expect("path");
        let rc = eval.evaluate(&stage, &comp, Time::ZERO).expect("evaluate");
        let frame = renderer
            .render_frame(&rc, &RenderRequest::full())
            .expect("render");
        for w in eval.take_warnings().iter().chain(&frame.warnings) {
            eprintln!("{name}: warning: {w}");
        }
        let img = renderer
            .read_rgba8(&frame, OutputAlpha::Premultiplied)
            .expect("readback");
        let bae_path = out.join(format!("{name}.png"));
        write_rgba8(&bae_path, &img).expect("write");
        let ae_path = ae_dir.join(format!("{name}.png"));
        match read_rgba8(&ae_path) {
            Ok((w, h, ae)) if w == img.width && h == img.height => {
                let d = diff(w, &img.data, &ae);
                println!(
                    "| {name} | {} | {:.3} | {} / {} | {} | ({},{}): {:?} vs {:?} |",
                    d.max,
                    d.mean,
                    d.over1,
                    d.pixels,
                    d.over3,
                    d.worst.0,
                    d.worst.1,
                    d.worst.2,
                    d.worst.3
                );
            }
            Ok((w, h, _)) => println!("| {name} | size mismatch: AE {w}×{h} | | | | |"),
            Err(e) => println!("| {name} | no AE render ({e}) | | | | |"),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize, default: &str| PathBuf::from(args.get(i).map_or(default, String::as_str));
    match args.first().map(String::as_str) {
        Some("prepare") => {
            let dir = arg(1, DEFAULT_ASSETS);
            prepare(&dir);
            println!("assets written to {}", dir.display());
        }
        Some("compare") => compare(
            &arg(1, DEFAULT_AE),
            &arg(2, DEFAULT_OUT),
            &PathBuf::from(DEFAULT_ASSETS),
        ),
        _ => eprintln!("usage: ae_compare prepare [ASSETS] | compare [AE_DIR] [OUT]"),
    }
}

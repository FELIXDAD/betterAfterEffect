//! Golden images from After Effects 26.2.1: every case of `tests/ae/cases.json`
//! rendered by AE (`tests/ae/render_cases.jsx`, `saveFrameToPng`, premultiplied)
//! must be reproduced by BAE within the case's tolerance (default ±1 per channel,
//! i.e. rounding). Regenerate the goldens with the steps in docs/perf.md §3.
//!
//! Software and virtual adapters (CI: WARP on the Windows runner, the
//! paravirtual GPU on macOS runners) filter textures with slightly different
//! precision, so there every tolerance is raised by [`SOFTWARE_SLACK`]; real
//! GPUs keep the strict values. The adapter and the tolerances used are printed.

use std::path::Path;

use bae_gpu::{GpuContext, GpuContextDescriptor, wgpu};
use bae_model::{PrimPath, Time};
use bae_render::png_out::read_rgba8;
use bae_render::{AnimEvaluator, CompEvaluator, OutputAlpha, RenderRequest, Renderer};

#[path = "ae/cases.rs"]
mod cases;

/// Extra per-channel tolerance (8-bit steps) on software / virtual adapters.
const SOFTWARE_SLACK: u8 = 2;

/// Tolerance slack for the adapter `gpu` runs on.
fn adapter_slack(gpu: &GpuContext) -> u8 {
    match gpu.adapter_info().device_type {
        wgpu::DeviceType::Cpu | wgpu::DeviceType::VirtualGpu | wgpu::DeviceType::Other => {
            SOFTWARE_SLACK
        }
        _ => 0,
    }
}

#[test]
fn matches_after_effects() {
    let gpu = match GpuContext::new_headless(GpuContextDescriptor::default()) {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("no GPU, skipping: {e}");
            return;
        }
    };
    let assets = tempfile::tempdir().unwrap();
    cases::prepare(assets.path());
    let (builder, names) = cases::build(assets.path());
    let stage = builder.into_stage();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
    let slack = adapter_slack(&gpu);
    eprintln!(
        "adapter: {} ({:?}); tolerance slack +{slack}",
        gpu.summary(),
        gpu.adapter_info().device_type
    );
    let mut renderer = Renderer::new(gpu);
    let mut eval = AnimEvaluator::new(None);
    let mut failures = Vec::new();
    for (name, case_tolerance) in &names {
        let tolerance = case_tolerance.saturating_add(slack);
        let comp = PrimPath::new(&format!("/comps/{name}")).unwrap();
        let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
        let frame = renderer.render_frame(&rc, &RenderRequest::full()).unwrap();
        let img = renderer
            .read_rgba8(&frame, OutputAlpha::Premultiplied)
            .unwrap();
        let (w, h, ae) = read_rgba8(&golden.join(format!("{name}.png"))).unwrap();
        assert_eq!((w, h), (img.width, img.height), "{name}: size");
        let d = cases::diff(w, &img.data, &ae);
        eprintln!(
            "{name}: max {} mean {:.3} (tolerance {tolerance})",
            d.max, d.mean
        );
        if d.max > tolerance {
            failures.push(format!(
                "{name}: max Δ {} > {tolerance} at ({},{}): BAE {:?} vs AE {:?}",
                d.max, d.worst.0, d.worst.1, d.worst.2, d.worst.3
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

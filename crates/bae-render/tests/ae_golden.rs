//! Golden images from After Effects 26.2.1: every case of `tests/ae/cases.json`
//! rendered by AE (`tests/ae/render_cases.jsx`, `saveFrameToPng`, premultiplied)
//! must be reproduced by BAE within the case's tolerance (default ±1 per channel,
//! i.e. rounding). Regenerate the goldens with the steps in docs/perf.md §3.

use std::path::Path;

use bae_gpu::{GpuContext, GpuContextDescriptor};
use bae_model::{PrimPath, Time};
use bae_render::png_out::read_rgba8;
use bae_render::{AnimEvaluator, CompEvaluator, OutputAlpha, RenderRequest, Renderer};

#[path = "ae/cases.rs"]
mod cases;

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
    let mut renderer = Renderer::new(gpu);
    let mut eval = AnimEvaluator::new(None);
    let mut failures = Vec::new();
    for (name, tolerance) in &names {
        let comp = PrimPath::new(&format!("/comps/{name}")).unwrap();
        let rc = eval.evaluate(&stage, &comp, Time::ZERO).unwrap();
        let frame = renderer.render_frame(&rc, &RenderRequest::full()).unwrap();
        let img = renderer
            .read_rgba8(&frame, OutputAlpha::Premultiplied)
            .unwrap();
        let (w, h, ae) = read_rgba8(&golden.join(format!("{name}.png"))).unwrap();
        assert_eq!((w, h), (img.width, img.height), "{name}: size");
        let d = cases::diff(w, &img.data, &ae);
        eprintln!("{name}: max {} mean {:.3}", d.max, d.mean);
        if d.max > *tolerance {
            failures.push(format!(
                "{name}: max Δ {} > {tolerance} at ({},{}): BAE {:?} vs AE {:?}",
                d.max, d.worst.0, d.worst.1, d.worst.2, d.worst.3
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

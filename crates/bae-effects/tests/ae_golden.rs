//! Built-in effects vs After Effects renders stored in `tests/ae/golden/`
//! (AE 26.2.1, see `tests/ae/README.md`). Each case bounds the max and the mean
//! per-channel difference; software / virtual adapters (CI's WARP) get 2 more
//! levels of max and 0.5 of mean.

#[path = "ae/harness.rs"]
mod harness;

#[test]
fn matches_after_effects() {
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
    let mut cases = harness::cases();
    cases.extend(harness::parse_cases(include_str!(
        "ae/cases-controls-channel.json"
    )));
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let soft = matches!(
        gpu.adapter_info().device_type,
        bae_gpu::wgpu::DeviceType::Cpu
            | bae_gpu::wgpu::DeviceType::VirtualGpu
            | bae_gpu::wgpu::DeviceType::Other
    );
    let (extra_max, extra_mean) = if soft { (2, 0.5) } else { (0, 0.0) };
    let mut failures = Vec::new();
    for case in &cases {
        let ae = harness::read_png(&golden.join(format!("{}.png", case.name)), case.bpc).unwrap();
        let bae = harness::render(&gpu, case);
        let (max, mean, o1, o3) = harness::diff(&bae, &ae);
        eprintln!("{}: max {max} mean {mean:.3} >1: {o1} >3: {o3}", case.name);
        if max > case.tolerance.saturating_add(extra_max) || mean > case.mean + extra_mean {
            failures.push(format!(
                "{} (max {max} / {}, mean {mean:.3} / {})",
                case.name, case.tolerance, case.mean
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "differs from AE: {}",
        failures.join(", ")
    );
}

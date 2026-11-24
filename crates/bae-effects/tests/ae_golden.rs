//! Built-in effects vs After Effects renders stored in `tests/ae/golden/`
//! (AE 26.2.1, see `tests/ae/README.md`). Each case bounds the max and the mean
//! per-channel difference; software / virtual adapters (CI's WARP) get 2 more
//! levels of max and 0.5 of mean.

#[path = "ae/harness.rs"]
mod harness;

use std::io::Write;

#[test]
fn matches_after_effects() {
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
    let mut cases = harness::cases();
    cases.extend(harness::parse_cases(include_str!(
        "ae/cases-controls-channel.json"
    )));
    cases.extend(harness::parse_cases(include_str!(
        "ae/cases-keying-matte.json"
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
    let ci = std::env::var_os("CI").is_some();
    for case in &cases {
        // libtest buffers eprintln until the test returns. A driver failure
        // can terminate the process before that output reaches the CI log.
        if ci {
            writeln!(std::io::stderr().lock(), "Rendering {}", case.name).unwrap();
        }
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

// A different noise sequence can pass loose pixel thresholds with a flat or
// clipped image. Check the amplitude independently of the spatial realization.
#[test]
fn noise_amplitude_matches_ae() {
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
    for case in harness::cases() {
        let is_grain = case.effect == "VISINF Grain Implant";
        let is_noise = case.effect == "ADBE Noise2";
        let block = case.effect == "ADBE Block Dissolve";
        let lattice = case.name.starts_with("fractal_") && case.name.ends_with("_zoom");
        if !(is_grain || is_noise || block || lattice) {
            continue;
        }
        let ae = harness::read_png(&dir.join(format!("{}.png", case.name)), case.bpc).unwrap();
        let bae = harness::render(&gpu, &case);
        let input = harness::input(&case.input);
        let channels = if block {
            3..4
        } else if lattice {
            0..1
        } else {
            0..3
        };
        for channel in channels {
            let moments = |pixels: &[[u8; 4]]| {
                let (mut n, mut sum, mut squares) = (0.0, 0.0, 0.0);
                for (i, (p, src)) in pixels.iter().zip(&input).enumerate() {
                    let mut v = f64::from(p[channel]);
                    if is_grain || is_noise {
                        // Sample midtones to avoid the integer buffer's clipping.
                        if src[3] != 255 || !(56..190).contains(&src[channel]) {
                            continue;
                        }
                        if case.name == "grain_preview"
                            && !(36..220).contains(&(i % harness::SIZE[0] as usize))
                        {
                            continue;
                        }
                        v -= f64::from(src[channel]);
                    }
                    n += 1.0;
                    sum += v;
                    squares += v * v;
                }
                let mean = sum / n;
                (mean, (squares / n - mean * mean).max(0.0).sqrt())
            };
            let (mean, sd) = moments(&bae);
            let (target, contrast) = moments(&ae);
            let mean_tol = if block || lattice { 5.0 } else { 1.0 };
            assert!(
                (mean - target).abs() < mean_tol && (sd - contrast).abs() < 1.0 + 0.08 * contrast,
                "{} channel {channel}: mean/stddev {mean:.2}/{sd:.2}, AE {target:.2}/{contrast:.2}",
                case.name
            );
        }
    }
}

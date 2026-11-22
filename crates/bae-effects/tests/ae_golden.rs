//! Built-in effects vs After Effects renders stored in `tests/ae/golden/`
//! (produced on a machine with AE, see `tests/ae/README.md`). Cases without a
//! golden image are skipped, so the test passes until the renders exist.

#[path = "ae/harness.rs"]
mod harness;

#[test]
fn matches_after_effects() {
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
    let cases: Vec<_> = harness::cases()
        .into_iter()
        .filter(|c| golden.join(format!("{}.png", c.name)).exists())
        .collect();
    if cases.is_empty() {
        eprintln!("no AE golden images yet ({}): skipped", golden.display());
        return;
    }
    let Some(gpu) = bae_gpu::testing::shared_context() else {
        return;
    };
    let mut failures = Vec::new();
    for case in &cases {
        let ae = harness::read_png(&golden.join(format!("{}.png", case.name))).unwrap();
        let bae = harness::render(&gpu, case);
        let (max, mean, o1, o3) = harness::diff(&bae, &ae);
        eprintln!("{}: max {max} mean {mean:.3} >1: {o1} >3: {o3}", case.name);
        if max > case.tolerance {
            failures.push(format!("{} (max {max} > {})", case.name, case.tolerance));
        }
    }
    assert!(
        failures.is_empty(),
        "differs from AE: {}",
        failures.join(", ")
    );
}

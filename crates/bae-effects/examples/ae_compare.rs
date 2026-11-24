//! Compares the built-in effects with After Effects renders.
//!
//! ```sh
//! cargo run -p bae-effects --example ae_compare -- prepare   # inputs + cases → /private/tmp/bae-07-ae
//! tools/ae/aectl lock -- tools/ae/aectl run crates/bae-effects/tests/ae/render_cases.jsx   # on a machine with AE
//! cargo run -p bae-effects --example ae_compare -- compare   # BAE renders + table
//! cargo run -p bae-effects --example ae_compare -- golden    # copy AE outputs into tests/ae/golden/
//! ```

#[path = "../tests/ae/harness.rs"]
mod harness;

use std::path::{Path, PathBuf};

const ROOT: &str = "/private/tmp/bae-07-ae";

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "compare".into());
    // BAE_07_ROOT isolates concurrent runs. BAE_07_CASES selects a case list for prepare.
    let root = std::env::var_os("BAE_07_ROOT").map_or_else(|| PathBuf::from(ROOT), PathBuf::from);
    match cmd.as_str() {
        "prepare" => {
            std::fs::create_dir_all(root.join("inputs")).unwrap();
            for name in [
                "chart",
                "soft",
                "step",
                "ramp",
                "line",
                "lineg",
                "fine",
                "coords",
                "color-cast",
                "rgb-cube",
                "neutral-cast",
                "neutral-flip",
                "neutral-cool",
                "neutral-warm",
                "neutral-patch",
                "speckles",
                "matte-edge",
            ] {
                harness::write_png(
                    &root.join("inputs").join(format!("{name}.png")),
                    &harness::input(name),
                );
            }
            let cases = std::env::var_os("BAE_07_CASES").map_or_else(
                || harness::CASES.to_owned(),
                |path| std::fs::read_to_string(path).unwrap(),
            );
            std::fs::write(root.join("cases.json"), cases).unwrap();
            println!("wrote {}/inputs and cases.json", root.display());
        }
        "compare" => compare(&root),
        "golden" => {
            let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ae/golden");
            std::fs::create_dir_all(&golden).unwrap();
            for case in load_cases(&root) {
                let src = root.join("ae").join(format!("{}.png", case.name));
                if src.exists() {
                    std::fs::copy(&src, golden.join(format!("{}.png", case.name))).unwrap();
                }
            }
            println!("copied AE outputs into {}", golden.display());
        }
        other => eprintln!("unknown command {other} (prepare | compare | golden)"),
    }
}

fn compare(root: &Path) {
    let gpu =
        bae_gpu::GpuContext::new_headless(bae_gpu::GpuContextDescriptor::default().with_env())
            .unwrap();
    std::fs::create_dir_all(root.join("bae")).unwrap();
    println!("| case | max Δ | mean Δ | px Δ>1 | px Δ>3 |");
    println!("|---|---:|---:|---:|---:|");
    for case in load_cases(root) {
        let bae = harness::render(&gpu, &case);
        harness::write_png(&root.join("bae").join(format!("{}.png", case.name)), &bae);
        match harness::read_png(
            &root.join("ae").join(format!("{}.png", case.name)),
            case.bpc,
        ) {
            Some(ae) => {
                let (max, mean, o1, o3) = harness::diff(&bae, &ae);
                println!(
                    "| {} | {max} | {mean:.3} | {o1} / {} | {o3} |",
                    case.name,
                    ae.len()
                );
            }
            None => println!("| {} | (no AE render) | | | |", case.name),
        }
    }
}

fn load_cases(root: &Path) -> Vec<harness::Case> {
    match std::fs::read_to_string(root.join("cases.json")) {
        Ok(text) if std::env::var_os("BAE_07_ROOT").is_some() => harness::parse_cases(&text),
        _ => harness::cases(),
    }
}

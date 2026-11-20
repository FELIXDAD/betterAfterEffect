//! bae-color's OCIO subset against OpenColorIO 2.6 itself
//! (tools/gen_ocio_vectors.py): the ACES configs shipped with OCIO and a
//! hand-written config that uses every supported transform and LUT format.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bae_color::ColorError;
use bae_color::ocio::Config;
use serde_json::Value;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/ocio")
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

struct Report {
    compared: usize,
    worst: Vec<(f64, String)>,
    skipped: BTreeMap<String, usize>,
}

fn run(config: &str, tol: f64) -> Report {
    let text = std::fs::read_to_string(data_dir().join("vectors.json")).unwrap();
    let data: Value = serde_json::from_str(&text).unwrap();
    let cfg = Config::load(&data_dir().join(config)).unwrap_or_else(|e| panic!("{config}: {e}"));
    let mut report = Report {
        compared: 0,
        worst: Vec::new(),
        skipped: BTreeMap::new(),
    };
    for case in data["configs"][config].as_array().unwrap() {
        let (label, result) = if case["kind"] == "cs" {
            let (s, d) = (case["src"].as_str().unwrap(), case["dst"].as_str().unwrap());
            (format!("{s} -> {d}"), cfg.processor(s, d))
        } else {
            let (s, d, v) = (
                case["src"].as_str().unwrap(),
                case["display"].as_str().unwrap(),
                case["view"].as_str().unwrap(),
            );
            (format!("{s} -> {d} / {v}"), cfg.display_view(s, d, v))
        };
        let t = match result {
            Ok(t) => t,
            Err(ColorError::Unsupported(what)) => {
                *report.skipped.entry(what).or_default() += 1;
                continue;
            }
            Err(ColorError::NotInvertible(what)) => {
                *report.skipped.entry(format!("inverse {what}")).or_default() += 1;
                continue;
            }
            Err(e) => panic!("{config}: {label}: {e}"),
        };
        // Display and log code values only exist in [0, 1] (OCIO bakes some
        // log curves into LUTs that clamp outside it).
        let src_display = cfg
            .space(case["src"].as_str().unwrap())
            .is_ok_and(|s| s.encoding.ends_with("-video") || s.encoding == "log");
        let mut worst = 0.0f64;
        for (i, o) in case["in"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["out"].as_array().unwrap())
        {
            let (i, o) = (floats(i), floats(o));
            if src_display && i.iter().any(|v| *v > 1.0) {
                continue;
            }
            let got = t.apply_f64([i[0], i[1], i[2]]);
            for c in 0..3 {
                let err = (got[c] - o[c]).abs() / o[c].abs().max(1.0);
                assert!(
                    err <= tol,
                    "{config}: {label}: in {i:?} got {got:?} expected {o:?} (err {err:e})"
                );
                worst = worst.max(err);
            }
        }
        report.compared += 1;
        report.worst.push((worst, label));
    }
    report.worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    eprintln!("{config}: {} conversions compared, worst:", report.compared);
    for (e, l) in report.worst.iter().take(5) {
        eprintln!("  {e:.2e}  {l}");
    }
    for (what, n) in &report.skipped {
        eprintln!("  skipped {n}× unsupported: {what}");
    }
    report
}

#[test]
fn handmade_config_matches_ocio() {
    let r = run("handmade.ocio", 1e-4);
    // Inverse 3D LUTs are the only gap (OCIO inverts them iteratively).
    assert!(
        r.skipped.keys().all(|k| k == "inverse 3D LUT"),
        "{:?}",
        r.skipped
    );
    assert!(r.compared >= 45, "{}", r.compared);
}

#[test]
fn aces_studio_config_matches_ocio() {
    let r = run("studio-config-v5.0.0_aces-v2.1_ocio-v2.6.ocio", 1e-4);
    assert!(r.compared >= 120, "{}", r.compared);
    // Only the documented gaps may be skipped.
    for what in r.skipped.keys() {
        assert!(
            what.contains("ACES-OUTPUT")
                || what.contains("CLOG2")
                || what.contains("APPLE_LOG")
                || what.contains("ADX")
                || what.contains("HLG-1000nit")
                || what.contains("ACES-LMT")
                || what.contains("NamedTransform"),
            "unexpected unsupported item: {what}"
        );
    }
}

#[test]
fn aces_cg_config_v13_matches_ocio() {
    let r = run("cg-config-v2.2.0_aces-v1.3_ocio-v2.4.ocio", 1e-4);
    assert!(r.compared >= 40, "{}", r.compared);
}

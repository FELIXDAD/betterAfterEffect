//! Prints bae-color's results for the AE comparison chart as JSON
//! (`tools/ae/compare.py` reads it next to AE's renders).
//!
//! ```text
//! cargo run -p bae-color --example ae_compare > /private/tmp/bae-color-ae/bae.json
//! ```

use bae_color::ocio::Config;
use bae_color::{ColorSpace, ColorTransform, builtin};

const W: usize = 32;
const H: usize = 8;
const AE_OCIO: &str = "/Applications/Adobe After Effects 2026/Adobe After Effects 2026.app/Contents/Resources/OpenColorIO-Configs";

/// Mirrors `chart_value` in make_charts.py (values quantized to 16 bits).
fn chart() -> Vec<[f64; 3]> {
    let q = |v: f64| (v * 65535.0).round() / 65535.0;
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let t = x as f64 / (W - 1) as f64;
            let c = match y {
                0 => [t, 0.2, 0.1],
                1 => [0.1, t, 0.3],
                2 => [0.3, 0.1, t],
                3 => [t, t, t],
                _ => {
                    let k = ((y - 4) * W + x) as f64;
                    [
                        ((k * 37.0) % 101.0) / 100.0,
                        ((k * 61.0) % 103.0) / 102.0,
                        ((k * 89.0) % 107.0) / 106.0,
                    ]
                }
            };
            px.push(c.map(q));
        }
    }
    px
}

fn run(t: &ColorTransform) -> Vec<[f64; 3]> {
    chart().into_iter().map(|c| t.apply_f64(c)).collect()
}

fn conv(a: &ColorSpace, b: &ColorSpace) -> ColorTransform {
    a.transform_to(b).expect("conversion")
}

fn main() {
    let mut models: Vec<(String, Vec<[f64; 3]>)> = Vec::new();
    let b = |id: &str| builtin::by_id(id).expect(id);
    for (src, dst) in [
        ("srgb", "srgb"),
        ("display-p3", "srgb"),
        ("rec709", "srgb"),
        ("rec709-gamma24", "srgb"),
        ("rec2020", "srgb"),
        ("rec2020-gamma24", "srgb"),
        ("srgb", "srgb-linear"),
        ("srgb", "rec709-gamma24"),
        ("srgb", "acescg"),
        ("srgb", "display-p3"),
        ("srgb", "rec2100-pq"),
        ("rec2100-pq", "srgb"),
    ] {
        models.push((format!("{src}->{dst}"), run(&conv(&b(src), &b(dst)))));
    }
    // PQ with SDR white at 203 cd/m² (BT.2408): scale linear by 2.03.
    let scale = |s: f64| ColorTransform::matrix(bae_color::Affine::scale([s; 3]));
    let srgb_lin = b("srgb-linear");
    let pq = b("rec2100-pq");
    let to_pq203 = conv(&b("srgb"), &b("rec2020-linear"))
        .then(&scale(2.03))
        .then(&conv(&b("rec2020-linear"), &pq));
    models.push(("srgb->rec2100-pq(w203)".into(), run(&to_pq203)));
    let from_pq203 = conv(&pq, &b("rec2020-linear"))
        .then(&scale(1.0 / 2.03))
        .then(&conv(&b("rec2020-linear"), &b("srgb")));
    models.push(("rec2100-pq(w203)->srgb".into(), run(&from_pq203)));
    let _ = srgb_lin;

    // OCIO: AE's own ACES 1.3 CG config.
    let path = format!("{AE_OCIO}/ACES 1.3 CG v1.0/cg-config-v1.0.0_aces-v1.3_ocio-v2.1.ocio");
    match Config::load(std::path::Path::new(&path)) {
        Ok(cfg) => {
            let rule = cfg
                .colorspace_for_path("chart.png")
                .unwrap_or("")
                .to_string();
            for src in ["sRGB - Texture", rule.as_str()] {
                if let Ok(t) = cfg.processor(src, "ACEScg") {
                    models.push((format!("ocio-cg13:{src}->ACEScg"), run(&t)));
                }
            }
            eprintln!("ocio cg13 default rule for png: {rule}");
        }
        Err(e) => eprintln!("AE OCIO config not readable: {e}"),
    }

    let input = chart();
    let mut out = String::from("{\n\"input\": ");
    out.push_str(&serde_json::to_string(&input).unwrap());
    for (name, px) in &models {
        out.push_str(&format!(
            ",\n{}: {}",
            serde_json::to_string(name).unwrap(),
            serde_json::to_string(px).unwrap()
        ));
    }
    out.push_str("\n}\n");
    print!("{out}");
}

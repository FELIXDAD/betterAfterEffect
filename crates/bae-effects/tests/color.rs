//! Colour-correction effects on the GPU vs CPU references of the AE formulas
//! (docs/spec/ae/_parts/effects--fx-a.md AE-FX-036…043), with the constants
//! fitted to AE 26 renders (`tests/ae/`). The references are 32-bpc (no
//! clamping); `eight_bpc_clamps` checks the 8 / 16-bpc behaviour.

mod common;

use bae_model::Value;
use bae_render::IRect;
use common::*;

const RECT: IRect = IRect::new(-3, 2, 61, 45);
/// RGBA32F tolerance (shader f32 vs CPU f64, pow/exp/sin differences).
const TOL32: f32 = 2e-4;
/// RGBA16F tolerance (half precision storage of values up to ~2.5).
const TOL16: f32 = 4e-3;

/// Rec.601 weights (Tint and Tritone in AE 26).
fn luma(c: [f64; 3]) -> f64 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Applies `f` to the straight colour of every pixel and premultiplies again
/// with the alpha `f` returns.
fn straight(input: &Pixels, f: impl Fn([f64; 4]) -> [f64; 4]) -> Pixels {
    input.map(|c| {
        let s = unpremultiply(c).map(f64::from);
        let o = f(s);
        let a = o[3] as f32;
        [o[0] as f32 * a, o[1] as f32 * a, o[2] as f32 * a, a]
    })
}

/// Checks the effect against `reference` in RGBA32F and RGBA16F.
fn check(match_name: &str, params: Vec<(u32, Value)>, reference: impl Fn([f64; 4]) -> [f64; 4]) {
    check_tol(match_name, params, TOL16, reference);
}

/// [`check`] with an RGBA16F tolerance of `tol16`.
fn check_tol(
    match_name: &str,
    params: Vec<(u32, Value)>,
    tol16: f32,
    reference: impl Fn([f64; 4]) -> [f64; 4],
) {
    let Some(gpu) = context() else { return };
    let fx = builtin(match_name);
    let input = test_image(RECT);
    let want = straight(&input, &reference);
    let soft = tolerance_scale(&gpu);
    for (format, tol) in [(F32, TOL32 * soft), (F16, tol16 * soft)] {
        let got = run(&gpu, format, fx, params.clone(), &input, RECT, 1.0);
        // Tolerance relative to the magnitude (HDR values in half precision).
        let (d, (x, y), k) = max_diff(&got, &want, RECT);
        let scale = want.get(x, y)[k].abs().max(1.0);
        assert!(
            d <= tol * scale,
            "{match_name} {format:?} {params:?}: diff {d} at ({x}, {y}) ch {k}: got {:?}, want {:?}",
            got.get(x, y),
            want.get(x, y)
        );
    }
}

#[test]
fn tint() {
    let black = [0.1, 0.0, 0.3, 1.0];
    let white = [1.0, 0.9, 0.2, 1.0];
    for amount in [100.0, 35.0] {
        check(
            "ADBE Tint",
            vec![
                (1, color(black)),
                (2, color(white)),
                (3, Value::Float(amount)),
            ],
            |s| {
                let l = luma([s[0], s[1], s[2]]);
                let t = amount / 100.0;
                [
                    lerp(s[0], black[0] + (white[0] - black[0]) * l, t),
                    lerp(s[1], black[1] + (white[1] - black[1]) * l, t),
                    lerp(s[2], black[2] + (white[2] - black[2]) * l, t),
                    s[3],
                ]
            },
        );
    }
    // Defaults: black → white ramp at 100 % = luminance greyscale.
    check("ADBE Tint", vec![], |s| {
        let l = luma([s[0], s[1], s[2]]);
        [l, l, l, s[3]]
    });
}

#[test]
fn tritone() {
    let (hi, mid, sh) = ([1.0, 0.95, 0.8], [0.6, 0.3, 0.2], [0.05, 0.0, 0.1]);
    for blend in [0.0, 40.0] {
        check(
            "ADBE Tritone",
            vec![
                (1, color([hi[0], hi[1], hi[2], 1.0])),
                (2, color([mid[0], mid[1], mid[2], 1.0])),
                (3, color([sh[0], sh[1], sh[2], 1.0])),
                (4, Value::Float(blend)),
            ],
            |s| {
                let l = luma([s[0], s[1], s[2]]);
                let ramp: [f64; 3] = std::array::from_fn(|k| {
                    if l <= 0.5 {
                        lerp(sh[k], mid[k], l * 2.0)
                    } else {
                        lerp(mid[k], hi[k], (l - 0.5) * 2.0)
                    }
                });
                let b = blend / 100.0;
                [
                    lerp(ramp[0], s[0], b),
                    lerp(ramp[1], s[1], b),
                    lerp(ramp[2], s[2], b),
                    s[3],
                ]
            },
        );
    }
}

/// AE-FX-038 map() in a 32-bpc project; clip popups 1 = On (3 = off for
/// 32 bpc). Below input black the curve is mirrored (AE 26).
fn levels_map(x: f64, v: [f64; 5], clip: [i64; 2]) -> f64 {
    let [ib, iw, g, ob, ow] = v;
    let mut t = if iw == ib {
        if x >= ib { 1.0 } else { 0.0 }
    } else {
        (x - ib) / (iw - ib)
    };
    t = t.signum() * t.abs().powf(1.0 / g);
    let mut y = ob + (ow - ob) * t;
    if clip[0] == 1 {
        y = y.max(ob.min(ow));
    }
    if clip[1] == 1 {
        y = y.min(ob.max(ow));
    }
    y
}

#[test]
fn levels() {
    let v = [0.1, 0.8, 1.7, 0.05, 0.95];
    for (channel, clip) in [
        (1, [3, 3]),
        (1, [1, 1]),
        (2, [2, 2]),
        (4, [3, 3]),
        (5, [3, 3]),
    ] {
        let params = vec![
            (1, Value::Int(channel)),
            (3, Value::Float(v[0])),
            (4, Value::Float(v[1])),
            (5, Value::Float(v[2])),
            (6, Value::Float(v[3])),
            (7, Value::Float(v[4])),
            (8, Value::Int(clip[0])),
            (9, Value::Int(clip[1])),
        ];
        check("ADBE Easy Levels2", params, |s| {
            let m = |x| levels_map(x, v, clip);
            match channel {
                1 => [m(s[0]), m(s[1]), m(s[2]), s[3]],
                2 => [m(s[0]), s[1], s[2], s[3]],
                3 => [s[0], m(s[1]), s[2], s[3]],
                4 => [s[0], s[1], m(s[2]), s[3]],
                _ => {
                    let a = m(s[3]).clamp(0.0, 1.0);
                    if s[3] <= 0.0 {
                        [0.0, 0.0, 0.0, a]
                    } else {
                        [s[0], s[1], s[2], a]
                    }
                }
            }
        });
    }
}

#[test]
fn pro_levels() {
    let rgb = [0.0, 0.9, 1.2, 0.0, 1.0];
    let red = [0.1, 1.0, 0.8, 0.0, 1.0];
    let green = [0.0, 1.0, 1.0, 0.1, 0.9];
    let blue = [0.2, 0.7, 1.0, 0.0, 1.0];
    let alpha = [0.0, 0.8, 1.0, 0.0, 1.0];
    let mut params = vec![(38, Value::Int(3)), (39, Value::Int(3))];
    for (first, v) in [(4, rgb), (11, red), (18, green), (25, blue), (32, alpha)] {
        for (k, x) in v.iter().enumerate() {
            params.push((first + k as u32, Value::Float(*x)));
        }
    }
    check("ADBE Pro Levels2", params, |s| {
        let m = |x, v| levels_map(x, v, [3, 3]);
        let a = m(s[3], alpha).clamp(0.0, 1.0);
        let c = [
            m(m(s[0], red), rgb),
            m(m(s[1], green), rgb),
            m(m(s[2], blue), rgb),
        ];
        if s[3] <= 0.0 {
            // Fully transparent input: straight colour is 0 before the levels.
            let z = [
                m(m(0.0, red), rgb),
                m(m(0.0, green), rgb),
                m(m(0.0, blue), rgb),
            ];
            return [z[0], z[1], z[2], a];
        }
        [c[0], c[1], c[2], a]
    });
}

fn exposure_ch(x: f64, e: f64, o: f64, g: f64) -> f64 {
    let v = x * 2f64.powf(e) + o;
    v.signum() * v.abs().powf(1.0 / g)
}

/// AE 26 linearizes Exposure with a pure 2.4 power (sign kept).
fn pow_signed(x: f64, p: f64) -> f64 {
    x.signum() * x.abs().powf(p)
}

#[test]
fn exposure() {
    for bypass in [false, true] {
        let (e, o, g) = (0.7, -0.02, 1.3);
        // The 2.4 power curve is infinitely steep at 0: half-precision inputs
        // near 0 (negative after the offset) move the result more.
        check_tol(
            "ADBE Exposure2",
            vec![
                (3, Value::Float(e)),
                (4, Value::Float(o)),
                (5, Value::Float(g)),
                (22, Value::Bool(bypass)),
            ],
            1e-2,
            |s| {
                let f = |x: f64| {
                    if bypass {
                        exposure_ch(x, e, o, g)
                    } else {
                        pow_signed(exposure_ch(pow_signed(x, 2.4), e, o, g), 1.0 / 2.4)
                    }
                };
                [f(s[0]), f(s[1]), f(s[2]), s[3]]
            },
        );
    }
    // Individual channels.
    let per = [(-1.0, 0.0, 1.0), (0.5, 0.01, 0.9), (0.0, 0.0, 2.0)];
    let mut params = vec![(1, Value::Int(2)), (22, Value::Bool(true))];
    for (k, (e, o, g)) in per.iter().enumerate() {
        let first = 8 + 5 * k as u32;
        params.push((first, Value::Float(*e)));
        params.push((first + 1, Value::Float(*o)));
        params.push((first + 2, Value::Float(*g)));
    }
    check("ADBE Exposure2", params, |s| {
        let f = |k: usize| exposure_ch(s[k], per[k].0, per[k].1, per[k].2);
        [f(0), f(1), f(2), s[3]]
    });
}

fn bc_modern(x: f64, b: f64, c: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let x = 1.0 - (1.0 - x).powf(2f64.powf(1.8 * b));
    let x = x + (-c / (3.0 * std::f64::consts::PI)) * (2.0 * std::f64::consts::PI * x).sin();
    x.clamp(0.0, 1.0)
}

#[test]
fn brightness_contrast() {
    for (br, ct) in [(40.0, 30.0), (-80.0, -70.0), (150.0, 100.0)] {
        check(
            "ADBE Brightness & Contrast 2",
            vec![(1, Value::Float(br)), (2, Value::Float(ct))],
            |s| {
                let b = br.clamp(-150.0, 150.0) / 150.0;
                let c = ct.clamp(-50.0, 100.0) / 100.0;
                [
                    bc_modern(s[0], b, c),
                    bc_modern(s[1], b, c),
                    bc_modern(s[2], b, c),
                    s[3],
                ]
            },
        );
    }
    for (br, ct) in [(30.0, 40.0), (-20.0, -50.0)] {
        check(
            "ADBE Brightness & Contrast 2",
            vec![
                (1, Value::Float(br)),
                (2, Value::Float(ct)),
                (3, Value::Bool(true)),
            ],
            |s| {
                let b = br.clamp(-100.0, 100.0) / 255.0;
                let c = ct.clamp(-100.0, 100.0) / 100.0;
                let gain = if c > 0.0 {
                    1.0 / (1.0 - c).max(1e-4)
                } else {
                    1.0 + c
                };
                let f = |x: f64| gain * x + 0.5 * (1.0 - gain) + b * gain.max(1.0);
                [f(s[0]), f(s[1]), f(s[2]), s[3]]
            },
        );
    }
}

fn rgb_to_hsl(c: [f64; 3]) -> [f64; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let l = 0.5 * (mx + mn);
    let d = mx - mn;
    if d <= 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    [h / 6.0, s, l]
}

fn hsl_to_rgb(hsl: [f64; 3]) -> [f64; 3] {
    let h = hsl[0].rem_euclid(1.0);
    let ch = (1.0 - (2.0 * hsl[2] - 1.0).abs()) * hsl[1];
    let hue = [
        ((h * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0),
        (2.0 - (h * 6.0 - 2.0).abs()).clamp(0.0, 1.0),
        (2.0 - (h * 6.0 - 4.0).abs()).clamp(0.0, 1.0),
    ];
    hue.map(|v| (v - 0.5) * ch + hsl[2])
}

fn hs_lightness(c: [f64; 3], amount: f64) -> [f64; 3] {
    let l = amount / 100.0;
    c.map(|v| {
        if l < 0.0 {
            v * (1.0 + l)
        } else {
            v * (1.0 - l) + l
        }
    })
}

#[test]
fn hue_saturation() {
    for (hue, sat, light, range) in [
        (40.0, 30.0, 10.0, 1),
        (-100.0, -60.0, -20.0, 1),
        (180.0, 0.0, 0.0, 1),
        (30.0, 50.0, 0.0, 2),
    ] {
        check(
            "ADBE HUE SATURATION",
            vec![
                (2, Value::Int(range)),
                (4, Value::Float(hue)),
                (5, Value::Float(sat)),
                (6, Value::Float(light)),
            ],
            |s| {
                let orig = [s[0], s[1], s[2]];
                let mut c = hs_lightness(orig, light);
                let k = sat / 100.0;
                if k != 0.0 {
                    let mn = c[0].min(c[1]).min(c[2]);
                    let mx = c[0].max(c[1]).max(c[2]);
                    let mid = 0.5 * (mn + mx);
                    let ch = (mx - mn).max(1e-4);
                    let scale = if k > 0.0 {
                        (1.0 / (1.0 - k.min(0.999)))
                            .min(126.0)
                            .min(mid.min(1.0 - mid).max(0.0) / ch * 2.0)
                            .max(1.0)
                    } else {
                        1.0 + k
                    };
                    c = c.map(|v| mid + (v - mid) * scale);
                }
                if hue != 0.0 {
                    // HSL rotation keeps min and max (defined for HDR too).
                    let mn = c[0].min(c[1]).min(c[2]);
                    let mx = c[0].max(c[1]).max(c[2]);
                    if mx - mn > 1e-6 {
                        let h = (rgb_to_hsl(c)[0] + hue / 360.0).rem_euclid(1.0);
                        let pure = hsl_to_rgb([h, 1.0, 0.5]);
                        c = pure.map(|v| mn + (mx - mn) * v);
                    }
                }
                if range >= 2 {
                    let h = rgb_to_hsl(orig)[0] * 360.0;
                    let center = (range - 2) as f64 * 60.0;
                    let mut d = (h - center).abs();
                    d = d.min(360.0 - d);
                    let w = ((45.0 - d) / 30.0).clamp(0.0, 1.0);
                    c = std::array::from_fn(|i| lerp(orig[i], c[i], w));
                }
                [c[0], c[1], c[2], s[3]]
            },
        );
    }
    // Colorize.
    check(
        "ADBE HUE SATURATION",
        vec![
            (7, Value::Bool(true)),
            (8, Value::Float(210.0)),
            (9, Value::Float(60.0)),
            (10, Value::Float(-15.0)),
        ],
        |s| {
            let l = rgb_to_hsl([s[0], s[1], s[2]])[2].clamp(0.0, 1.0);
            let l2 = hs_lightness([l; 3], -15.0)[0];
            let c = hsl_to_rgb([210.0 / 360.0, 0.6, l2]);
            [c[0], c[1], c[2], s[3]]
        },
    );
}

/// 8 / 16 bpc (AE's default): every result is clamped to [0, 1] (straight)
/// like AE's integer buffers, and Levels' "Off for 32 bpc Color" clips.
#[test]
fn eight_bpc_clamps() {
    let Some(gpu) = context() else { return };
    let input = test_image(RECT);
    let clamp01 = |o: [f64; 4]| o.map(|v| v.clamp(0.0, 1.0));
    // Exposure +2: HDR in 32 bpc, clipped at white in 8 bpc.
    let fx = builtin("ADBE Exposure2");
    let params = vec![(3, Value::Float(2.0)), (22, Value::Bool(true))];
    let got = run_depth(&gpu, F32, fx, params, &input, RECT, 1.0, false);
    let want = straight(&input, |s| {
        let f = |x: f64| exposure_ch(x, 2.0, 0.0, 1.0);
        clamp01([f(s[0]), f(s[1]), f(s[2]), s[3]])
    });
    assert_close(
        &got,
        &want,
        RECT,
        TOL32 * tolerance_scale(&gpu),
        "exposure 8 bpc",
    );
    // Levels with the default clip options clip to the output range.
    let v = [0.1, 0.8, 1.7, 0.05, 0.95];
    let fx = builtin("ADBE Easy Levels2");
    let params = vec![
        (3, Value::Float(v[0])),
        (4, Value::Float(v[1])),
        (5, Value::Float(v[2])),
        (6, Value::Float(v[3])),
        (7, Value::Float(v[4])),
    ];
    let got = run_depth(&gpu, F32, fx, params, &input, RECT, 1.0, false);
    let want = straight(&input, |s| {
        let m = |x| levels_map(x, v, [1, 1]);
        clamp01([m(s[0]), m(s[1]), m(s[2]), s[3]])
    });
    assert_close(
        &got,
        &want,
        RECT,
        TOL32 * tolerance_scale(&gpu),
        "levels 8 bpc",
    );
}

/// Identity parameters return the input image itself (no pass).
#[test]
fn identity_skips_the_pass() {
    let Some(gpu) = context() else { return };
    let input = test_image(RECT);
    for name in [
        "ADBE Tint",
        "ADBE Brightness & Contrast 2",
        "ADBE Exposure2",
        "ADBE HUE SATURATION",
        "ADBE Easy Levels2",
    ] {
        let fx = builtin(name);
        let params = match name {
            "ADBE Tint" => vec![(3, Value::Float(0.0))],
            _ => vec![],
        };
        let got = run(&gpu, F32, fx, params, &input, RECT, 1.0);
        assert_close(&got, &input, RECT, 0.0, name);
    }
}

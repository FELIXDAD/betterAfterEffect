//! Gaussian Blur and Fast Box Blur on the GPU vs exact CPU references
//! (AE-FX-300, AE-FX-302): separable kernels cut at 3σ (Gaussian Blur in linear
//! light), transparent or clamped (Repeat Edge Pixels) addressing, output
//! bounds.

mod common;

use bae_effects::BLURRINESS_TO_SIGMA;
use bae_effects::effects::blur::LINEAR_LIGHT_GAMMA;
use bae_model::Value;
use bae_render::{EffectParams, IRect};
use common::*;

/// Premultiplied `c` with its straight colour raised to `g` (sign kept).
fn regamma(c: [f32; 4], g: f64) -> [f32; 4] {
    if c[3] <= 0.0 {
        return c;
    }
    let a = f64::from(c[3]);
    let f = |v: f32| {
        let s = f64::from(v) / a;
        (s.signum() * s.abs().powf(g) * a) as f32
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}

/// Exact separable Gaussian, taps |x| ≤ 3σ, weights normalised, filtering the
/// straight colour raised to `gamma` (1 = as is).
fn gauss_reference(
    input: &Pixels,
    sigma: [f64; 2],
    repeat: bool,
    out: IRect,
    gamma: f64,
) -> Pixels {
    let kernel = |s: f64| -> Vec<f64> {
        if s <= 0.0 {
            return vec![1.0];
        }
        let r = (3.0 * s + 1e-9).floor() as i32;
        let w: Vec<f64> = (-r..=r)
            .map(|k| (-(k * k) as f64 / (2.0 * s * s)).exp())
            .collect();
        let sum: f64 = w.iter().sum();
        w.into_iter().map(|x| x / sum).collect()
    };
    let (kx, ky) = (kernel(sigma[0]), kernel(sigma[1]));
    let (rx, ry) = ((kx.len() / 2) as i32, (ky.len() / 2) as i32);
    let fetch = |x: i32, y: i32| {
        let c = if repeat {
            input.get_clamped(x, y)
        } else {
            input.get(x, y)
        };
        regamma(c, gamma)
    };
    // Horizontal pass over the rows the vertical pass reads.
    let mid_rect = IRect::new(out.x0, out.y0 - ry, out.x1, out.y1 + ry);
    let mid = Pixels::new(mid_rect, |x, y| {
        let mut acc = [0.0f64; 4];
        for (i, w) in kx.iter().enumerate() {
            let c = fetch(x + i as i32 - rx, y);
            for k in 0..4 {
                acc[k] += w * f64::from(c[k]);
            }
        }
        acc.map(|v| v as f32)
    });
    Pixels::new(out, |x, y| {
        let mut acc = [0.0f64; 4];
        for (i, w) in ky.iter().enumerate() {
            let yy = y + i as i32 - ry;
            let yy = if repeat {
                yy.clamp(input.rect.y0, input.rect.y1 - 1)
            } else {
                yy
            };
            let c = mid.get(x, yy);
            for k in 0..4 {
                acc[k] += w * f64::from(c[k]);
            }
        }
        regamma(acc.map(|v| v as f32), 1.0 / gamma)
    })
}

fn sharp_image(rect: IRect) -> Pixels {
    // Hard edges (worst case for the pyramid's interpolation), HDR spots.
    Pixels::new(rect, |x, y| {
        let inside = (x / 16 + y / 16) % 2 == 0;
        let spot = x % 37 == 0 && y % 29 == 0;
        if spot {
            [3.0, 3.0, 3.0, 1.0]
        } else if inside {
            [0.9, 0.4, 0.1, 1.0]
        } else {
            [0.0, 0.1, 0.3, 0.5]
        }
    })
}

fn gauss_case(blurriness: f64, dims: i64, repeat: bool, scale: f64, tol: f32) {
    let Some(gpu) = context() else { return };
    let fx = builtin("ADBE Gaussian Blur 2");
    let in_rect = IRect::new(-20, 10, 300, 230);
    let input = sharp_image(in_rect);
    let params = vec![
        (1, Value::Float(blurriness)),
        (2, Value::Int(dims)),
        (3, Value::Bool(repeat)),
    ];
    let inst = instance(&fx.info.match_name, params.clone());
    let p = EffectParams {
        scale,
        ..EffectParams::new(&inst)
    };
    let bounds = fx.effect().output_bounds(&p, in_rect);
    let s = BLURRINESS_TO_SIGMA * blurriness * scale;
    let sigma = match dims {
        2 => [s, 0.0],
        3 => [0.0, s],
        _ => [s, s],
    };
    let r = sigma.map(|x| {
        if x > 0.0 {
            (3.0 * x + 1e-9).floor() as i32
        } else {
            0
        }
    });
    let expected_bounds = if repeat {
        in_rect
    } else {
        IRect::new(
            in_rect.x0 - r[0],
            in_rect.y0 - r[1],
            in_rect.x1 + r[0],
            in_rect.y1 + r[1],
        )
    };
    assert_eq!(bounds, expected_bounds);
    let want = gauss_reference(&input, sigma, repeat, bounds, LINEAR_LIGHT_GAMMA);
    let soft = tolerance_scale(&gpu);
    for (format, tol) in [(F32, tol * soft), (F16, tol.max(4e-3) * soft)] {
        let got = run(&gpu, format, fx, params.clone(), &input, bounds, scale);
        assert_eq!(got.rect, bounds);
        let (d, (x, y), k) = max_diff(&got, &want, bounds);
        eprintln!(
            "gaussian b={blurriness} dims={dims} repeat={repeat} scale={scale} {format:?}: max diff {d}"
        );
        assert!(
            d <= tol,
            "b={blurriness} dims={dims} repeat={repeat} {format:?}: diff {d} at ({x}, {y}) ch {k}: got {:?} want {:?}",
            got.get(x, y),
            want.get(x, y)
        );
    }
}

#[test]
fn gaussian_direct() {
    // σ < 6: exact separable kernel.
    gauss_case(5.0, 1, false, 1.0, 2e-5);
    gauss_case(19.0, 1, true, 1.0, 2e-5);
    gauss_case(12.0, 2, false, 1.0, 2e-5);
    gauss_case(12.0, 3, true, 1.0, 2e-5);
    gauss_case(0.7, 1, false, 1.0, 2e-5);
}

#[test]
fn gaussian_pyramid() {
    // σ ≥ 6: downsample / blur / upsample. Values up to 3.0 (HDR spots); the
    // bound is relative to that range. Linear light (2.4) magnifies the
    // pyramid's small linear error in dark tones when converting back
    // (≈ 4/255 at worst next to an HDR spot, < 1/255 on 0..1 edges).
    gauss_case(20.0, 1, false, 1.0, 0.016);
    gauss_case(25.0, 1, true, 1.0, 0.016);
    gauss_case(30.0, 1, false, 1.0, 0.016);
    gauss_case(60.0, 1, true, 1.0, 0.016);
    gauss_case(100.0, 2, false, 1.0, 0.016);
    gauss_case(150.0, 1, false, 1.0, 0.016);
}

#[test]
fn gaussian_half_resolution() {
    // Blurriness is scaled by the downsample factor.
    gauss_case(38.0, 1, false, 0.5, 2e-5);
    gauss_case(100.0, 1, true, 0.5, 0.016);
}

#[test]
fn gaussian_roi_is_tile_independent() {
    // A sub-rect render equals the same pixels of a full render (the pyramid
    // grid is anchored, ROI → input_roi covers every read).
    let Some(gpu) = context() else { return };
    let fx = builtin("ADBE Gaussian Blur 2");
    let in_rect = IRect::new(0, 0, 320, 240);
    let input = sharp_image(in_rect);
    for blurriness in [10.0, 45.0, 120.0] {
        let params = vec![(1, Value::Float(blurriness)), (3, Value::Bool(false))];
        let inst = instance(&fx.info.match_name, params.clone());
        let p = EffectParams {
            scale: 1.0,
            ..EffectParams::new(&inst)
        };
        let effect = fx.effect();
        let bounds = effect.output_bounds(&p, in_rect);
        let full = run(&gpu, F32, fx, params.clone(), &input, bounds, 1.0);
        let roi = IRect::new(37, 61, 151, 133);
        let need = effect.input_roi(&p, roi).intersect(&in_rect);
        let cropped = Pixels::new(need, |x, y| input.get(x, y));
        let tile = run(&gpu, F32, fx, params, &cropped, roi, 1.0);
        assert_close(&tile, &full, roi, 1e-5, &format!("roi b={blurriness}"));
    }
}

/// Fast Box Blur reference: per pass box of width 2r+1, fractional outer taps;
/// transparent addressing on an unbounded plane, or clamp to the input rect
/// for every pass.
fn box_reference(input: &Pixels, r: [f64; 2], iterations: u32, repeat: bool, out: IRect) -> Pixels {
    let reach: [i32; 2] = r.map(|x| iterations as i32 * x.ceil() as i32);
    let mut cur = input.clone();
    let canvas = if repeat {
        input.rect
    } else {
        IRect::new(
            input.rect.x0 - reach[0],
            input.rect.y0 - reach[1],
            input.rect.x1 + reach[0],
            input.rect.y1 + reach[1],
        )
    };
    for _ in 0..iterations {
        for (axis, &ra) in r.iter().enumerate() {
            if ra <= 0.0 {
                continue;
            }
            let ri = ra.floor() as i32;
            let frac = ra - f64::from(ri);
            let norm = 1.0 / (f64::from(2 * ri + 1) + 2.0 * frac);
            let src = cur.clone();
            let fetch = |x: i32, y: i32| {
                if repeat {
                    src.get_clamped(x, y)
                } else {
                    src.get(x, y)
                }
            };
            cur = Pixels::new(canvas, |x, y| {
                let mut acc = [0.0f64; 4];
                for k in -(ri + 1)..=(ri + 1) {
                    let w = if k.abs() <= ri { 1.0 } else { frac };
                    if w == 0.0 {
                        continue;
                    }
                    let c = if axis == 0 {
                        fetch(x + k, y)
                    } else {
                        fetch(x, y + k)
                    };
                    for ch in 0..4 {
                        acc[ch] += w * f64::from(c[ch]);
                    }
                }
                acc.map(|v| (v * norm) as f32)
            });
        }
    }
    Pixels::new(out, |x, y| cur.get(x, y))
}

#[test]
fn fast_box_blur() {
    let Some(gpu) = context() else { return };
    let fx = builtin("ADBE Box Blur2");
    let in_rect = IRect::new(5, -7, 140, 100);
    let input = sharp_image(in_rect);
    for (radius, iterations, dims, repeat) in [
        (4.0, 1, 1, false),
        (3.5, 3, 1, false),
        (6.0, 2, 2, true),
        (2.25, 3, 3, false),
        (20.0, 1, 1, true),
    ] {
        let params = vec![
            (1, Value::Float(radius)),
            (2, Value::Int(iterations)),
            (3, Value::Int(dims)),
            (4, Value::Bool(repeat)),
        ];
        let inst = instance(&fx.info.match_name, params.clone());
        let p = EffectParams {
            scale: 1.0,
            ..EffectParams::new(&inst)
        };
        let bounds = fx.effect().output_bounds(&p, in_rect);
        let r = match dims {
            2 => [radius, 0.0],
            3 => [0.0, radius],
            _ => [radius, radius],
        };
        let want = box_reference(&input, r, iterations as u32, repeat, bounds);
        let got = run(&gpu, F32, fx, params, &input, bounds, 1.0);
        assert_eq!(got.rect, bounds);
        assert_close(
            &got,
            &want,
            bounds,
            1e-4,
            &format!("box r={radius} n={iterations} dims={dims} repeat={repeat}"),
        );
    }
}

#[test]
fn zero_blur_is_identity() {
    let Some(gpu) = context() else { return };
    let rect = IRect::new(0, 0, 40, 30);
    let input = sharp_image(rect);
    for name in ["ADBE Gaussian Blur 2", "ADBE Box Blur2"] {
        let got = run(
            &gpu,
            F32,
            builtin(name),
            vec![(1, Value::Float(0.0))],
            &input,
            rect,
            1.0,
        );
        assert_close(&got, &input, rect, 0.0, name);
    }
}

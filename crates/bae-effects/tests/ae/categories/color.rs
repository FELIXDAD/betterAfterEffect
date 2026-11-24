use super::{CONTROLS_CHANNEL, ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    golden_only: &[CONTROLS_CHANNEL, include_str!("../cases-color-3.json")],
    probes: &[
        ("ramp", ramp),
        ("rgb-cube", rgb_cube),
        ("color-cast", color_cast),
        ("neutral-cast", neutral_cast),
        ("neutral-flip", |x, y| {
            neutral_cast(x, super::SIZE[1] - 1 - y)
        }),
        ("neutral-cool", |x, y| {
            neutral_grid(x, y, [-10., 3., 7.], 6.)
        }),
        ("neutral-warm", |x, y| {
            neutral_grid(x, y, [12., 8., -14.], 9.)
        }),
        ("neutral-patch", neutral_patch),
    ],
    straight_inputs: &[
        "ADBE AutoLevels",
        "ADBE AutoContrast",
        "ADBE AutoColor",
        "ADBE Equalize",
        "APC Colorama",
    ],
};

fn ramp(x: u32, y: u32) -> [f64; 4] {
    let [w, h] = super::SIZE;
    let v = x as f64 / (w - 1) as f64;
    let v = if y < h / 2 { v } else { 0.5 * v };
    [v, v, v, 1.0]
}

fn rgb_cube(x: u32, y: u32) -> [f64; 4] {
    let n = y * super::SIZE[0] + x;
    let [r, g, b] = [n % 32, (n / 32) % 32, n / 1024].map(|k| f64::from(k) / 31.);
    [r, g, b, 1.]
}

fn neutral_cast(x: u32, y: u32) -> [f64; 4] {
    let [r, g, b, _] = rgb_cube(x, y);
    [
        (35. + 80. * r) / 255.,
        (60. + 90. * g) / 255.,
        (20. + 190. * b) / 255.,
        1.,
    ]
}

fn color_cast(x: u32, y: u32) -> [f64; 4] {
    let [w, h] = super::SIZE;
    let x = (x / 4) as f64 / ((w - 1) / 4) as f64;
    let y = (y / 4) as f64 / ((h - 1) / 4) as f64;
    [0.11 + 0.63 * x * x, 0.18 + 0.5 * y, 0.04 + 0.32 * x * y, 1.]
}

fn neutral_grid(x: u32, y: u32, cast: [f64; 3], step: f64) -> [f64; 4] {
    let n = (y / 8) * 32 + x / 8;
    let l = 55. + f64::from(n % 16) * 10.;
    let channel = |i: usize, j: u32| (l + cast[i] + (f64::from((n / j) % 5) - 2.) * step) / 255.;
    [channel(0, 16), channel(1, 8), channel(2, 5), 1.]
}

fn neutral_patch(x: u32, y: u32) -> [f64; 4] {
    if y < 16 {
        let g = f64::from(x / 4 * 4) / 255.;
        [g, g, g, 1.]
    } else {
        [144. / 255., 112. / 255., 128. / 255., 1.]
    }
}

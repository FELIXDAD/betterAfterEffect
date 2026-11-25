use super::{SIZE, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[include_str!("../cases-input-time.json")],
    probes: &[
        ("frame-red", red),
        ("frame-green", green),
        ("frame-blue", blue),
        ("frame-dark", dark),
        ("frame-mid", mid),
        ("frame-light", light),
        ("frame-cut", cut),
        ("time-map", map),
    ],
    ..Suite::EMPTY
};

fn red(x: u32, _y: u32) -> [f64; 4] {
    [0.8, 0.25, 0.1, if x < SIZE[0] / 2 { 0.55 } else { 0. }]
}

fn green(x: u32, _y: u32) -> [f64; 4] {
    [0.15, 0.8, 0.2, if x >= SIZE[0] / 4 { 0.75 } else { 0. }]
}

fn blue(x: u32, _y: u32) -> [f64; 4] {
    [0.25, 0.2, 0.9, if x >= SIZE[0] / 2 { 0.9 } else { 0.25 }]
}

fn dark(x: u32, _y: u32) -> [f64; 4] {
    let v = 0.1 + 0.4 * f64::from(x) / 255.;
    [v, 0.8 * v, 0.9 * v, 1.]
}

fn mid(x: u32, _y: u32) -> [f64; 4] {
    let v = 0.18 + 0.5 * f64::from(x) / 255.;
    [v, 0.8 * v, 0.9 * v, 1.]
}

fn light(x: u32, _y: u32) -> [f64; 4] {
    let v = 0.3 + 0.53 * f64::from(x) / 255.;
    [v, 0.8 * v, 0.9 * v, 1.]
}

fn cut(x: u32, y: u32) -> [f64; 4] {
    let k = f64::from((x / 16 + y / 16).is_multiple_of(2));
    [0.15 + 0.08 * k, 0.4 + 0.12 * k, 0.7 + 0.28 * k, 1.]
}

fn map(x: u32, _y: u32) -> [f64; 4] {
    let v = [0., 128. / 255., 1.][(x * 3 / SIZE[0]) as usize];
    [v, v, v, 1.]
}

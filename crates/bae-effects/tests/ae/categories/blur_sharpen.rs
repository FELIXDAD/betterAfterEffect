use super::{ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    probes: &[
        ("step", step),
        ("line", line),
        ("lineg", lineg),
        ("ush", ush),
        ("alpha", alpha),
    ],
    ..Suite::EMPTY
};

fn step(x: u32, _y: u32) -> [f64; 4] {
    let [w, _] = super::SIZE;
    let v = if x >= w / 2 { 1.0 } else { 0.0 };
    [v, v, v, 1.0]
}

fn line(x: u32, _y: u32) -> [f64; 4] {
    let [w, _] = super::SIZE;
    let v = if x == w / 2 { 1.0 } else { 0.0 };
    [v, v, v, 1.0]
}

fn lineg(x: u32, _y: u32) -> [f64; 4] {
    let [w, _] = super::SIZE;
    // A white line on 50 % grey (negative kernel lobes show).
    let v = if x == w / 2 { 1.0 } else { 0.5 };
    [v, v, v, 1.0]
}

fn ush(x: u32, y: u32) -> [f64; 4] {
    let a = [255., 192., 128., 64.][(y / 16 % 4) as usize] / 255.;
    let [r, g, b] = if x % 32 >= 16 {
        [180., 80., 120.]
    } else {
        [60., 140., 124.]
    };
    [r / 255., g / 255., b / 255., a]
}

fn alpha(x: u32, _y: u32) -> [f64; 4] {
    [
        220. / 255.,
        93. / 255.,
        47. / 255.,
        if x % 32 >= 16 {
            192. / 255.
        } else {
            64. / 255.
        },
    ]
}

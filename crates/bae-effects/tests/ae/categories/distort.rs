use super::Suite;

pub(super) const SUITE: Suite = Suite {
    default_cases: &[include_str!("../cases-distort.json")],
    probes: &[
        ("coords", coords),
        ("fine", fine),
        ("comb", comb),
        ("shift", shift),
    ],
    ..Suite::EMPTY
};

fn coords(x: u32, y: u32) -> [f64; 4] {
    let [w, _] = super::SIZE;
    let fx = x as f64 / (w - 1) as f64;
    [fx, y as f64 * 2.0 / 255.0, 0.25, 1.0]
}

fn fine(x: u32, y: u32) -> [f64; 4] {
    [
        (x % 16) as f64 * 16. / 255.,
        (y % 16) as f64 * 16. / 255.,
        ((x / 16 + y / 16) % 2) as f64 * 128. / 255.,
        1.,
    ]
}

fn comb(x: u32, y: u32) -> [f64; 4] {
    let stripe = |v: u32| f64::from(u8::from(v.is_multiple_of(4)));
    [stripe(x), stripe(y), stripe(x + y), 1.]
}

fn shift(x: u32, y: u32) -> [f64; 4] {
    [
        f64::from((x + 8) % 16 * 16) / 255.,
        f64::from((y + 8) % 16 * 16) / 255.,
        f64::from(u8::from(x.is_multiple_of(8) && y.is_multiple_of(8))),
        1.,
    ]
}

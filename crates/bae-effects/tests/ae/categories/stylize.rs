use super::Suite;

pub(super) const SUITE: Suite = Suite {
    default_cases: &[include_str!("../cases-stylize.json")],
    probes: &[("edge", edge), ("white", white)],
    ..Suite::EMPTY
};

fn edge(x: u32, y: u32) -> [f64; 4] {
    let a = [255., 192., 128., 64.][(y / 16 % 4) as usize] / 255.;
    [
        f64::from(x / 16 % 2),
        f64::from(y / 16 % 2),
        f64::from(u8::from((x + 2 * y) % 32 < 16)),
        a,
    ]
}

fn white(_x: u32, _y: u32) -> [f64; 4] {
    [1.; 4]
}

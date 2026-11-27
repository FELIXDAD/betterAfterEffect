use super::{KEYING_MATTE, ORIGINAL, Suite};

fn paint_solid(_: u32, _: u32) -> [f64; 4] {
    [120. / 255., 140. / 255., 150. / 255., 1.]
}

fn paint_rectangle(x: u32, y: u32) -> [f64; 4] {
    let l = f64::from((30..140).contains(&x) && (32..83).contains(&y));
    [l, l, l, 1.]
}

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL, include_str!("../cases-stroke-family.json")],
    golden_only: &[KEYING_MATTE],
    probes: &[
        ("paint_solid", paint_solid),
        ("paint_rectangle", paint_rectangle),
    ],
    ..Suite::EMPTY
};

use super::{KEYING_MATTE, ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    golden_only: &[KEYING_MATTE],
    probes: &[("speckles", speckles)],
    straight_inputs: &["ADBE PS Median", "ADBE Dust & Scratches"],
};

fn speckles(x: u32, y: u32) -> [f64; 4] {
    let levels = |n: u32| f64::from(n % 251) / 250.0;
    [
        levels(x * 17 + y * 53 + x * y),
        levels(x * 73 + y * 7),
        levels(x * 3 + y * 13),
        1.0,
    ]
}

use super::{CONTROLS_CHANNEL, ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    golden_only: &[CONTROLS_CHANNEL],
    probes: &[("ramp", ramp)],
    ..Suite::EMPTY
};

fn ramp(x: u32, y: u32) -> [f64; 4] {
    let [w, h] = super::SIZE;
    let v = x as f64 / (w - 1) as f64;
    let v = if y < h / 2 { v } else { 0.5 * v };
    [v, v, v, 1.0]
}

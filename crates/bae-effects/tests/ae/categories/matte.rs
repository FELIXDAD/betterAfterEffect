use super::{CONTROLS_CHANNEL, KEYING_MATTE, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[CONTROLS_CHANNEL, KEYING_MATTE],
    probes: &[("matte-edge", matte_edge)],
    straight_inputs: &["ADBE Simple Choker"],
    ..Suite::EMPTY
};

fn matte_edge(x: u32, y: u32) -> [f64; 4] {
    let [w, h] = super::SIZE;
    let fx = x as f64 / (w - 1) as f64;
    let fy = y as f64 / (h - 1) as f64;
    let edge = (fx * 2.0 + 0.2 * fy - 0.5).clamp(0.0, 1.0);
    let alpha = if y < h / 3 {
        f64::from(x > w / 2)
    } else if y < 2 * h / 3 {
        edge
    } else {
        f64::from(!(x / 5 + y / 7).is_multiple_of(3))
    };
    [0.2 + 0.75 * fx, 0.3 + 0.35 * fy, 0.8 - 0.6 * fx, alpha]
}

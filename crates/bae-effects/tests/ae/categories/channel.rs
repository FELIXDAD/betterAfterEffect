use super::{CONTROLS_CHANNEL, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[CONTROLS_CHANNEL, include_str!("../cases-input-layers.json")],
    straight_inputs: &[
        "ADBE Invert",
        "ADBE Shift Channels",
        "ADBE Set Matte3",
        "ADBE Minimax",
    ],
    ..Suite::EMPTY
};

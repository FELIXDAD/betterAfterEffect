use super::{CONTROLS_CHANNEL, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[CONTROLS_CHANNEL],
    straight_inputs: &[
        "ADBE Invert",
        "ADBE Shift Channels",
        "ADBE Set Matte3",
        "ADBE Minimax",
    ],
    ..Suite::EMPTY
};

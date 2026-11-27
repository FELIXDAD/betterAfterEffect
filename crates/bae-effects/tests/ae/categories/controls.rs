use super::{CONTROLS_CHANNEL, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[CONTROLS_CHANNEL],
    ..Suite::EMPTY
};

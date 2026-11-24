use super::{KEYING_MATTE, Suite};

pub(super) const SUITE: Suite = Suite {
    golden_only: &[KEYING_MATTE],
    straight_inputs: &["ADBE Linear Color Key2"],
    ..Suite::EMPTY
};

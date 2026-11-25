use super::{KEYING_MATTE, ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    golden_only: &[KEYING_MATTE],
    ..Suite::EMPTY
};

use super::{ORIGINAL, Suite};

pub(super) const SUITE: Suite = Suite {
    default_cases: &[ORIGINAL],
    ..Suite::EMPTY
};

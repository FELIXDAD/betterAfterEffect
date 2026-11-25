//! Built-in effects vs After Effects renders stored in `tests/ae/golden/`
//! (AE 26.2.1, see `tests/ae/README.md`). Each case bounds the max and the mean
//! per-channel difference; software / virtual adapters (CI's WARP) get 2 more
//! levels of max and 0.5 of mean.

#[path = "ae/categories/mod.rs"]
mod categories;
#[path = "ae/harness.rs"]
mod harness;

include!("ae/golden.rs");

//! Render-only samples of animated effect controls, ordered from newest to oldest.
use bae_model::{Time, Value};

/// Outside the AE disk-ID range. The derived value travels with the evaluated
/// effect so render caching includes its history even when today's value is unchanged.
pub const HISTORY_PARAM: u32 = u32::MAX;
pub const PROFILE_SAMPLES: usize = 64;

pub fn ripple_profile(time: Time, span: f64, level: impl Fn(Time) -> f64) -> Value {
    Value::FloatList(
        (0..PROFILE_SAMPLES)
            .map(|i| {
                let delay = span * i as f64 / (PROFILE_SAMPLES - 1) as f64;
                level(time - Time::from_seconds(delay))
            })
            .collect(),
    )
}

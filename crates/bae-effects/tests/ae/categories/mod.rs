//! Case manifests, input probes and straight-alpha fixtures by AE category.
use crate::harness::SIZE;

mod blur_sharpen;
mod channel;
mod color;
mod controls;
mod distort;
mod generate;
mod keying;
mod matte;
mod noise_grain;
mod perspective;
mod stylize;
mod time;
mod transition;

pub type Pixel = fn(u32, u32) -> [f64; 4];
pub type Probe = (&'static str, Pixel);

pub struct Suite {
    pub default_cases: &'static [&'static str],
    pub golden_only: &'static [&'static str],
    pub probes: &'static [Probe],
    pub straight_inputs: &'static [&'static str],
}

impl Suite {
    const EMPTY: Self = Self {
        default_cases: &[],
        golden_only: &[],
        probes: &[],
        straight_inputs: &[],
    };
}

const ORIGINAL: &str = include_str!("../cases.json");
const CONTROLS_CHANNEL: &str = include_str!("../cases-controls-channel.json");
const KEYING_MATTE: &str = include_str!("../cases-keying-matte.json");

pub const ALL: &[Suite] = &[
    blur_sharpen::SUITE,
    channel::SUITE,
    color::SUITE,
    controls::SUITE,
    distort::SUITE,
    generate::SUITE,
    keying::SUITE,
    matte::SUITE,
    noise_grain::SUITE,
    perspective::SUITE,
    stylize::SUITE,
    time::SUITE,
    transition::SUITE,
];

// Older AE batches span categories. Preserve their original cases and run each
// batch once even when several categories use the same manifest.
pub fn manifests(select: impl Fn(&Suite) -> &'static [&'static str]) -> Vec<&'static str> {
    let mut files = Vec::new();
    for suite in ALL {
        for &file in select(suite) {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    files
}

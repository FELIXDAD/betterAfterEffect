//! Duotone: a native CPU effect written in Rust with `bae-plugin-sdk`.
//!
//! Maps each pixel's luminance to a gradient between a shadow color and a
//! highlight color, then mixes with the original by "Amount". The SDK
//! generates the C ABI entry point, vtable and parameter definitions; this
//! file only describes the parameters and renders pixels.

use core::ffi::CStr;

use bae_plugin_sdk::abi::{BAE_DISPLAY_PERCENT, BAE_EFFECT_FLAG_PIXEL_INDEPENDENT};
use bae_plugin_sdk::effect::{CpuEffect, Error, Param, RenderArgs};

const SHADOWS: u32 = 1;
const HIGHLIGHTS: u32 = 2;
const AMOUNT: u32 = 3;

#[derive(Default)]
struct Duotone;

impl CpuEffect for Duotone {
    const ID: &'static CStr = c"duotone";
    const PARAMS: &'static [Param] = &[
        Param::color(SHADOWS, c"shadows", c"Shadows", [0.10, 0.05, 0.30, 1.0]),
        Param::color(
            HIGHLIGHTS,
            c"highlights",
            c"Highlights",
            [1.0, 0.85, 0.40, 1.0],
        ),
        Param::float(AMOUNT, c"amount", c"Amount", 100.0, [0.0, 100.0])
            .display(BAE_DISPLAY_PERCENT),
    ];
    const FLAGS: u64 = BAE_EFFECT_FLAG_PIXEL_INDEPENDENT;

    fn render(&self, args: &mut RenderArgs<'_>) -> Result<(), Error> {
        let dark = args.params.color(SHADOWS).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let light = args.params.color(HIGHLIGHTS).unwrap_or([1.0; 4]);
        let amount = (args.params.float(AMOUNT).unwrap_or(100.0) / 100.0).clamp(0.0, 1.0) as f32;
        args.map_pixels(|_, _, [r, g, b, a]| {
            if a <= 0.0 {
                return [0.0; 4];
            }
            // Luminance of the straight color (Rec. 709 weights).
            let y = ((0.2126 * r + 0.7152 * g + 0.0722 * b) / a).clamp(0.0, 1.0);
            let mut out = [0.0; 4];
            for k in 0..3 {
                let tinted = (dark[k] + (light[k] - dark[k]) * y) * a;
                let original = [r, g, b][k];
                out[k] = original + (tinted - original) * amount;
            }
            out[3] = a;
            out
        })
    }
}

bae_plugin_sdk::export_plugin!(
    id: "com.example.duotone-rs",
    version: "0.1.0",
    effects: [Duotone],
);

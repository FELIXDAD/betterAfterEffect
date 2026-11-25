//! Numeric displays. The host formats each string once and the GPU draws its strokes.
mod definitions;
mod format;
use crate::effects::image_grid;
use crate::{
    Builtin, EffectTarget, TargetEffect,
    gpu::{self, Resource, UniformWriter},
    params::Params,
    rust_builtin,
};
use bae_gpu::wgpu;
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
pub(crate) use definitions::{numbers, timecode};
use std::sync::Arc;

struct Counter {
    def: EffectDef,
    timecode: bool,
}
const BYTES: u32 = 144 + 32 * 16;
const LABEL: &str = "bae.effects.numeric-display";

fn kernel(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<Arc<gpu::Kernel>, RenderError> {
    image_grid::kernel_with_inputs(rt, format, LABEL, include_str!("glyphs.wgsl"), BYTES, 1)
}
pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    kernel(rt, format)?;
    Ok(())
}

// Bits index the line segments in glyphs.wgsl. Punctuation uses separate dots.
fn glyph(c: char) -> u32 {
    let segments = match c.to_ascii_uppercase() {
        '0' => "abcdef",
        '1' => "bc",
        '2' => "abged",
        '3' => "abgcd",
        '4' => "fgbc",
        '5' => "afgcd",
        '6' => "afgedc",
        '7' => "abc",
        '8' => "abcdefg",
        '9' => "abfgcd",
        'A' => "abcefg",
        'B' => "fegcdmn",
        'C' => "afed",
        'D' => "abcdmn",
        'E' => "afged",
        'F' => "afge",
        'G' => "afedcg",
        'H' => "febcg",
        'I' => "admn",
        'J' => "bcde",
        'K' => "feik",
        'L' => "fed",
        'M' => "febchi",
        'N' => "febchk",
        'O' => "abcdef",
        'P' => "afbeg",
        'Q' => "abcdefk",
        'R' => "afbegk",
        'S' => "afgcd",
        'T' => "amn",
        'U' => "febcd",
        'V' => "feij",
        'W' => "febcjk",
        'X' => "hijk",
        'Y' => "hin",
        'Z' => "aijd",
        '-' => "g",
        '+' => "gmn",
        '/' => "ij",
        '.' => "o",
        ',' => "oq",
        ':' => "op",
        ';' => "opq",
        _ => "",
    };
    segments.bytes().fold(0, |bits, b| bits | 1 << (b - b'a'))
}

impl Effect for Counter {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}
impl TargetEffect for Counter {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let tc = self.timecode;
        let text = if tc {
            format::timecode(v)
        } else {
            format::numbers(v)
        };
        let size = (v.f(if tc { 6 } else { 11 }) * p.scale) as f32;
        let grid = image_grid::grid_scale(p, input);
        let bounds = if p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(p)
        };
        let [x, y] = p
            .point(if tc { 5 } else { 6 })
            .map(|a| a.map(|x| (x * p.scale) as f32))
            .unwrap_or_else(|| {
                let factor = if tc { 0.1 } else { 0.5 };
                [
                    bounds.width() as f32 * factor,
                    bounds.height() as f32 * factor,
                ]
            });
        let mut u = UniformWriter::with_size(BYTES as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        let rgba = |id| match v.value(id) {
            Some(Value::Color(c)) => c.map(|x| x as f32),
            _ => [1.; 4],
        };
        let mut left = 0.;
        let mut count = 0;
        for (i, c) in text.chars().take(32).enumerate() {
            let narrow = !tc && v.b(13) && "1:.,; I".contains(c);
            let width = if narrow { 0.29 } else { 0.55 };
            u.u32(144 + i * 16, glyph(c))
                .f32(148 + i * 16, left)
                .f32(152 + i * 16, width);
            let tracking = if tc {
                0.
            } else {
                (v.f(12) * p.scale) as f32 / size.max(0.01)
            };
            left += (width + 0.13 + tracking).max(0.08);
            count = i + 1;
        }
        u.i32s(32, &image_grid::rect4(roi))
            .f32s(48, &[x, y, size, left])
            .f32s(64, &rgba(if tc { 7 } else { 8 }))
            .f32s(80, &rgba(9))
            .f32s(96, &if tc && v.b(10) { rgba(11) } else { [0.; 4] })
            .f32s(
                112,
                &[
                    if tc { (v.f(12) * 0.01) as f32 } else { 1. },
                    if tc { 0. } else { (v.f(10) * p.scale) as f32 },
                    0.,
                    0.,
                ],
            )
            .u32(128, count as u32)
            .u32(132, if tc { 1 } else { v.popup(7) as u32 })
            .u32(136, u32::from(v.b(if tc { 15 } else { 14 })))
            .u32(140, u32::from(p.float));
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(
            &mut pass,
            &pipeline,
            &group,
            &[gpu::push_uniform(&rt, &u)],
            roi.size(),
        );
        Ok(out)
    }
}

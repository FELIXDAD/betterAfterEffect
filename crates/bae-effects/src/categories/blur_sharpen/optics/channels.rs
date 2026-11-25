//! Filter premultiplied colour and its matching coverage with a different Gaussian per channel.
//! A channel uses its own pyramid level, but all four horizontal lines share one dispatch,
//! as do the vertical lines. Two RGBA scratch surfaces hold the four colour/coverage pairs.
use std::sync::Arc;

use super::BLURRINESS_TO_SIGMA;
use crate::{
    EffectTarget, blur,
    effects::image_grid,
    gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, Runtime, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};

const WEIGHTS: usize = 19; // The Gaussian planner keeps level sigma below six texels.
const BYTES: u32 = (256 + WEIGHTS * 16) as u32;
const SOURCE: &str = include_str!("channels.wgsl");

fn pipeline(
    rt: &Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<Kernel>, RenderError> {
    let key = KernelKey {
        owner: "bae.effects.channel-blur".into(),
        entry: entry.into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        let mut bindings = vec![(0, Binding::Uniform(BYTES))];
        let inputs = match entry {
            "horizontal" => 4,
            "vertical" => 2,
            _ => 3,
        };
        bindings.extend((1..=inputs).map(|i| (i, Binding::Texture)));
        bindings.push((5, Binding::Storage(format)));
        if entry != "resolve" {
            bindings.push((6, Binding::Storage(format)));
        }
        Ok(KernelSource {
            label: "bae.effects.channel-blur",
            wgsl: SOURCE.replace("WORKING_FORMAT", blur::format_name(format)?),
            entry,
            bindings,
            workgroup: [8, 8, 1],
        })
    })
}

pub(super) fn prewarm(rt: &Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in ["horizontal", "vertical", "resolve"] {
        pipeline(rt, format, entry)?;
    }
    Ok(())
}

fn dispatch(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    offset: u32,
    inputs: &[&GpuImage],
    outputs: &[&GpuImage],
) -> Result<(), RenderError> {
    let rt = gpu::runtime(target.gpu());
    let kernel = pipeline(&rt, target.format(), entry)?;
    let resources: Vec<_> = std::iter::once(Resource::Uniform)
        .chain(
            inputs
                .iter()
                .chain(outputs)
                .map(|im| Resource::View(&im.view)),
        )
        .collect();
    let group = rt.bind_group(&kernel, &resources);
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(entry),
            timestamp_writes: None,
        });
    gpu::dispatch(
        &mut pass,
        &kernel,
        &group,
        &[offset],
        outputs[0].texel_size(),
    );
    Ok(())
}

pub(super) fn render(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let grid = image_grid::grid_scale(v.p, input);
    let native = grid / input.pixel_scale;
    let sigma = [1, 2, 3, 4].map(|id| v.f(id) * v.p.scale * native * BLURRINESS_TO_SIGMA);
    if roi.is_empty() || sigma.iter().all(|&s| blur::radius(s) == 0) {
        return Ok(input.clone());
    }
    let axes = [v.popup(6) != 3, v.popup(6) != 2];
    let repeat = v.b(5);
    let factors = sigma.map(blur::level_factor);
    let mut source = input.clone();
    source.rect = image_grid::pixel_rect(input.rect, 1. / input.pixel_scale);
    source.pixel_scale = 1.;
    let mut levels = vec![source];
    while (1 << (levels.len() - 1)) < *factors.iter().max().unwrap() {
        let next = blur::reduce_level(
            target,
            levels.last().unwrap(),
            axes.map(|active| if active { 2 } else { 1 }),
            repeat,
        )?;
        levels.push(next);
    }
    let textures = factors.map(|f| &levels[f.ilog2() as usize]);
    let mut uniforms = UniformWriter::with_size(BYTES as usize);
    let mut weights = [[0.; 4]; WEIGHTS];
    let mut hsize = [1; 2];
    let mut vsize = [1; 2];
    for (c, &factor) in factors.iter().enumerate() {
        let xy = axes.map(|axis| if axis { factor } else { 1 });
        let s = blur::level_sigma(sigma[c], factor);
        let r = blur::radius(s);
        let tile = |start: i32, end: i32, f: i32| {
            let k = native / f64::from(f);
            if k == 1. {
                (start, end)
            } else {
                (
                    ((f64::from(start) + 0.5) * k - 0.5).floor() as i32,
                    ((f64::from(end) - 0.5) * k - 0.5).floor() as i32 + 2,
                )
            }
        };
        let (x0, x1) = tile(roi.x0, roi.x1, xy[0]);
        let (y0, y1) = tile(roi.y0, roi.y1, xy[1]);
        let rect = IRect::new(x0, y0, x1, y1);
        let at = c * 48;
        uniforms
            .i32s(at, &image_grid::rect4(textures[c].rect))
            .i32s(at + 16, &image_grid::rect4(rect))
            .i32s(at + 32, &[xy[0], xy[1], r, 0]);
        if r == 0 {
            continue;
        }
        let [w, h] = rect.size();
        hsize = [
            hsize[0].max(w),
            hsize[1].max(h + if axes[1] { 2 * r as u32 } else { 0 }),
        ];
        vsize = [vsize[0].max(w), vsize[1].max(h)];
        for (row, w) in weights.iter_mut().zip(blur::gaussian_weights(s, r)) {
            row[c] = w;
        }
    }
    uniforms
        .i32s(192, &image_grid::rect4(roi))
        .i32s(208, &image_grid::rect4(levels[0].rect))
        .u32(224, u32::from(axes[0]))
        .u32(228, u32::from(axes[1]))
        .u32(232, u32::from(repeat))
        .u32(236, u32::from(v.p.float))
        .f32(240, native as f32);
    for (i, w) in weights.iter().enumerate() {
        uniforms.f32s(256 + i * 16, w);
    }
    let offset = gpu::push_uniform(&gpu::runtime(target.gpu()), &uniforms);
    let mut pair = |[w, h]: [u32; 2]| -> Result<[GpuImage; 2], RenderError> {
        let r = IRect::from_size(w, h);
        Ok([target.allocate(r, 1.)?, target.allocate(r, 1.)?])
    };
    let [hc, ha] = pair(hsize)?;
    let [vc, va] = pair(vsize)?;
    dispatch(target, "horizontal", offset, &textures, &[&hc, &ha])?;
    dispatch(target, "vertical", offset, &[&hc, &ha], &[&vc, &va])?;
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    dispatch(target, "resolve", offset, &[&vc, &va, &levels[0]], &[&out])?;
    Ok(out)
}

use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};

use crate::{
    EffectTarget,
    blur::{self, Edge},
    gpu::{self, Resource, UniformWriter},
    params::Params,
};

use super::{Pass, image_grid};

const LABEL: &str = "bae.effects.compound-blur";
const SOURCE: &str = include_str!("blur_map.wgsl");
const LEVELS: usize = 7;
const BYTES: u32 = 336;
const MAX_RADIUS: f64 = 31.;

/// The measured impulse widths account for a small base filter.
fn sigma(radius: f64) -> f64 {
    radius * 1.05 + 0.60
}

pub(super) fn input_roi(radius: f64, scale: f64, roi: IRect) -> IRect {
    if radius <= 0. {
        return roi;
    }
    blur::plan_gaussian(
        IRect::EMPTY,
        [sigma(radius.ceil().min(MAX_RADIUS)) * scale; 2],
        Edge::Transparent,
        roi,
    )
    .read
    .inflate(2)
}

pub(super) fn encode(
    target: &mut dyn EffectTarget,
    pass: &Pass,
    source: &GpuImage,
    map: &GpuImage,
    params: Params<'_>,
) -> Result<GpuImage, RenderError> {
    let radius = params.f(2);
    let limit = radius.ceil().min(MAX_RADIUS);
    let radii = [0., 1., 2., 4., 8., 16., MAX_RADIUS].map(|r: f64| r.min(limit));
    let rt = gpu::runtime(target.gpu());
    let kernel = image_grid::kernel_with_inputs(&rt, target.format(), LABEL, SOURCE, BYTES, 8)?;
    let out = target.allocate(image_grid::pixel_rect(pass.roi, pass.grid), pass.grid)?;
    let premult = crate::common::premultiplied(target, source)?;
    let base = image_grid::half_level(target, &premult, pass.layer, pass.grid, false)?;
    let mut levels = Vec::with_capacity(LEVELS);
    levels.push(source.clone());
    for index in 1..LEVELS {
        let r = radii[index];
        if r == radii[index - 1] {
            levels.push(levels[index - 1].clone());
        } else {
            levels.push(make_level(target, pass, &base, r)?);
        }
    }

    let mut u = UniformWriter::with_size(BYTES as usize);
    image_grid::image_coords(&mut u, 0, map, pass.grid);
    for (i, level) in levels.iter().enumerate() {
        image_grid::image_coords(&mut u, 32 * (i + 1), level, pass.grid);
    }
    u.i32s(256, &image_grid::rect4(pass.roi))
        .i32s(272, &image_grid::rect4(pass.layer))
        .f32s(288, &radii.map(|r| r as f32))
        .f32(316, radius as f32)
        .f32(320, pass.grid as f32)
        .u32(324, if params.b(3) { 3 } else { 2 })
        .u32(328, u32::from(params.b(4)))
        .u32(332, u32::from(params.p.float));
    let offsets = [gpu::push_uniform(&rt, &u)];
    let mut resources = vec![Resource::Uniform, Resource::View(&map.view)];
    resources.extend(levels.iter().map(|level| Resource::View(&level.view)));
    resources.push(Resource::View(&out.view));
    let group = rt.bind_group(&kernel, &resources);
    let mut encoder = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(LABEL),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut encoder, &kernel, &group, &offsets, pass.roi.size());
    Ok(out)
}

fn make_level(
    target: &mut dyn EffectTarget,
    pass: &Pass,
    source: &GpuImage,
    radius: f64,
) -> Result<GpuImage, RenderError> {
    let scale = source.pixel_scale;
    let [x, y] = [source.rect.x0, source.rect.y0];
    let in_texels = |v: i32, origin: i32| (f64::from(v) * pass.grid - f64::from(origin)) / scale;
    let out = IRect::new(
        in_texels(pass.roi.x0, x).floor() as i32,
        in_texels(pass.roi.y0, y).floor() as i32,
        in_texels(pass.roi.x1, x).ceil() as i32,
        in_texels(pass.roi.y1, y).ceil() as i32,
    )
    .inflate(1);
    let [w, h] = source.texel_size();
    let mut texels = source.clone();
    texels.rect = IRect::from_size(w, h);
    texels.pixel_scale = 1.;
    let sigma = blur::level_sigma(sigma(radius) * 2. / scale, 2);
    let mut plan = blur::plan_gaussian(texels.rect, [sigma; 2], Edge::Transparent, out);
    // At radius one the measured reduced-grid weights are 1/4, 1/2, 1/4.
    if radius == 1. {
        plan = plan.with_tent(2.);
    }
    let mut level = plan.encode_level(target, &texels)?;
    let in_layer = |v: i32, origin: i32| (f64::from(v) * scale + f64::from(origin)).round() as i32;
    level.rect = IRect::new(
        in_layer(level.rect.x0, x),
        in_layer(level.rect.y0, y),
        in_layer(level.rect.x1, x),
        in_layer(level.rect.y1, y),
    );
    level.pixel_scale *= scale;
    Ok(level)
}

pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    image_grid::kernel_with_inputs(rt, format, LABEL, SOURCE, BYTES, 8)?;
    Ok(())
}

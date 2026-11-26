//! Dense optical flow between two frames, shared by Pixel Motion Blur,
//! Timewarp's Pixel Motion and Pixel Motion frame blending (`flow.wgsl`).
//!
//! DIS-style (Kroeger et al. 2016): a luma pyramid of both frames, then from
//! the coarsest level down, an inverse-compositional patch search seeded by
//! the coarser flow and a per-pixel densification weighted by the match
//! error. The finest level follows Vector Detail (20 % → a quarter of the
//! render size), so the default costs a few milliseconds at 1080p.

use std::sync::Arc;

use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};

use super::super::image_grid;
use crate::EffectTarget;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};

const LABEL: &str = "bae.effects.flow";
const SHADER: &str = include_str!("flow.wgsl");
const BYTES: u32 = 160;
const ENTRIES: [&str; 6] = [
    "luma_base",
    "luma_down",
    "search",
    "densify",
    "vector_blur",
    "interpolate",
];
/// Pyramid levels above the finest.
const MAX_LEVELS: usize = 6;
/// Gauss-Newton iterations per patch.
const ITERATIONS: u32 = 10;
/// Render pixels around the ROI the flow also covers (motion entering it).
const MARGIN: i32 = 32;

fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<Kernel>, RenderError> {
    let key = KernelKey {
        owner: Arc::from(LABEL),
        entry: Arc::from(entry),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label: LABEL,
            wgsl: [
                image_grid::CHANNEL_MATH,
                include_str!("../../shaders/image_grid.wgsl"),
                SHADER,
            ]
            .concat()
            .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
            entry,
            bindings: vec![
                (0, Binding::Uniform(BYTES)),
                (1, Binding::Texture),
                (2, Binding::Texture),
                (3, Binding::Texture),
                (4, Binding::Texture),
                (5, Binding::Storage(format)),
            ],
            workgroup: [8, 8, 1],
        })
    })
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in ENTRIES {
        kernel(rt, format, entry)?;
    }
    Ok(())
}

#[derive(Default)]
struct Call {
    region: IRect,
    roi: IRect,
    size: [u32; 4],
    data: [f32; 4],
    flags: [u32; 4],
    coarse: [u32; 2],
}

/// Encodes one dispatch of `entry` over `threads` writing `out`. `images[0]`
/// and `images[1]` also get frame coordinates (FxImage) on the render grid.
fn run(
    target: &mut dyn EffectTarget,
    entry: &'static str,
    grid: f64,
    images: [Option<&GpuImage>; 4],
    c: &Call,
    out: &GpuImage,
    threads: [u32; 2],
) -> Result<(), RenderError> {
    let rt = gpu::runtime(target.gpu());
    let kernel = kernel(&rt, target.format(), entry)?;
    let mut w = UniformWriter::with_size(BYTES as usize);
    for (i, image) in images.iter().take(2).enumerate() {
        if let Some(image) = image {
            image_grid::image_coords(&mut w, i * 32, image, grid);
        }
    }
    w.i32s(64, &image_grid::rect4(c.region))
        .i32s(80, &image_grid::rect4(c.roi))
        .f32s(112, &c.data);
    for i in 0..4 {
        w.u32(96 + 4 * i, c.size[i]).u32(128 + 4 * i, c.flags[i]);
    }
    w.u32(144, c.coarse[0]).u32(148, c.coarse[1]);
    let resources: Vec<Resource<'_>> = std::iter::once(Resource::Uniform)
        .chain(
            images
                .iter()
                .map(|i| i.map_or(Resource::Uniform, |i| Resource::View(&i.view))),
        )
        .chain(std::iter::once(Resource::View(&out.view)))
        .collect();
    let group = rt.bind_group(&kernel, &resources);
    let offset = gpu::push_uniform(&rt, &w);
    let mut pass = target
        .encoder()
        .begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(LABEL),
            timestamp_writes: None,
        });
    gpu::dispatch(&mut pass, &kernel, &group, &[offset], threads);
    Ok(())
}

/// Both frames' luma from the finest flow level up.
pub(crate) struct Pyramid {
    levels: Vec<(GpuImage, [u32; 2])>,
    finest: u32,
    region: IRect,
    grid: f64,
}

/// A flow field: render-pixel motion per texel of the finest level.
pub(crate) struct Field {
    image: GpuImage,
    size: [u32; 2],
    scale: f32,
    region: IRect,
}

impl Field {
    /// Reuse the flow texture in render-grid coordinates. Its two channels are
    /// displacements in render pixels, including at reduced preview resolution.
    pub(crate) fn grid_image(&self, grid: f64) -> GpuImage {
        let mut image = self.image.clone();
        image.rect = image_grid::pixel_rect(self.region, grid);
        image.pixel_scale = grid * f64::from(self.scale);
        image.alpha = bae_render::ImageAlpha::Straight;
        image
    }
}

fn patches(n: u32) -> u32 {
    if n <= 8 { 1 } else { (n - 8).div_ceil(4) + 1 }
}

/// The luma pyramid of `a` and `b` around `roi` (render grid at `grid`).
/// `detail` is AE's Vector Detail (percent).
pub(crate) fn pyramid(
    target: &mut dyn EffectTarget,
    a: &GpuImage,
    b: &GpuImage,
    roi: IRect,
    grid: f64,
    detail: f64,
) -> Result<Pyramid, RenderError> {
    let region = IRect::new(
        roi.x0 - MARGIN,
        roi.y0 - MARGIN,
        roi.x1 + MARGIN,
        roi.y1 + MARGIN,
    );
    let finest = (100. / detail.clamp(5., 100.)).log2().round().clamp(0., 4.) as u32;
    let n = 1u32 << finest;
    let [w, h] = region.size();
    let size = [w.div_ceil(n).max(1), h.div_ceil(n).max(1)];
    let base = target.allocate(IRect::from_size(size[0], size[1]), 1.)?;
    let call = Call {
        region,
        size: [size[0], size[1], 0, 0],
        data: [n as f32, 0., 0., 0.],
        ..Call::default()
    };
    run(
        target,
        "luma_base",
        grid,
        [Some(a), Some(b), None, None],
        &call,
        &base,
        size,
    )?;
    let mut levels = vec![(base, size)];
    while levels.len() < MAX_LEVELS {
        let (prev, [pw, ph]) = levels.last().expect("non-empty");
        if pw.min(ph) < &32 {
            break;
        }
        let next = [pw.div_ceil(2), ph.div_ceil(2)];
        let image = target.allocate(IRect::from_size(next[0], next[1]), 1.)?;
        let call = Call {
            size: [next[0], next[1], *pw, *ph],
            ..Call::default()
        };
        let prev = prev.clone();
        run(
            target,
            "luma_down",
            grid,
            [Some(&prev), None, None, None],
            &call,
            &image,
            next,
        )?;
        levels.push((image, next));
    }
    Ok(Pyramid {
        levels,
        finest,
        region,
        grid,
    })
}

/// The flow from the pyramid's first frame to its second (`reverse`: second
/// to first).
pub(crate) fn flow(
    target: &mut dyn EffectTarget,
    pyramid: &Pyramid,
    reverse: bool,
) -> Result<Field, RenderError> {
    let mut coarser: Option<(GpuImage, [u32; 2])> = None;
    for (k, (luma, size)) in pyramid.levels.iter().enumerate().rev() {
        let scale = (1u32 << (pyramid.finest + k as u32)) as f32;
        let grid_n = [patches(size[0]), patches(size[1])];
        let found = target.allocate(IRect::from_size(grid_n[0], grid_n[1]), 1.)?;
        let call = Call {
            size: [size[0], size[1], grid_n[0], grid_n[1]],
            data: [scale, 0., 0., 0.],
            flags: [
                u32::from(reverse),
                0,
                ITERATIONS,
                u32::from(coarser.is_some()),
            ],
            coarse: coarser.as_ref().map_or([0, 0], |c| c.1),
            ..Call::default()
        };
        let prev = coarser.as_ref().map(|c| c.0.clone());
        run(
            target,
            "search",
            pyramid.grid,
            [Some(luma), prev.as_ref(), None, None],
            &call,
            &found,
            grid_n,
        )?;
        let field = target.allocate(IRect::from_size(size[0], size[1]), 1.)?;
        run(
            target,
            "densify",
            pyramid.grid,
            [Some(luma), Some(&found), None, None],
            &call,
            &field,
            *size,
        )?;
        coarser = Some((field, *size));
    }
    let (image, size) = coarser.expect("at least one level");
    Ok(Field {
        image,
        size,
        scale: (1u32 << pyramid.finest) as f32,
        region: pyramid.region,
    })
}

/// Pixel Motion Blur: `a` averaged along `field` over the shutter
/// `[start, start + span]` (fractions of the frame-to-frame motion).
#[allow(clippy::too_many_arguments)]
pub(crate) fn vector_blur(
    target: &mut dyn EffectTarget,
    a: &GpuImage,
    field: &Field,
    roi: IRect,
    grid: f64,
    start: f32,
    span: f32,
    samples: u32,
) -> Result<GpuImage, RenderError> {
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let call = Call {
        region: field.region,
        roi,
        size: [field.size[0], field.size[1], 0, 0],
        data: [field.scale, start, span, 0.],
        flags: [0, samples, 0, 0],
        ..Call::default()
    };
    run(
        target,
        "vector_blur",
        grid,
        [Some(a), None, Some(&field.image), None],
        &call,
        &out,
        roi.size(),
    )?;
    Ok(out)
}

/// The frame at `phase` (0 = `a`, 1 = `b`) between two frames, given the flow
/// each way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn interpolate(
    target: &mut dyn EffectTarget,
    a: &GpuImage,
    b: &GpuImage,
    ab: &Field,
    ba: &Field,
    roi: IRect,
    grid: f64,
    phase: f32,
) -> Result<GpuImage, RenderError> {
    let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let call = Call {
        region: ab.region,
        roi,
        size: [ab.size[0], ab.size[1], 0, 0],
        data: [ab.scale, phase, 0., 0.],
        ..Call::default()
    };
    run(
        target,
        "interpolate",
        grid,
        [Some(a), Some(b), Some(&ab.image), Some(&ba.image)],
        &call,
        &out,
        roi.size(),
    )?;
    Ok(out)
}

/// Both flows between `a` and `b`, then the frame at `phase`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tween(
    target: &mut dyn EffectTarget,
    a: &GpuImage,
    b: &GpuImage,
    roi: IRect,
    grid: f64,
    detail: f64,
    phases: &[f32],
) -> Result<Vec<GpuImage>, RenderError> {
    let pyramid = pyramid(target, a, b, roi, grid, detail)?;
    let ab = flow(target, &pyramid, false)?;
    let ba = flow(target, &pyramid, true)?;
    phases
        .iter()
        .map(|&w| interpolate(target, a, b, &ab, &ba, roi, grid, w))
        .collect()
}

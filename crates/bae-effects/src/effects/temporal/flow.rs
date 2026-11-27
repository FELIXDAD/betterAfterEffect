//! Dense optical flow between two frames, shared by Pixel Motion Blur,
//! Timewarp's Pixel Motion and Pixel Motion frame blending (`flow.wgsl`).
//!
//! DIS-style (Kroeger et al. 2016): a luma pyramid of both frames, then from
//! the coarsest level down, an inverse-compositional patch search seeded by
//! the coarser flow and a per-pixel densification weighted by the match
//! error. The finest level follows Vector Detail (20 % → a quarter of the
//! render size), so the default costs a few milliseconds at 1080p.

use std::{hash::BuildHasher, sync::Arc};

use bae_gpu::wgpu;
use bae_model::{PrimPath, Time};
use bae_render::{FrameKey, GpuImage, IRect, RenderError};

use super::super::image_grid;
use crate::EffectTarget;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};

const LABEL: &str = "bae.effects.flow";
const SHADER: &str = concat!(
    include_str!("flow_sampling.wgsl"),
    include_str!("flow.wgsl")
);
const BYTES: u32 = 224;
const ENTRIES: [&str; 7] = [
    "luma_base",
    "luma_down",
    "search",
    "densify",
    "smooth_vectors",
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
                (6, Binding::Texture),
                (7, Binding::Texture),
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

/// Parameters that affect vector estimation, shared by both optical-flow effects.
#[derive(Clone, Copy)]
pub(crate) struct Analysis {
    weights: [f32; 3],
    global: f32,
    local: f32,
    iterations: u32,
    block: u32,
    correct_luma: bool,
    threshold: f32,
}

impl Default for Analysis {
    fn default() -> Self {
        Self {
            weights: [0.29899994, 0.58699995, 0.11440002],
            global: 0.5,
            local: 0.5,
            iterations: 5,
            block: 6,
            correct_luma: false,
            threshold: 0.01,
        }
    }
}

impl Analysis {
    pub(super) fn timewarp(v: crate::params::Params<'_>) -> Self {
        Self {
            weights: [18, 19, 20].map(|id| (v.f(id) / 100.) as f32),
            global: (v.f(8) / 1000.) as f32,
            local: (v.f(9) / 1000.) as f32,
            iterations: v.f(10).clamp(1., 300.) as u32,
            block: v.f(16).clamp(4., 1000.) as u32,
            correct_luma: v.b(13),
            threshold: (v.f(15) / 100.) as f32,
        }
    }

    fn key(self) -> impl std::hash::Hash {
        (
            self.weights.map(f32::to_bits),
            self.global.to_bits(),
            self.local.to_bits(),
            self.iterations,
            self.block,
            self.correct_luma,
            self.threshold.to_bits(),
        )
    }
}

#[derive(Default)]
struct Call<'a> {
    guides: [Option<&'a GpuImage>; 2],
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
    c: &Call<'_>,
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
    for (i, guide) in c.guides.iter().enumerate() {
        if let Some(image) = guide {
            image_grid::image_coords(&mut w, 160 + 32 * i, image, grid);
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
        .chain(
            c.guides
                .iter()
                .map(|g| g.map_or(Resource::Uniform, |i| Resource::View(&i.view))),
        )
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
struct Pyramid {
    levels: Vec<(GpuImage, [u32; 2])>,
    finest: u32,
    region: IRect,
    grid: f64,
}

/// A flow field: render-pixel motion per texel of the finest level.
pub(crate) struct Field {
    pub(crate) image: GpuImage,
    size: [u32; 2],
    pub(crate) scale: f32,
    pub(crate) region: IRect,
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

fn patches(n: u32, block: u32) -> u32 {
    n.saturating_sub(block).div_ceil(block / 2) + 1
}

/// The luma pyramid of `a` and `b` around `roi` (render grid at `grid`).
/// `detail` is AE's Vector Detail (percent).
fn pyramid(
    target: &mut dyn EffectTarget,
    a: &GpuImage,
    b: &GpuImage,
    region: IRect,
    grid: f64,
    finest: u32,
    analysis: Analysis,
) -> Result<Pyramid, RenderError> {
    let n = 1u32 << finest;
    let [w, h] = region.size();
    let size = [w.div_ceil(n).max(1), h.div_ceil(n).max(1)];
    let base = target.allocate(IRect::from_size(size[0], size[1]), 1.)?;
    let call = Call {
        region,
        size: [size[0], size[1], 0, 0],
        data: [
            n as f32,
            analysis.weights[0],
            analysis.weights[1],
            analysis.weights[2],
        ],
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
fn flow(
    target: &mut dyn EffectTarget,
    pyramid: &Pyramid,
    reverse: bool,
    analysis: Analysis,
) -> Result<Field, RenderError> {
    let mut coarser: Option<(GpuImage, [u32; 2])> = None;
    for (k, (luma, size)) in pyramid.levels.iter().enumerate().rev() {
        let scale = (1u32 << (pyramid.finest + k as u32)) as f32;
        let block = analysis.block.min(size[0].min(size[1])).max(4);
        let grid_n = size.map(|n| patches(n, block));
        let found = target.allocate(IRect::from_size(grid_n[0], grid_n[1]), 1.)?;
        let call = Call {
            size: [size[0], size[1], grid_n[0], grid_n[1]],
            data: [scale, analysis.global, analysis.threshold, block as f32],
            flags: [
                u32::from(reverse),
                u32::from(analysis.correct_luma),
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
        let mut field = target.allocate(IRect::from_size(size[0], size[1]), 1.)?;
        run(
            target,
            "densify",
            pyramid.grid,
            [Some(luma), Some(&found), None, None],
            &call,
            &field,
            *size,
        )?;
        if k == 0 && analysis.local != 0. {
            let mut scratch = target.allocate(IRect::from_size(size[0], size[1]), 1.)?;
            let smooth = Call {
                data: [scale, analysis.local, analysis.threshold, block as f32],
                ..call
            };
            for _ in 0..analysis.iterations {
                run(
                    target,
                    "smooth_vectors",
                    pyramid.grid,
                    [Some(luma), Some(&field), None, None],
                    &smooth,
                    &scratch,
                    *size,
                )?;
                std::mem::swap(&mut field, &mut scratch);
            }
        }
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
impl Pair<'_> {
    pub(super) fn interpolate(
        &self,
        target: &mut dyn EffectTarget,
        frames: [&GpuImage; 2],
        fields: [&Field; 2],
        phase: f32,
        single_image: bool,
        extreme_filter: bool,
    ) -> Result<GpuImage, RenderError> {
        let [a, b] = frames;
        let [ab, ba] = fields;
        let (roi, grid) = (self.roi, self.grid);
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let call = Call {
            guides: self.images.map(Some),
            region: ab.region,
            roi,
            size: [ab.size[0], ab.size[1], 0, 0],
            data: [ab.scale, phase, 0., 0.],
            flags: [u32::from(single_image), u32::from(extreme_filter), 0, 0],
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
}

/// One source-frame pair. The two directions can share the luma pyramid on a miss.
pub(crate) struct Pair<'a> {
    pub images: [&'a GpuImage; 2],
    pub times: [Time; 2],
    pub layer: &'a PrimPath,
    pub roi: IRect,
    pub grid: f64,
    pub detail: f64,
    pub analysis: Analysis,
}

impl Pair<'_> {
    fn field(
        &self,
        target: &mut dyn EffectTarget,
        reverse: bool,
        levels: &mut Option<Pyramid>,
    ) -> Result<Field, RenderError> {
        let region = self.roi.inflate(MARGIN);
        let finest = (100. / self.detail.clamp(5., 100.))
            .log2()
            .round()
            .clamp(0., 4.) as u32;
        let i = usize::from(reverse);
        let signature = |i: usize| {
            let image = self.images[i];
            Some((image.content_key()?, self.times[i]))
        };
        let key = signature(i).zip(signature(1 - i)).map(|pair| FrameKey {
            comp: self.layer.clone(),
            time: self.times[i],
            request: rustc_hash::FxBuildHasher.hash_one((
                LABEL,
                pair,
                region,
                self.grid.to_bits(),
                finest,
                self.analysis.key(),
                target.format(),
            )),
        });
        if let Some(image) = key
            .as_ref()
            .and_then(|key| target.cached_image(key.clone()))
        {
            return Ok(Field {
                size: image.texel_size(),
                image,
                scale: (1u32 << finest) as f32,
                region,
            });
        }
        let pyramid = match levels {
            Some(levels) => levels,
            None => levels.insert(pyramid(
                target,
                self.images[0],
                self.images[1],
                region,
                self.grid,
                finest,
                self.analysis,
            )?),
        };
        let field = flow(target, pyramid, reverse, self.analysis)?;
        if let Some(key) = key {
            target.remember_image(key, field.image.clone());
        }
        Ok(field)
    }

    pub(crate) fn forward(&self, target: &mut dyn EffectTarget) -> Result<Field, RenderError> {
        self.field(target, false, &mut None)
    }

    pub(super) fn both(&self, target: &mut dyn EffectTarget) -> Result<[Field; 2], RenderError> {
        let mut pyramid = None;
        let ab = self.field(target, false, &mut pyramid)?;
        let ba = self.field(target, true, &mut pyramid)?;
        Ok([ab, ba])
    }
}

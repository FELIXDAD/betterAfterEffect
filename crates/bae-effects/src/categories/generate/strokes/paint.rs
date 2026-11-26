//! Bins vector marks in 32-pixel tiles and composites them on the GPU.
use std::sync::Arc;

use bae_gpu::wgpu::{self, util::DeviceExt};
use bae_render::{EffectParams, GpuImage, IRect, RenderError};
use glam::DVec2;

use crate::{
    EffectTarget,
    effects::image_grid,
    gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter},
};

const LABEL: &str = "bae.effects.path_paint";
const TILE: u32 = 32;
const UNIFORM_SIZE: u32 = 128;
/// Style: the Clone Stamp, the dab showing the image at [`Paint::offset`].
pub(super) const CLONE: u32 = 6;
/// Style: the dab restores [`Paint::restore`] (Eraser: Paint Only, Last
/// Stroke Only).
pub(super) const RESTORE: u32 = 7;

pub(super) struct Paint {
    pub marks: Vec<Mark>,
    pub style: u32,
    pub profile: u32,
    pub opacity: f32,
    pub gradient: [f32; 4],
    /// [`CLONE`]: where each dab reads, relative to it (layer pixels).
    pub offset: DVec2,
    /// [`RESTORE`]: the image the dabs bring back.
    pub restore: Option<GpuImage>,
}

pub(super) struct Mark {
    pub start: DVec2,
    pub end: DVec2,
    pub radius: f64,
    pub rgb: [f32; 3],
    pub hardness: f32,
    pub alpha: [f32; 2],
    pub dash_length: f32,
}

impl Mark {
    pub fn dab(center: DVec2, radius: f64, rgb: [f32; 3], hardness: f32, opacity: f32) -> Self {
        Self {
            start: center,
            end: center,
            radius,
            rgb,
            hardness,
            alpha: [opacity; 2],
            dash_length: 0.,
        }
    }
}

impl Paint {
    pub fn new(style: u32, profile: u32, opacity: f32) -> Self {
        Self {
            marks: Vec::new(),
            style,
            profile,
            opacity,
            gradient: [1., 1., 1., 0.5],
            offset: DVec2::ZERO,
            restore: None,
        }
    }

    pub fn render(
        self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        if self.marks.is_empty() && matches!(self.style, 1 | 4) {
            return Ok(input.clone());
        }
        let grid = image_grid::grid_scale(p, input);
        let [width, height] = roi.size().map(|v| v.div_ceil(TILE));
        let mut tiles = vec![Vec::new(); (width * height) as usize];
        let origin = DVec2::new(f64::from(roi.x0), f64::from(roi.y0));
        let mut marks = Vec::new();
        for mark in self.marks {
            if mark.radius <= 0. {
                continue;
            }
            let start = mark.start / grid;
            let end = mark.end / grid;
            let radius = mark.radius / grid;
            let extent = radius + 1.5;
            let bounds = |xy: DVec2| ((xy - origin) / f64::from(TILE)).floor();
            let lo = bounds(start.min(end) - extent)
                .as_ivec2()
                .max(glam::IVec2::ZERO);
            let hi = bounds(start.max(end) + extent)
                .as_ivec2()
                .min(glam::IVec2::new(width as i32 - 1, height as i32 - 1));
            if lo.x > hi.x || lo.y > hi.y {
                continue;
            }
            let index = (marks.len() / 48) as u32;
            let direction = end - start;
            let length_sq = direction.length_squared();
            let tile_size = f64::from(TILE);
            let reach = extent + tile_size * std::f64::consts::FRAC_1_SQRT_2;
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let center =
                        origin + (DVec2::new(f64::from(x), f64::from(y)) + 0.5) * tile_size;
                    let relative = center - start;
                    let t = (relative.dot(direction) / length_sq.max(f64::EPSILON)).clamp(0., 1.);
                    if (relative - t * direction).length_squared() <= reach * reach {
                        tiles[(y as u32 * width + x as u32) as usize].push(index);
                    }
                }
            }
            marks.extend(
                [
                    start.x as f32,
                    start.y as f32,
                    end.x as f32,
                    end.y as f32,
                    mark.rgb[0],
                    mark.rgb[1],
                    mark.rgb[2],
                    radius as f32,
                    mark.alpha[0],
                    mark.alpha[1],
                    mark.hardness,
                    mark.dash_length / grid as f32,
                ]
                .into_iter()
                .flat_map(f32::to_le_bytes),
            );
        }
        if marks.is_empty() {
            marks.resize(48, 0);
        }
        let mut bins = vec![0u32; tiles.len() * 2];
        for (i, tile) in tiles.into_iter().enumerate() {
            bins[2 * i] = bins.len() as u32;
            bins[2 * i + 1] = tile.len() as u32;
            bins.extend(tile);
        }
        let bytes: Vec<_> = bins.into_iter().flat_map(u32::to_le_bytes).collect();
        let rt = gpu::runtime(target.gpu());
        let pipeline = kernel(&rt, target.format())?;
        let buffer = |data: &[u8]| -> Result<_, RenderError> {
            if data.len() as u64 > rt.device.limits().max_storage_buffer_binding_size {
                return Err(RenderError::Other(
                    "paint geometry exceeds the GPU buffer limit".into(),
                ));
            }
            Ok(rt
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(LABEL),
                    contents: data,
                    usage: wgpu::BufferUsages::STORAGE,
                }))
        };
        let marks = buffer(&marks)?;
        let bins = buffer(&bytes)?;
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(UNIFORM_SIZE as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        u.i32s(32, &image_grid::rect4(roi))
            .u32(48, width)
            .u32(52, self.style)
            .u32(56, self.profile | (u32::from(p.float) << 31))
            .f32(60, self.opacity)
            .f32s(64, &self.gradient)
            .f32(80, (self.offset.x / grid) as f32)
            .f32(84, (self.offset.y / grid) as f32);
        let restore = self.restore.as_ref().unwrap_or(input);
        image_grid::image_coords(&mut u, 96, restore, grid);
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &pipeline,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&out.view),
                Resource::Buffer(&marks),
                Resource::Buffer(&bins),
                Resource::View(&restore.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &pipeline, &group, &[offset], roi.size());
        Ok(out)
    }
}

pub(super) fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
) -> Result<Arc<Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: LABEL.into(),
            entry: "main".into(),
            variant: gpu::format_key(format),
        },
        || {
            Ok(KernelSource {
                label: LABEL,
                wgsl: [
                    image_grid::CHANNEL_MATH,
                    include_str!("../../../shaders/image_grid.wgsl"),
                    include_str!("dash.wgsl"),
                    include_str!("paint.wgsl"),
                ]
                .concat()
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
                entry: "main",
                bindings: vec![
                    (0, Binding::Uniform(UNIFORM_SIZE)),
                    (1, Binding::Texture),
                    (2, Binding::Storage(format)),
                    (3, Binding::ReadBuffer),
                    (4, Binding::ReadBuffer),
                    (5, Binding::Texture),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}

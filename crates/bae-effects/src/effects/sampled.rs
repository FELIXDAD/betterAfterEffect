//! A layer-space compute pass for effects with auxiliary images. Each parameter
//! occupies its AE disk-ID slot. This keeps the CPU and WGSL layouts identical.
use super::image_grid;
use crate::{
    EffectTarget,
    gpu::{self, Resource, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_model::Value;
use bae_render::{GpuImage, IRect, RenderError};

const SLOTS: usize = 233;
const HEADER: usize = 192;
const BYTES: u32 = (HEADER + SLOTS * 16) as u32;

pub(super) struct Shader {
    pub label: &'static str,
    pub source: &'static str,
}

impl Shader {
    pub(super) fn kernel(
        &self,
        rt: &gpu::Runtime,
        format: wgpu::TextureFormat,
    ) -> Result<std::sync::Arc<gpu::Kernel>, RenderError> {
        image_grid::kernel_with_inputs(rt, format, self.label, self.source, BYTES, 4)
    }

    pub fn dispatch(
        &self,
        target: &mut dyn EffectTarget,
        v: Params<'_>,
        roi: IRect,
        input: &GpuImage,
        aux: [Option<&GpuImage>; 3],
        stage: [u32; 2],
    ) -> Result<GpuImage, RenderError> {
        let rt = gpu::runtime(target.gpu());
        let kernel = self.kernel(&rt, target.format())?;
        let grid = image_grid::grid_scale(v.p, input);
        let layer = if v.p.layer_bounds.is_empty() {
            image_grid::pixel_rect(input.rect, 1. / grid)
        } else {
            image_grid::layer_rect(v.p)
        };
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let mut u = UniformWriter::with_size(BYTES as usize);
        let mut presence = 0;
        let maps = std::array::from_fn::<_, 3, _>(|i| {
            if aux[i].is_some() {
                presence |= 1 << i;
            }
            aux[i].unwrap_or(input)
        });
        for (index, image) in std::iter::once(input).chain(maps).enumerate() {
            image_grid::image_coords(&mut u, index * 32, image, grid);
        }
        u.i32s(128, &image_grid::rect4(roi))
            .i32s(144, &image_grid::rect4(layer))
            .u32(160, presence)
            .u32(164, stage[0])
            .u32(168, stage[1])
            .u32(172, u32::from(v.p.float))
            .f32(176, v.p.scale as f32)
            .f32(180, v.p.time.as_seconds() as f32)
            .f32(184, (1.0 / v.p.time_step.as_seconds()) as f32);
        for p in &v.def.params {
            let i = p.id as usize;
            if i >= SLOTS {
                continue;
            }
            let offset = HEADER + i * 16;
            match v.value(p.id) {
                Some(Value::Color(c)) => {
                    u.f32s(offset, &c.map(|x| x as f32));
                }
                Some(Value::Vec2(xy)) => {
                    let point = if v.p.value(p.id).is_some() {
                        xy.map(|x| (x * v.p.scale) as f32)
                    } else {
                        [
                            (f64::from(layer.x0) + xy[0] * f64::from(layer.width()) / 640.) as f32,
                            (f64::from(layer.y0) + xy[1] * f64::from(layer.height()) / 360.) as f32,
                        ]
                    };
                    u.f32s(offset, &point);
                }
                Some(Value::Int(_) | Value::Bool(_) | Value::Float(_)) => {
                    u.f32(offset, v.f(p.id) as f32);
                }
                _ => {}
            }
        }
        let offset = gpu::push_uniform(&rt, &u);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::View(&maps[0].view),
                Resource::View(&maps[1].view),
                Resource::View(&maps[2].view),
                Resource::View(&out.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(self.label),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], roi.size());
        Ok(out)
    }
}

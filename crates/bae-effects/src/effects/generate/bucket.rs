use super::*;
use gpu::{Binding, Kernel, KernelKey, KernelSource};
use std::sync::Arc;

pub(super) struct PaintBucket {
    pub(super) def: EffectDef,
}
const LABEL: &str = "bae.effects.paint-bucket";
const BYTES: u32 = 128;
const STAGES: [&str; 5] = ["classify", "connect", "boundary", "jump", "paint"];

fn kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
    entry: &'static str,
) -> Result<Arc<Kernel>, RenderError> {
    rt.kernel(
        &KernelKey {
            owner: LABEL.into(),
            entry: entry.into(),
            variant: gpu::format_key(format),
        },
        || {
            let wgsl = [
                image_grid::CHANNEL_MATH,
                include_str!("../../shaders/image_grid.wgsl"),
                include_str!("../../shaders/generated_blend.wgsl"),
                include_str!("bucket.wgsl"),
            ]
            .concat()
            .replace("bae_unpremultiply", "fx_unpremult")
            .replace("WORKING_FORMAT", crate::blur::format_name(format)?);
            Ok(KernelSource {
                label: LABEL,
                wgsl,
                entry,
                bindings: vec![
                    (0, Binding::Uniform(BYTES)),
                    (1, Binding::Texture),
                    (2, Binding::StorageBuffer),
                    (3, Binding::StorageBuffer),
                    (4, Binding::StorageBuffer),
                    (5, Binding::Storage(format)),
                ],
                workgroup: [8, 8, 1],
            })
        },
    )
}
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for entry in STAGES {
        kernel(rt, format, entry)?;
    }
    Ok(())
}
impl Effect for PaintBucket {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn input_roi(&self, p: &EffectParams<'_>, roi: IRect) -> IRect {
        image_grid::layer_rect(p).union(&roi)
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
impl TargetEffect for PaintBucket {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let v = Params::new(&self.def, p);
        let grid = image_grid::grid_scale(p, input);
        let rect = image_grid::pixel_rect(input.rect, 1. / grid).union(&roi);
        if rect.is_empty() {
            return Ok(input.clone());
        }
        let size = rect.size();
        let bytes = u64::from(size[0]) * u64::from(size[1]) * 4;
        let rt = gpu::runtime(target.gpu());
        if bytes > rt.device.limits().max_storage_buffer_binding_size {
            return Err(RenderError::Other(
                "Paint Bucket component map exceeds GPU storage limit".into(),
            ));
        }
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let buffer = |size| {
            rt.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(LABEL),
                size,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        let parents = buffer(bytes);
        let stroke = v.popup(8);
        let radius = (v.f(5) * p.scale) as f32;
        let distance = stroke > 1 && radius > 0.0 && !v.b(7);
        let scratch_size = if distance { bytes } else { 4 };
        let mut from = buffer(scratch_size);
        let mut to = buffer(scratch_size);
        let mut u = UniformWriter::with_size(BYTES as usize);
        image_grid::image_coords(&mut u, 0, input, grid);
        u.i32s(32, &image_grid::rect4(rect))
            .i32s(48, &image_grid::rect4(roi))
            .f32s(64, &color(v, 1))
            .f32s(80, &point(v, 2))
            .f32(88, v.f(3) as f32 / 255.)
            .f32(92, radius)
            .u32(96, v.popup(4) as u32)
            .u32(100, stroke as u32)
            .u32(104, v.popup(11) as u32)
            .u32(
                108,
                u32::from(v.b(9))
                    | u32::from(v.b(7)) << 1
                    | u32::from(p.float) << 2
                    | u32::from(distance) << 3,
            )
            .f32(112, v.f(10) as f32 * 0.01);
        let format = target.format();
        let mut run = |stage, step: u32, from: &wgpu::Buffer, to: &wgpu::Buffer, size| {
            u.u32(116, step);
            let kernel = kernel(&rt, format, stage)?;
            let group = rt.bind_group(
                &kernel,
                &[
                    Resource::Uniform,
                    Resource::View(&input.view),
                    Resource::Buffer(&parents),
                    Resource::Buffer(from),
                    Resource::Buffer(to),
                    Resource::View(&out.view),
                ],
            );
            let mut pass = target
                .encoder()
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(stage),
                    timestamp_writes: None,
                });
            gpu::dispatch(
                &mut pass,
                &kernel,
                &group,
                &[gpu::push_uniform(&rt, &u)],
                size,
            );
            Ok::<_, RenderError>(())
        };
        run("classify", 0, &from, &to, size)?;
        run("connect", 0, &from, &to, size)?;
        if distance {
            run("boundary", 0, &to, &from, size)?;
            let mut step = (radius.ceil() as u32 + 1).next_power_of_two();
            while step > 0 {
                run("jump", step, &from, &to, size)?;
                std::mem::swap(&mut from, &mut to);
                step /= 2;
            }
        }
        run("paint", 0, &from, &to, roi.size())?;
        Ok(out)
    }
}

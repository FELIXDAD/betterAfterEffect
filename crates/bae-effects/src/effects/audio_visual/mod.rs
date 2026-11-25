//! Shared drawing code for Audio Spectrum and Audio Waveform.
mod geometry;
use crate::{
    Builtin, EffectTarget, TargetEffect,
    catalog::DefBuilder,
    common,
    gpu::{self, Binding, KernelKey, KernelSource, Resource, UniformWriter},
    params::Params,
    rust_builtin,
};
use bae_gpu::{GpuContext, wgpu};
use bae_model::{EffectDef, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use parking_lot::Mutex;
use std::sync::{Arc, Weak};

struct Display(EffectDef);

/// Geometry buffers of one GPU context, reused across frames. Each draw takes
/// the next one (grown when too small) and fills it with `queue.write_buffer`.
/// Those writes all land before the next submission, so draws encoded into the
/// same submission must not share a buffer.
#[derive(Default)]
struct GeometryPool {
    buffers: [Option<wgpu::Buffer>; 4],
    next: usize,
    bytes: Vec<u8>,
}

static GEOMETRY: Mutex<Vec<(Weak<GpuContext>, GeometryPool)>> = Mutex::new(Vec::new());

fn geometry_buffer(gpu: &Arc<GpuContext>, nodes: &[[f32; 12]]) -> wgpu::Buffer {
    let mut pools = GEOMETRY.lock();
    pools.retain(|(ctx, _)| ctx.strong_count() > 0);
    let i = match pools
        .iter()
        .position(|(ctx, _)| std::ptr::eq(ctx.as_ptr(), Arc::as_ptr(gpu)))
    {
        Some(i) => i,
        None => {
            pools.push((Arc::downgrade(gpu), GeometryPool::default()));
            pools.len() - 1
        }
    };
    let pool = &mut pools[i].1;
    pool.bytes.clear();
    pool.bytes
        .extend(nodes.iter().flatten().flat_map(|v| v.to_le_bytes()));
    // A binding needs at least one node.
    pool.bytes.resize(pool.bytes.len().max(48), 0);
    let size = pool.bytes.len() as u64;
    let slot = &mut pool.buffers[pool.next];
    pool.next = (pool.next + 1) % 4;
    let buffer = match slot {
        Some(buffer) if buffer.size() >= size => buffer.clone(),
        _ => slot
            .insert(gpu.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("audio display geometry"),
                size: size.next_power_of_two(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }))
            .clone(),
    };
    gpu.queue().write_buffer(&buffer, 0, &pool.bytes);
    buffer
}

fn draw_kernel(
    rt: &gpu::Runtime,
    format: wgpu::TextureFormat,
) -> Result<Arc<gpu::Kernel>, RenderError> {
    let key = KernelKey {
        owner: "bae.effects/audio-visual".into(),
        entry: "draw".into(),
        variant: gpu::format_key(format),
    };
    rt.kernel(&key, || {
        Ok(KernelSource {
            label: "bae.effects.audio_visual",
            wgsl: include_str!("display.wgsl")
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
            entry: "draw",
            bindings: vec![
                (0, Binding::Uniform(96)),
                (1, Binding::Texture),
                (2, Binding::ReadBuffer),
                (3, Binding::Storage(format)),
            ],
            workgroup: [8, 8, 1],
        })
    })
}

impl Effect for Display {
    fn def(&self) -> &EffectDef {
        &self.0
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
impl TargetEffect for Display {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let out = input.rect.intersect(&roi);
        if out.is_empty() {
            return Ok(input.clone());
        }
        let spec = self.0.match_name == "ADBE AudSpect";
        let p = Params::new(&self.0, p);
        let radius = (p.f(if spec { 12 } else { 9 }) * p.p.scale * 0.5) as f32;
        let nodes = if radius > 0.0 {
            geometry::nodes(p, spec, radius)
        } else {
            Vec::new()
        };
        let input = common::premultiplied(target, input)?;
        let output = target.allocate(out, input.pixel_scale)?;
        let format = target.format();
        let rt = gpu::runtime(target.gpu());
        let kernel = draw_kernel(&rt, format)?;
        let mut w = UniformWriter::with_size(96);
        for (i, r) in [input.rect, out].iter().enumerate() {
            w.i32s(i * 16, &[r.x0, r.y0, r.x1, r.y1]);
        }
        for (offset, id) in [
            (32, if spec { 14 } else { 12 }),
            (48, if spec { 15 } else { 13 }),
        ] {
            if let Some(Value::Color(rgb)) = p.value(id) {
                w.f32s(offset, &rgb.map(|v| v as f32));
            }
        }
        w.f32(64, radius)
            .f32(68, p.f(if spec { 13 } else { 10 }) as f32)
            .u32(72, u32::from(p.b(if spec { 23 } else { 16 })))
            .u32(76, u32::from(spec && p.b(16)))
            .u32(80, nodes.len() as u32);
        let offset = gpu::push_uniform(&rt, &w);
        let data = geometry_buffer(target.gpu(), &nodes);
        let group = rt.bind_group(
            &kernel,
            &[
                Resource::Uniform,
                Resource::View(&input.view),
                Resource::Buffer(&data),
                Resource::View(&output.view),
            ],
        );
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("audio display"),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &kernel, &group, &[offset], out.size());
        drop(pass);
        Ok(output)
    }
}

fn builtin(slug: &str, zh: &str, builder: DefBuilder, fixed: &[u32]) -> Builtin {
    let (mut def, names) = builder.build();
    for p in &mut def.params {
        if fixed.contains(&p.id) {
            p.flags.cannot_time_vary = true;
            p.flags.cannot_animate = true;
        }
    }
    rust_builtin((Arc::new(Display(def)), names), slug, zh, &["audio", zh])
}

mod spectrum;
mod waveform;
pub(crate) fn register(registry: &mut crate::Registry) {
    registry.prewarm.push(|rt, format| {
        bae_render::anim::prewarm_audio_analysis();
        draw_kernel(rt, format).map(drop)
    });
    registry
        .effects
        .extend([waveform::effect(), spectrum::effect()]);
}

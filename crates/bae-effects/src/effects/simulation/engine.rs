use super::{Kind, Scene, surface};
use crate::{EffectTarget, gpu};
use bae_gpu::wgpu::{self, util::DeviceExt};
use bae_render::{GpuImage, IRect, RenderError};
use std::collections::VecDeque;

const CHECKPOINT_FRAMES: f64 = 30.;
const CACHE_BYTES: u64 = 64 << 20;
const STATE_BYTES: u64 = 32;
const SPRITE_BYTES: u64 = 64;
const LABEL: &str = "bae.effects.particles";

struct Checkpoint {
    key: Vec<u8>,
    config: wgpu::Buffer,
    state: wgpu::Buffer,
}

pub(crate) struct Engine {
    checkpoint: wgpu::ComputePipeline,
    advance: [wgpu::ComputePipeline; 4],
    draw: [wgpu::RenderPipeline; 5],
    checkpoints: VecDeque<Checkpoint>,
    scratch: Option<wgpu::Buffer>,
}

fn buffer_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}
fn view_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}
fn group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    entries: &[wgpu::BindGroupEntry<'_>],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(LABEL),
        layout,
        entries,
    })
}

fn compute(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    entry: &str,
    layout: Option<&wgpu::PipelineLayout>,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout,
        module,
        entry_point: Some(entry),
        compilation_options: Default::default(),
        cache: None,
    })
}

// Image bindings accept both working formats, including unfilterable RGBA32F.
fn layout(
    device: &wgpu::Device,
    entries: &[(u32, wgpu::ShaderStages, wgpu::BindingType)],
) -> wgpu::PipelineLayout {
    let entries: Vec<_> = entries
        .iter()
        .map(|(binding, visibility, ty)| wgpu::BindGroupLayoutEntry {
            binding: *binding,
            visibility: *visibility,
            ty: *ty,
            count: None,
        })
        .collect();
    let group = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(LABEL),
        entries: &entries,
    });
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(LABEL),
        bind_group_layouts: &[Some(&group)],
        immediate_size: 0,
    })
}

impl Engine {
    fn get<'a>(
        slot: &'a mut Option<Self>,
        device: &wgpu::Device,
    ) -> Result<&'a mut Self, RenderError> {
        match slot {
            Some(engine) => Ok(engine),
            None => Ok(slot.insert(Self::new(device)?)),
        }
    }

    fn new(device: &wgpu::Device) -> Result<Self, RenderError> {
        use wgpu::ShaderStages as Stage;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let source = concat!(
            include_str!("shared.wgsl"),
            include_str!("motion.wgsl"),
            include_str!("sprites.wgsl"),
            include_str!("tiles.wgsl"),
            include_str!("draw.wgsl")
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(LABEL),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let buf = |ty| wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let tex = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let uniform = buf(wgpu::BufferBindingType::Uniform);
        let viewport = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: std::num::NonZeroU64::new(224),
        };
        let write = buf(wgpu::BufferBindingType::Storage { read_only: false });
        let read = buf(wgpu::BufferBindingType::Storage { read_only: true });
        let checkpoint = compute(device, &module, "checkpoint", None);
        let mut entries = vec![
            (0, Stage::COMPUTE, uniform),
            (2, Stage::COMPUTE, read),
            (3, Stage::COMPUTE, write),
            (4, Stage::COMPUTE, viewport),
        ];
        entries.extend((5..=10).map(|n| (n, Stage::COMPUTE, tex)));
        let advance_layout = layout(device, &entries);
        let advance = [
            "advance_emitter",
            "advance_weather",
            "advance_drizzle",
            "advance_tiles",
        ]
        .map(|entry| compute(device, &module, entry, Some(&advance_layout)));
        let draw_layout = layout(
            device,
            &[
                (0, Stage::FRAGMENT, uniform),
                (4, Stage::VERTEX_FRAGMENT, viewport),
                (5, Stage::FRAGMENT, tex),
                (6, Stage::FRAGMENT, tex),
                (7, Stage::FRAGMENT, tex),
                (8, Stage::FRAGMENT, tex),
                (9, Stage::FRAGMENT, tex),
                (10, Stage::FRAGMENT, tex),
            ],
        );
        let attributes =
            wgpu::vertex_attr_array![0=>Float32x4,1=>Float32x4,2=>Float32x4,3=>Float32x4];
        let over = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let screen = wgpu::BlendComponent {
            dst_factor: wgpu::BlendFactor::OneMinusSrc,
            ..add
        };
        let lighten = wgpu::BlendComponent {
            operation: wgpu::BlendOperation::Max,
            ..add
        };
        let draw = [
            over,
            wgpu::BlendState {
                color: screen,
                alpha: over.alpha,
            },
            wgpu::BlendState {
                color: add,
                alpha: over.alpha,
            },
            wgpu::BlendState {
                color: add,
                alpha: add,
            },
            wgpu::BlendState {
                color: lighten,
                alpha: over.alpha,
            },
        ]
        .map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(LABEL),
                layout: Some(&draw_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: SPRITE_BYTES,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Other(format!("{LABEL}: {err}")));
        }
        Ok(Self {
            checkpoint,
            advance,
            draw,
            checkpoints: VecDeque::new(),
            scratch: None,
        })
    }

    fn snapshot(
        &mut self,
        rt: &gpu::Runtime,
        key: Vec<u8>,
        count: u32,
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        if let Some(i) = self.checkpoints.iter().position(|c| c.key == key) {
            let c = self.checkpoints.remove(i).unwrap();
            let buffers = (c.config.clone(), c.state.clone());
            self.checkpoints.push_back(c);
            return buffers;
        }
        let config = rt
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("particle controls"),
                contents: &key,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let size = u64::from(count.max(1)) * STATE_BYTES;
        let state = rt.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle checkpoint"),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let bind = group(
            &rt.device,
            &self.checkpoint.get_bind_group_layout(0),
            &[buffer_entry(0, &config), buffer_entry(1, &state)],
        );
        // Publish only submitted, immutable checkpoints. Independently submitted
        // preview/export encoders may consume the same snapshot in either order.
        let mut encoder = rt.device.create_command_encoder(&Default::default());
        dispatch(&mut encoder, &self.checkpoint, &bind, count, &[]);
        rt.queue.submit([encoder.finish()]);
        while !self.checkpoints.is_empty()
            && (self.checkpoints.len() >= 16
                || self.checkpoints.iter().map(|c| c.state.size()).sum::<u64>() + size
                    > CACHE_BYTES)
        {
            self.checkpoints.pop_front();
        }
        self.checkpoints.push_back(Checkpoint {
            key,
            config: config.clone(),
            state: state.clone(),
        });
        (config, state)
    }
}

fn dispatch(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    bind: &wgpu::BindGroup,
    count: u32,
    offsets: &[u32],
) {
    if count == 0 {
        return;
    }
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some(LABEL),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, offsets);
    pass.dispatch_workgroups(count.div_ceil(128), 1, 1);
}

pub(crate) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    Engine::get(&mut rt.particles.lock(), &rt.device)?;
    surface::composite_kernel(rt, format)?;
    Ok(())
}

pub(super) fn render(
    kind: Kind,
    target: &mut dyn EffectTarget,
    scene: &Scene<'_>,
    input: &GpuImage,
    roi: IRect,
    maps: &[Option<GpuImage>],
) -> Result<GpuImage, RenderError> {
    let rt = gpu::runtime(target.gpu());
    let p = scene.p;
    let time = p.p.time.as_seconds().max(0.);
    let fps = p.p.frame_rate.as_f64();
    let anchor = ((time * fps + 1e-8) / CHECKPOINT_FRAMES).floor() * CHECKPOINT_FRAMES / fps;
    let seed = p.p.seed as u32
        ^ match kind {
            Kind::World => p.f(104) as u32,
            Kind::Systems => p.f(30) as u32,
            Kind::Rain => p.f(22) as u32,
            Kind::Snow => p.f(31) as u32,
            _ => 0,
        };
    let max = (rt.device.limits().max_storage_buffer_binding_size / SPRITE_BYTES)
        .min(u64::from(rt.device.limits().max_compute_workgroups_per_dimension) * 128)
        as u32;
    let advance = match kind {
        Kind::World | Kind::Systems => 0,
        Kind::Rain | Kind::Snow | Kind::Bubbles => 1,
        Kind::Drizzle => 2,
        _ => 3,
    };
    let total = scene.count(kind);
    let stateful = matches!(
        kind,
        Kind::World | Kind::Systems | Kind::Rain | Kind::Snow | Kind::Bubbles | Kind::Drizzle
    );
    let blend = match kind {
        Kind::World => {
            if p.popup(39) == 4 {
                1
            } else {
                p.popup(39)
            }
        }
        Kind::Systems => {
            if p.popup(28) == 4 {
                1
            } else {
                p.popup(28)
            }
        }
        Kind::Scatter => p.popup(4),
        Kind::Rain if p.popup(15) == 2 => 5,
        Kind::Snow if p.popup(25) == 2 => 5,
        _ => 1,
    }
    .clamp(1, 5) as usize
        - 1;
    let mut slot = rt.particles.lock();
    let engine = Engine::get(&mut slot, &rt.device)?;
    let advance = engine.advance[advance].clone();
    let frame = surface::frame(scene, input, roi, maps);
    let viewport_offset = rt.uniform(&frame.bytes);
    let viewport = wgpu::BindGroupEntry {
        binding: 4,
        resource: rt.uniform_binding(frame.bytes.len() as u64),
    };
    let mut textures = vec![view_entry(5, &input.view)];
    for i in 0..5 {
        textures.push(view_entry(
            6 + i as u32,
            &maps.get(i).and_then(Option::as_ref).unwrap_or(input).view,
        ));
    }
    let overlay = if target.format() == wgpu::TextureFormat::Rgba16Float {
        target.allocate(roi, input.pixel_scale)?
    } else {
        let texture = rt.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("particle blend target"),
            size: wgpu::Extent3d {
                width: roi.width(),
                height: roi.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mut image = GpuImage::from_texture(texture, bae_render::ImageAlpha::Premultiplied);
        image.rect = roi;
        image.pixel_scale = input.pixel_scale;
        image
    };
    // Split large image grids at the device storage-binding limit. The source
    // index is global, so fragments at the bottom of a 4K layer are retained.
    let mut start = 0;
    loop {
        let count = (total - start).min(max);
        let mut controls =
            scene.controls(kind, count, seed, if stateful { anchor as f32 } else { 0. });
        controls.u32(24, start).u32(28, total);
        let (config, state) =
            engine.snapshot(&rt, controls.bytes, if stateful { count } else { 0 });
        let size = u64::from(count.max(1)) * SPRITE_BYTES;
        let scratch = engine
            .scratch
            .get_or_insert_with(|| storage(&rt.device, size));
        if scratch.size() < size {
            *scratch = storage(&rt.device, size);
        }
        let mut entries = vec![
            buffer_entry(0, &config),
            buffer_entry(2, &state),
            buffer_entry(3, scratch),
            viewport.clone(),
        ];
        entries.extend_from_slice(&textures);
        let bind = group(&rt.device, &advance.get_bind_group_layout(0), &entries);
        dispatch(target.encoder(), &advance, &bind, count, &[viewport_offset]);
        let mut entries = vec![buffer_entry(0, &config), viewport.clone()];
        entries.extend_from_slice(&textures);
        let bind = group(
            &rt.device,
            &engine.draw[blend].get_bind_group_layout(0),
            &entries,
        );
        let mut pass = target
            .encoder()
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(LABEL),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &overlay.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if start == 0 {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        pass.set_pipeline(&engine.draw[blend]);
        pass.set_bind_group(0, &bind, &[viewport_offset]);
        pass.set_vertex_buffer(0, scratch.slice(..));
        pass.set_viewport(0., 0., roi.width() as f32, roi.height() as f32, 0., 1.);
        pass.draw(0..4, 0..count);
        drop(pass);
        start += count;
        if start == total {
            break;
        }
    }
    drop(slot);
    surface::composite(kind, target, scene, input, &overlay)
}
fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle sprites"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_warmup_prepares_particles_before_the_first_frame() {
        let Some(gpu) = bae_gpu::testing::shared_context() else {
            return;
        };
        let rt = gpu::runtime(&gpu);
        crate::prewarm(&gpu, wgpu::TextureFormat::Rgba16Float).unwrap();
        let particles = rt.particles.lock();
        let engine = particles.as_ref().expect("registered particle warm-up");
        assert!(engine.checkpoints.is_empty());
        assert!(engine.scratch.is_none());
    }
}

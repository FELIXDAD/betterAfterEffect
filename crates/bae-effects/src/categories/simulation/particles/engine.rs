use super::{Kind, Scene, emission, surface};
use crate::{EffectTarget, gpu};
use bae_gpu::wgpu::{self, util::DeviceExt as _};
use bae_render::{GpuImage, IRect, RenderError};

const SPRITE_BYTES: u64 = 64;
const LABEL: &str = "bae.effects.particles";

pub(crate) struct Engine {
    advance: [wgpu::ComputePipeline; 8],
    draw: [wgpu::RenderPipeline; 5],
    scratch: wgpu::Buffer,
    unused_intervals: wgpu::Buffer,
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
    fn get(rt: &gpu::Runtime) -> Result<&Self, RenderError> {
        // Interactive frames may arrive before the registered warm-up has run.
        let result = if bae_render::is_interactive_thread() {
            rt.particles
                .get()
                .ok_or_else(|| bae_render::warming_up("the particle pipelines"))?
        } else {
            rt.particles
                .get_or_init(|| Self::new(&rt.device).map_err(|e| e.to_string()))
        };
        result.as_ref().map_err(|e| RenderError::Other(e.clone()))
    }

    fn new(device: &wgpu::Device) -> Result<Self, RenderError> {
        use wgpu::ShaderStages as Stage;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let source = concat!(
            include_str!("shared.wgsl"),
            include_str!("motion.wgsl"),
            include_str!("sprites.wgsl"),
            include_str!("tiles.wgsl"),
            include_str!(concat!(env!("OUT_DIR"), "/playground_slots.wgsl")),
            include_str!("playground.wgsl"),
            include_str!("surfaces.wgsl"),
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
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: None,
        };
        let viewport = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: std::num::NonZeroU64::new(272),
        };
        let write = buf(wgpu::BufferBindingType::Storage { read_only: false });
        let mut entries = vec![
            (0, Stage::COMPUTE, uniform),
            (
                1,
                Stage::COMPUTE,
                buf(wgpu::BufferBindingType::Storage { read_only: true }),
            ),
            (3, Stage::COMPUTE, write),
            (4, Stage::COMPUTE, viewport),
        ];
        entries.extend((5..=15).map(|n| (n, Stage::COMPUTE, tex)));
        let advance_layout = layout(device, &entries);
        let advance = [
            "advance_emitter",
            "advance_weather",
            "advance_drizzle",
            "advance_tiles",
            "advance_ball",
            "advance_hair",
            "advance_mercury",
            "advance_playground",
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
            advance,
            draw,
            // A single interval travels inline in Frame; the table binding is unused.
            unused_intervals: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("particle inline interval placeholder"),
                size: emission::INTERVAL_BYTES as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }),
            scratch: storage(
                device,
                (device.limits().max_storage_buffer_binding_size / SPRITE_BYTES)
                    .min(u64::from(device.limits().max_compute_workgroups_per_dimension) * 128)
                    .min(65_536)
                    * SPRITE_BYTES,
            ),
        })
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
    Engine::get(rt)?;
    surface::composite_kernel(rt, format)?;
    super::scatter::prewarm(rt, format)?;
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
    let seed = p.p.seed as u32
        ^ match kind {
            Kind::World => p.f(104) as u32,
            Kind::Systems => p.f(30) as u32,
            Kind::Rain => p.f(22) as u32,
            Kind::Snow => p.f(31) as u32,
            _ => 0,
        };
    let advance = match kind {
        Kind::World | Kind::Systems => 0,
        Kind::Rain | Kind::Snow | Kind::Bubbles => 1,
        Kind::Drizzle => 2,
        Kind::Ball => 4,
        Kind::Hair => 5,
        Kind::Mercury => 6,
        Kind::Playground => 7,
        _ => 3,
    };
    let batches = super::emission::batches(kind, scene);
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
        Kind::Mercury => 4,
        Kind::Rain if p.popup(15) == 2 => 5,
        Kind::Snow if p.popup(25) == 2 => 5,
        _ => 1,
    }
    .clamp(1, 5) as usize
        - 1;
    let engine = Engine::get(&rt)?;
    let advance = engine.advance[advance].clone();
    let mut frame = surface::frame(scene, input, roi, maps);
    let controls = scene.controls(kind, seed);
    let config_offset = rt.uniform(&controls.bytes);
    let config = wgpu::BindGroupEntry {
        binding: 0,
        resource: rt.uniform_binding(controls.bytes.len() as u64),
    };
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
    let overlay =
        target.allocate_format(roi, input.pixel_scale, wgpu::TextureFormat::Rgba16Float)?;
    // Split large image grids at the device storage-binding limit. The source
    // index is global, so fragments at the bottom of a 4K layer are retained.
    let scratch = &engine.scratch;
    let max = (scratch.size() / SPRITE_BYTES) as u32;
    let mut compute_entries = vec![
        config.clone(),
        buffer_entry(1, &engine.unused_intervals),
        buffer_entry(3, scratch),
        viewport.clone(),
    ];
    compute_entries.extend_from_slice(&textures);
    for i in 5..10 {
        compute_entries.push(view_entry(
            6 + i as u32,
            &maps.get(i).and_then(Option::as_ref).unwrap_or(input).view,
        ));
    }
    let compute_layout = advance.get_bind_group_layout(0);
    let inline_bind = group(&rt.device, &compute_layout, &compute_entries);
    let mut entries = vec![config, viewport];
    entries.extend_from_slice(&textures);
    let draw_bind = group(
        &rt.device,
        &engine.draw[blend].get_bind_group_layout(0),
        &entries,
    );
    // These emitters consume one inline birth interval per draw, including animated rates.
    let group_size = if matches!(kind, Kind::Mercury | Kind::Playground) {
        1
    } else {
        usize::MAX
    };
    let pages = batches
        .chunks(group_size)
        .flat_map(|group| emission::pages(group, max));
    for (page_index, page) in pages.enumerate() {
        let count = page.count;
        let records = &page.intervals.bytes;
        let interval_count = records.len() / emission::INTERVAL_BYTES;
        frame.u32(224, count).u32(228, interval_count as u32);
        let table_bind;
        let compute_bind = if interval_count == 1 {
            frame.bytes[240..272].copy_from_slice(records);
            &inline_bind
        } else {
            // Immutable for this submission, including when several effects
            // are encoded before the queue is submitted.
            let table = rt
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("particle birth intervals"),
                    contents: records,
                    usage: wgpu::BufferUsages::STORAGE,
                });
            let mut entries = compute_entries.clone();
            entries[1] = buffer_entry(1, &table);
            table_bind = group(&rt.device, &compute_layout, &entries);
            &table_bind
        };
        let offsets = [config_offset, rt.uniform(&frame.bytes)];
        dispatch(target.encoder(), &advance, compute_bind, count, &offsets);
        let mut pass = target
            .encoder()
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(LABEL),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &overlay.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if page_index == 0 {
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
        pass.set_bind_group(0, &draw_bind, &offsets);
        pass.set_vertex_buffer(0, scratch.slice(..));
        pass.set_viewport(0., 0., roi.width() as f32, roi.height() as f32, 0., 1.);
        pass.draw(0..4, 0..count);
    }
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
        let engine = rt
            .particles
            .get()
            .expect("registered particle warm-up")
            .as_ref()
            .unwrap();
        assert!(engine.scratch.size() >= 50_000 * SPRITE_BYTES);
    }
}

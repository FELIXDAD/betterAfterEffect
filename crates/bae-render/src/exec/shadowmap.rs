//! Receiver-space shadow maps ([`crate::graph::ShadowMapNode`],
//! `gpu/shadowmap.wgsl`): one draw over a strip of slots, one per
//! shadow-casting light, testing the receiver's casters per map texel with
//! the same code as the composite pass. The renderer keeps the maps across
//! frames ([`ShadowMapCache`]): a camera move or an edit elsewhere reuses
//! them.

use bae_gpu::wgpu;
use rustc_hash::FxHashMap;

use crate::source::GpuImage;

/// The shadow maps' texel format (how much of each light gets through).
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The shadow-map pipeline, created on first use or by the warm-up.
pub(crate) struct ShadowMapPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    dummy: wgpu::TextureView,
}

impl ShadowMapPipeline {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.shadowmap"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("../gpu/shadowmap.wgsl"),
                    include_str!("../gpu/shadow.wgsl")
                )
                .into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.shadowmap"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.shadowmap"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bae-render.shadowmap"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_map"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.shadowmap"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.shadowmap"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        Self {
            layout,
            pipeline,
            sampler,
            dummy,
        }
    }
}

/// Where a shadow map's slots sit and which light each holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MapLayout {
    /// Layer pixels covered.
    pub rect: [f32; 4],
    /// Slot size in texels.
    pub slot: [u32; 2],
    /// The light (index in the receiver's lighting record) of each slot.
    pub lights: [u32; 8],
    pub count: u32,
}

/// Records the shadow map of `record` (its lighting record, packed in layer
/// pixels) into `target`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    pipeline: &mut Option<ShadowMapPipeline>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    target: &GpuImage,
    layout: &MapLayout,
    record: &[[f32; 4]],
    atlas: Option<&wgpu::TextureView>,
) {
    let p = pipeline.get_or_insert_with(|| ShadowMapPipeline::new(device));
    let mut params: Vec<u8> = Vec::with_capacity(64);
    for v in layout.rect {
        params.extend(v.to_le_bytes());
    }
    for v in [
        layout.slot[0] as f32,
        layout.slot[1] as f32,
        layout.count as f32,
        0.0,
    ] {
        params.extend(v.to_le_bytes());
    }
    for v in layout.lights {
        params.extend(v.to_le_bytes());
    }
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bae-render.shadowmap"),
        size: params.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform, 0, &params);
    let records = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bae-render.shadowmap"),
        size: (record.len().max(1) * 16) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&records, 0, bytemuck::cast_slice(record));
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bae-render.shadowmap"),
        layout: &p.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: records.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(atlas.unwrap_or(&p.dummy)),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(&p.sampler),
            },
        ],
    });
    let [w, h] = [layout.slot[0] * layout.count, layout.slot[1]];
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.shadowmap"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&p.pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.set_scissor_rect(0, 0, w.max(1), h.max(1));
    pass.draw(0..3, 0..1);
}

/// Shadow maps kept across frames, by node key; dropped two frames after
/// their last use.
#[derive(Default)]
pub(crate) struct ShadowMapCache {
    maps: FxHashMap<u64, (GpuImage, u64)>,
    /// Each layer's map key last asked for, with that frame.
    last: FxHashMap<bae_model::PrimPath, (u64, u64)>,
    frame: u64,
}

impl ShadowMapCache {
    pub(crate) fn get(&mut self, key: u64) -> Option<GpuImage> {
        let frame = self.frame;
        self.maps.get_mut(&key).map(|(image, used)| {
            *used = frame;
            image.clone()
        })
    }

    /// Whether `layer` asked for the map `key` in an earlier frame too (its
    /// shadows hold still); records the request.
    pub(crate) fn stable(&mut self, layer: &bae_model::PrimPath, key: u64) -> bool {
        let frame = self.frame;
        match self.last.insert(layer.clone(), (key, frame)) {
            Some((k, f)) => k == key && f < frame,
            None => false,
        }
    }

    pub(crate) fn insert(&mut self, key: u64, image: GpuImage) {
        self.maps.insert(key, (image, self.frame));
    }

    /// Ends a frame: forgets maps unused in the last two frames.
    pub(crate) fn end_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.maps.retain(|_, (_, used)| frame - *used <= 2);
        self.last.retain(|_, (_, asked)| frame - *asked <= 2);
    }
}

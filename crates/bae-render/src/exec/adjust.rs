//! The adjustment layer mix pass ([`crate::graph::MixNode`], `gpu/adjust.wgsl`):
//! one full-screen draw blending the adjusted backdrop into the backdrop by
//! the adjustment layer's coverage.

use bae_gpu::wgpu;
use rustc_hash::FxHashMap;

use crate::source::{GpuImage, ImageAlpha};

/// The mix pipelines (one per working format), created on first use.
pub(crate) struct MixPipelines {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl MixPipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.adjust"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gpu/adjust.wgsl").into()),
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.adjust"),
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
                tex(1),
                tex(2),
                tex(3),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.adjust"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Self {
            module,
            layout,
            pipeline_layout,
            pipelines: FxHashMap::default(),
        }
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bae-render.adjust"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_mix"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

/// `Img` of adjust.wgsl.
fn image_params(image: &GpuImage) -> [f32; 8] {
    let r = image.rect;
    let alpha = match image.alpha {
        ImageAlpha::Premultiplied => 0.0,
        ImageAlpha::Straight => 1.0,
        ImageAlpha::Opaque => 2.0,
    };
    [
        r.x0 as f32,
        r.y0 as f32,
        r.x1 as f32,
        r.y1 as f32,
        image.pixel_scale as f32,
        alpha,
        0.0,
        0.0,
    ]
}

/// Records the mix of `base`, `over` and `cover` into `target`.
pub(crate) fn encode(
    pipelines: &mut Option<MixPipelines>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    format: wgpu::TextureFormat,
    target: &GpuImage,
    [base, over, cover]: [&GpuImage; 3],
) {
    let pipelines = pipelines.get_or_insert_with(|| MixPipelines::new(device));
    let mut params = vec![
        target.rect.x0 as f32,
        target.rect.y0 as f32,
        target.pixel_scale as f32,
        0.0,
    ];
    for image in [base, over, cover] {
        params.extend(image_params(image));
    }
    let bytes: Vec<u8> = params.iter().flat_map(|v| v.to_le_bytes()).collect();
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bae-render.adjust"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniform, 0, &bytes);
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bae-render.adjust"),
        layout: &pipelines.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&base.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&over.view),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&cover.view),
            },
        ],
    });
    let [w, h] = target.texel_size();
    let pipeline = pipelines.pipeline(device, format);
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.adjust"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.set_scissor_rect(0, 0, w.max(1), h.max(1));
    pass.draw(0..3, 0..1);
}

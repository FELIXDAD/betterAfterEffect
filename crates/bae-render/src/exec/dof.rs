//! Depth of field of one 3D layer ([`crate::graph::DepthBlurNode`],
//! `gpu/dof.wgsl`): a gather over the camera's iris at full resolution, or for
//! large blurs over a prefiltered copy reduced by a power of two and resolved
//! against the sharp layer. The reduced copies come from the texture pool.

use bae_gpu::wgpu;
use rustc_hash::FxHashMap;

use crate::error::RenderError;
use crate::graph::DepthBlurNode;
use crate::pool::TexturePool;
use crate::source::GpuImage;

/// Output texels of radius gathered at full resolution; larger blurs gather a
/// reduced copy.
const DIRECT_RADIUS: f64 = 12.0;
/// Largest reduction of the gathered copy.
const MAX_REDUCTION: f64 = 8.0;

/// The passes of `dof.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pass {
    Down,
    Gather,
    Resolve,
}

impl Pass {
    const ALL: [Pass; 3] = [Pass::Down, Pass::Gather, Pass::Resolve];

    fn entry(self) -> &'static str {
        match self {
            Pass::Down => "fs_down",
            Pass::Gather => "fs_gather",
            Pass::Resolve => "fs_resolve",
        }
    }
}

/// The depth-of-field pipelines (per working format and pass), created on
/// first use or by the warm-up.
pub(crate) struct DofPipelines {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    pipelines: FxHashMap<(wgpu::TextureFormat, Pass), wgpu::RenderPipeline>,
}

impl DofPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.dof"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gpu/dof.wgsl").into()),
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.dof"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.dof"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.dof"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            module,
            layout,
            pipeline_layout,
            sampler,
            pipelines: FxHashMap::default(),
        }
    }

    /// Compiles every pass for `format` (the warm-up).
    pub(crate) fn warm(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        for pass in Pass::ALL {
            self.pipeline(device, format, pass);
        }
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pass: Pass,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.pipelines.entry((format, pass)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bae-render.dof"),
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
                    entry_point: Some(pass.entry()),
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

/// `Img` of dof.wgsl.
fn image_params(image: &GpuImage) -> [f32; 8] {
    let r = image.rect;
    [
        r.x0 as f32,
        r.y0 as f32,
        r.x1 as f32,
        r.y1 as f32,
        image.pixel_scale as f32,
        1.0 / image.texture.width() as f32,
        1.0 / image.texture.height() as f32,
        0.0,
    ]
}

/// Lens parameters shared by every pass (`plane`, `lens`, `iris`, `light`).
fn lens_params(node: &DepthBlurNode) -> [f32; 16] {
    let iris = &node.iris;
    // Fast Rectangle: an upright square, neither turned nor rounded.
    let (blades, rotation, roundness) = if iris.shape >= 3 {
        (
            iris.shape as f32,
            iris.rotation.to_radians() as f32,
            (iris.roundness / 100.0).clamp(0.0, 1.0) as f32,
        )
    } else {
        (4.0, 0.0, 0.0)
    };
    [
        node.inv_depth[0] as f32,
        node.inv_depth[1] as f32,
        node.inv_depth[2] as f32,
        node.focus as f32,
        node.coc as f32,
        node.max_radius as f32,
        blades,
        rotation,
        roundness,
        iris.aspect_ratio.clamp(0.01, 100.0) as f32,
        (iris.diffraction_fringe / 100.0).clamp(0.0, 5.0) as f32,
        0.0,
        (iris.highlight_gain * 0.05).max(0.0) as f32,
        iris.highlight_threshold.clamp(0.0, 1.0) as f32,
        (iris.highlight_saturation / 100.0).clamp(0.0, 1.0) as f32,
        0.0,
    ]
}

struct Encoder<'a> {
    pipelines: &'a mut DofPipelines,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    encoder: &'a mut wgpu::CommandEncoder,
    format: wgpu::TextureFormat,
    lens: [f32; 16],
}

impl Encoder<'_> {
    fn pass(
        &mut self,
        pass: Pass,
        target: &GpuImage,
        src: &GpuImage,
        low: &GpuImage,
        reduction: f64,
    ) {
        let mut params = vec![
            target.rect.x0 as f32,
            target.rect.y0 as f32,
            target.pixel_scale as f32,
            reduction as f32,
        ];
        params.extend(self.lens);
        params.extend(image_params(src));
        params.extend(image_params(low));
        let bytes: Vec<u8> = params.iter().flat_map(|v| v.to_le_bytes()).collect();
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bae-render.dof"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&uniform, 0, &bytes);
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.dof"),
            layout: &self.pipelines.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&src.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&low.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.pipelines.sampler),
                },
            ],
        });
        let pipeline = self.pipelines.pipeline(self.device, self.format, pass);
        // The whole attachment: texels past the rect turn transparent, so
        // bilinear reads at its edge stay clean.
        let mut rp = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.dof"),
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
        rp.set_pipeline(pipeline);
        rp.set_bind_group(0, &group, &[]);
        rp.draw(0..3, 0..1);
    }
}

/// Records the depth-of-field blur of `input` into `target`. Returns the
/// number of passes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    pipelines: &mut Option<DofPipelines>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    pool: &mut TexturePool,
    format: wgpu::TextureFormat,
    node: &DepthBlurNode,
    input: &GpuImage,
    target: &GpuImage,
) -> Result<usize, RenderError> {
    let pipelines = pipelines.get_or_insert_with(|| DofPipelines::new(device));
    let mut enc = Encoder {
        pipelines,
        device,
        queue,
        encoder,
        format,
        lens: lens_params(node),
    };
    let texels = node.max_radius / target.pixel_scale;
    if texels <= DIRECT_RADIUS {
        enc.pass(Pass::Gather, target, input, input, 1.0);
        return Ok(1);
    }
    let reduction = (texels / DIRECT_RADIUS)
        .log2()
        .ceil()
        .exp2()
        .min(MAX_REDUCTION);
    let scale = target.pixel_scale * reduction;
    let low = pool.acquire(device, format, target.rect, scale)?;
    enc.pass(Pass::Down, &low, input, input, reduction);
    let blurred = pool.acquire(device, format, target.rect, scale)?;
    enc.pass(Pass::Gather, &blurred, &low, &low, reduction);
    enc.pass(Pass::Resolve, target, input, &blurred, reduction);
    Ok(3)
}

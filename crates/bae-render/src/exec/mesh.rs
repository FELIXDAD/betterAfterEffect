//! 3D model layers ([`crate::graph::MeshNode`], `gpu/mesh.wgsl`): the model's
//! triangles drawn through the comp's camera into the node's target with a
//! depth buffer. Vertex, index and texture data are uploaded once per model
//! and kept (per device) while the model lives. Environment light maps
//! (`exec/env.rs`) live here too: models read them, and the background
//! ([`crate::graph::SkyNode`]) draws them.

use std::sync::{Arc, Weak};

use bae_gpu::wgpu;
use bae_gpu::wgpu::util::DeviceExt;
use glam::{DMat4, DVec3};
use rustc_hash::FxHashMap;

use super::env::{self, EnvPipelines};
use crate::camera::Projection;
use crate::graph::{EnvironmentInput, MeshNode, SkyNode};
use crate::light::{Falloff, LightKind};
use crate::model::Model;
use crate::source::GpuImage;

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Lights a model is lit by at most.
const MAX_LIGHTS: usize = 8;
/// Camera depth of the near plane (pixels).
const NEAR: f64 = 1.0;

/// A model on the GPU.
struct GpuModel {
    model: Weak<Model>,
    /// Vertex buffer, index buffer, index count, material bind group.
    parts: Vec<(wgpu::Buffer, wgpu::Buffer, u32, usize)>,
    materials: Vec<wgpu::BindGroup>,
}

/// The mesh pipelines (per working format) and the models uploaded.
pub(crate) struct MeshPipelines {
    module: wgpu::ShaderModule,
    frame_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    env_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    /// Untextured materials sample it.
    white: Option<wgpu::TextureView>,
    models: FxHashMap<usize, GpuModel>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
    env: EnvPipelines,
    /// Bound when no environment light has a picture.
    no_env: wgpu::TextureView,
}

impl MeshPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.mesh"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gpu/mesh.wgsl").into()),
        });
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.mesh.frame"),
            entries: &[uniform(
                0,
                wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            )],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.mesh.material"),
            entries: &[
                uniform(0, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let env_texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let env_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.mesh.env"),
            entries: &[
                env_texture(0),
                env_texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.mesh"),
            bind_group_layouts: &[
                Some(&frame_layout),
                Some(&material_layout),
                Some(&env_layout),
            ],
            immediate_size: 0,
        });
        let no_env = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.mesh.no-env"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: env::FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.mesh"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            module,
            frame_layout,
            material_layout,
            env_layout,
            pipeline_layout,
            pipelines: FxHashMap::default(),
            sampler,
            white: None,
            models: FxHashMap::default(),
            depth: None,
            env: EnvPipelines::new(device),
            no_env,
        }
    }

    /// Compiles the pipelines for `format` (the warm-up).
    pub(crate) fn warm(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipeline(device, format);
        self.env.warm(device, format);
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.pipelines.entry(format).or_insert_with(|| {
            let attributes = [
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x3,
                    offset: 12,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 24,
                    shader_location: 2,
                },
            ];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bae-render.mesh"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_mesh"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Greater),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_mesh"),
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

    /// The model's GPU data (uploaded on first use; models gone are dropped).
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, model: &Arc<Model>) -> usize {
        self.models.retain(|_, g| g.model.strong_count() > 0);
        let key = Arc::as_ptr(model) as usize;
        if self
            .models
            .get(&key)
            .is_some_and(|g| std::ptr::eq(g.model.as_ptr(), Arc::as_ptr(model)))
        {
            return key;
        }
        let white = self
            .white
            .get_or_insert_with(|| texture(device, queue, 1, 1, &[255; 4]))
            .clone();
        let textures: Vec<wgpu::TextureView> = model
            .textures
            .iter()
            .map(|t| texture(device, queue, t.width, t.height, &t.rgba))
            .collect();
        let materials = model
            .materials
            .iter()
            .map(|m| {
                let view = m.base_texture.and_then(|i| textures.get(i));
                let mut data = Vec::with_capacity(48);
                for v in m.base_color {
                    data.extend(v.to_le_bytes());
                }
                for v in [
                    m.metallic,
                    m.roughness,
                    f32::from(u8::from(view.is_some())),
                    0.0,
                ] {
                    data.extend(v.to_le_bytes());
                }
                for v in [m.emissive[0], m.emissive[1], m.emissive[2], 0.0] {
                    data.extend(v.to_le_bytes());
                }
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("bae-render.mesh.material"),
                    contents: &data,
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bae-render.mesh.material"),
                    layout: &self.material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(view.unwrap_or(&white)),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                })
            })
            .collect();
        let parts = model
            .parts
            .iter()
            .filter(|p| !p.indices.is_empty())
            .map(|p| {
                let mut vertices: Vec<f32> = Vec::with_capacity(p.positions.len() * 8);
                for i in 0..p.positions.len() {
                    vertices.extend(p.positions[i]);
                    vertices.extend(p.normals[i]);
                    vertices.extend(p.uvs[i]);
                }
                let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("bae-render.mesh.vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("bae-render.mesh.indices"),
                    contents: bytemuck::cast_slice(&p.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                (vb, ib, p.indices.len() as u32, p.material)
            })
            .collect();
        self.models.insert(
            key,
            GpuModel {
                model: Arc::downgrade(model),
                parts,
                materials,
            },
        );
        key
    }

    /// A depth texture as large as `size` (reused while it fits).
    fn depth(&mut self, device: &wgpu::Device, size: [u32; 2]) -> wgpu::TextureView {
        if let Some((s, view)) = &self.depth
            && *s == size
        {
            return view.clone();
        }
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.mesh.depth"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        self.depth = Some((size, view.clone()));
        view
    }
}

fn texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    rgba: &[u8],
) -> wgpu::TextureView {
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("bae-render.mesh.texture"),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        )
        .create_view(&Default::default())
}

/// Camera space → clip space of a target covering `rect` (texels at
/// `scale` comp pixels each) of a `comp` pixels comp: AE's projection, depth
/// reversed (`near / z`).
fn clip_matrix(node: &MeshNode, rect: [f64; 4], scale: f64) -> DMat4 {
    let [rx, ry, rw, rh] = rect;
    let (w, h) = (f64::from(node.comp_size[0]), f64::from(node.comp_size[1]));
    let bx = 2.0 / rw * (w / 2.0 / scale - rx) - 1.0;
    let by = 1.0 - 2.0 / rh * (h / 2.0 / scale - ry);
    let rows = match node.camera.projection {
        Projection::Perspective => {
            let (ax, ay) = (
                2.0 * node.camera.zoom / (scale * rw),
                2.0 * node.camera.zoom / (scale * rh),
            );
            [
                [ax, 0.0, bx, 0.0],
                [0.0, -ay, by, 0.0],
                [0.0, 0.0, 0.0, NEAR],
                [0.0, 0.0, 1.0, 0.0],
            ]
        }
        Projection::Orthographic { scale: s } => {
            let (ax, ay) = (2.0 * s / (scale * rw), 2.0 * s / (scale * rh));
            [
                [ax, 0.0, 0.0, bx],
                [0.0, -ay, 0.0, by],
                [0.0, 0.0, -1e-5, 0.5],
                [0.0, 0.0, 0.0, 1.0],
            ]
        }
    };
    DMat4::from_cols_array_2d(&rows).transpose()
}

/// The frame uniform of `mesh.wgsl` (`levels`: the environment map's level
/// count, 0 without one).
fn frame_uniform(node: &MeshNode, target: &GpuImage, linear: bool, levels: u32) -> Vec<u8> {
    let s = target.pixel_scale;
    let r = target.rect;
    let rect = [
        f64::from(r.x0) / s,
        f64::from(r.y0) / s,
        f64::from(r.x1 - r.x0) / s,
        f64::from(r.y1 - r.y0) / s,
    ];
    let clip = clip_matrix(node, rect, s);
    let view = node.camera.view();
    let normal = node.matrix.inverse().transpose();
    let mut out: Vec<f32> = Vec::with_capacity(200);
    for m in [clip, view, node.matrix, normal] {
        out.extend(m.to_cols_array().map(|v| v as f32));
    }
    let eye: DVec3 = node.camera.eye();
    // An environment light with a picture lights through its maps.
    let lights: Vec<_> = node
        .lights
        .iter()
        .filter(|l| levels == 0 || l.environment.is_none())
        .take(MAX_LIGHTS)
        .collect();
    out.extend([
        eye.x as f32,
        eye.y as f32,
        eye.z as f32,
        lights.len() as f32,
    ]);
    out.extend([f32::from(u8::from(!linear)), 0.0, 0.0, 0.0]);
    match node.environment.as_ref().filter(|_| levels > 0) {
        Some(e) => {
            out.extend(e.rotation.to_cols_array().map(|v| v as f32));
            out.extend([e.tint[0], e.tint[1], e.tint[2], levels as f32]);
        }
        None => {
            out.extend(DMat4::IDENTITY.to_cols_array().map(|v| v as f32));
            out.extend([0.0; 4]);
        }
    }
    for i in 0..MAX_LIGHTS {
        let Some(l) = lights.get(i) else {
            out.extend([0.0; 16]);
            continue;
        };
        let kind = match l.kind {
            LightKind::Ambient | LightKind::Environment => 0.0,
            LightKind::Parallel => 1.0,
            LightKind::Point => 2.0,
            LightKind::Spot => 3.0,
        };
        let falloff = match l.falloff {
            Falloff::None => 0.0,
            Falloff::Smooth => 1.0,
            Falloff::InverseSquareClamped => 2.0,
        };
        let k = l.intensity / 100.0;
        let outer = (f64::from(l.cone_angle) / 2.0).to_radians();
        let inner = outer * (1.0 - f64::from(l.cone_feather) / 100.0);
        let p = l.position();
        let d = l.direction();
        out.extend([p.x as f32, p.y as f32, p.z as f32, kind]);
        out.extend([d.x as f32, d.y as f32, d.z as f32, falloff]);
        out.extend([l.color[0] * k, l.color[1] * k, l.color[2] * k, l.radius]);
        out.extend([
            outer.cos() as f32,
            inner.cos() as f32,
            l.falloff_distance,
            0.0,
        ]);
    }
    bytemuck::cast_slice(&out).to_vec()
}

/// Records the model of `node` into `target`. Returns the number of draws.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    pipelines: &mut Option<MeshPipelines>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    format: wgpu::TextureFormat,
    node: &MeshNode,
    environment: Option<(&EnvironmentInput, u64, Option<&GpuImage>)>,
    target: &GpuImage,
) -> usize {
    let p = pipelines.get_or_insert_with(|| MeshPipelines::new(device));
    let key = p.upload(device, queue, &node.model);
    let size = target.texture_size();
    let depth = p.depth(device, size);
    p.pipeline(device, format);
    let map = environment
        .and_then(|(e, key, image)| p.env.map(device, encoder, key, image, e.width, node.linear));
    let env_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bae-render.mesh.env"),
        layout: &p.env_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    map.as_ref().map_or(&p.no_env, |m| &m.view),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(
                    map.as_ref().map_or(&p.no_env, |m| &m.irradiance),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&p.env.sampler),
            },
        ],
    });
    let levels = map.as_ref().map_or(0, |m| m.levels);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("bae-render.mesh.frame"),
        contents: &frame_uniform(node, target, node.linear, levels),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bae-render.mesh.frame"),
        layout: &p.frame_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let [w, h] = target.texel_size();
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.mesh"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(0.0),
                store: wgpu::StoreOp::Discard,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_viewport(0.0, 0.0, w.max(1) as f32, h.max(1) as f32, 0.0, 1.0);
    pass.set_scissor_rect(0, 0, w.max(1), h.max(1));
    pass.set_pipeline(&p.pipelines[&format]);
    pass.set_bind_group(0, &frame, &[]);
    pass.set_bind_group(2, &env_group, &[]);
    let gpu = &p.models[&key];
    for (vb, ib, count, material) in &gpu.parts {
        let Some(group) = gpu.materials.get(*material) else {
            continue;
        };
        pass.set_bind_group(1, group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..*count, 0, 0..1);
    }
    gpu.parts.len()
}

/// Draws an environment light's picture behind the layers into `target`
/// (`key`: the picture node's key; its maps are built once, `image` is
/// `None` when they were already built).
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_sky(
    pipelines: &mut Option<MeshPipelines>,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    format: wgpu::TextureFormat,
    node: &SkyNode,
    key: u64,
    image: Option<&GpuImage>,
    target: &GpuImage,
) {
    let p = pipelines.get_or_insert_with(|| MeshPipelines::new(device));
    let e = &node.environment;
    let map = p.env.map(device, encoder, key, image, e.width, node.linear);
    p.env
        .sky(device, encoder, format, node, map.as_ref(), target);
}

impl MeshPipelines {
    /// Starts a frame for the environment maps.
    pub(crate) fn begin_frame(&mut self) {
        self.env.begin_frame();
    }

    /// Whether the maps of environment picture node `key` are built.
    pub(crate) fn has_env(&mut self, key: u64, width: u32, linear: bool) -> bool {
        self.env.has(key, width, linear)
    }
}

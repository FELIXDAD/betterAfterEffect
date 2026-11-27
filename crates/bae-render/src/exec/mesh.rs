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
use crate::graph::{MeshModel, MeshNode, SkyNode};
use crate::light::{Falloff, LightKind};
use crate::model::Model;
use crate::source::GpuImage;

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Camera depth (hi, lo halves) and alpha of each pixel, for the planes
/// drawn over the models (filterable, so the composite can bind it).
pub(crate) const DEPTH_OUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
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
    /// Model shadows in receivers' shadow maps.
    shadow: Option<(wgpu::BindGroupLayout, wgpu::RenderPipeline)>,
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
            shadow: None,
        }
    }

    /// Compiles the pipelines for `format` (the warm-up).
    pub(crate) fn warm(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.pipeline(device, format);
        self.env.warm(device, format);
        self.shadow_pipeline(device);
    }

    /// The model shadow pipeline (into [`super::shadowmap::FORMAT`] maps).
    fn shadow_pipeline(
        &mut self,
        device: &wgpu::Device,
    ) -> &(wgpu::BindGroupLayout, wgpu::RenderPipeline) {
        self.shadow.get_or_insert_with(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("bae-render.mesh.shadow"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../gpu/model_shadow.wgsl").into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bae-render.mesh.shadow"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("bae-render.mesh.shadow"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let attributes = [wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            }];
            let min = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Min,
            };
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bae-render.mesh.shadow"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_shadow"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_shadow"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: super::shadowmap::FORMAT,
                        blend: Some(wgpu::BlendState {
                            color: min,
                            alpha: min,
                        }),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            (layout, pipeline)
        })
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
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(wgpu::ColorTargetState {
                            format: DEPTH_OUT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                    ],
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
fn frame_uniform(
    node: &MeshNode,
    model: &MeshModel,
    target: &GpuImage,
    linear: bool,
    levels: u32,
) -> Vec<u8> {
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
    let normal = model.matrix.inverse().transpose();
    let mut out: Vec<f32> = Vec::with_capacity(200);
    for m in [clip, view, model.matrix, normal] {
        out.extend(m.to_cols_array().map(|v| v as f32));
    }
    let eye: DVec3 = node.camera.eye();
    // An environment light with a picture lights through its maps.
    let lights: Vec<_> = model
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
    out.extend([f32::from(u8::from(!linear)), model.opacity, 0.0, 0.0]);
    match model.environment.as_ref().filter(|_| levels > 0) {
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

/// Records the models of `node` into `target`, with each pixel's camera
/// depth and alpha into `depth_out` ([`DEPTH_OUT`], same size).
/// `environments`: per model, its environment picture's node key and image
/// (`None` when its maps are built). Returns the number of draws.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    pipelines: &mut Option<MeshPipelines>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    format: wgpu::TextureFormat,
    node: &MeshNode,
    environments: &[Option<(u64, Option<GpuImage>)>],
    target: &GpuImage,
    depth_out: &GpuImage,
) -> usize {
    let p = pipelines.get_or_insert_with(|| MeshPipelines::new(device));
    let size = target.texture_size();
    let depth = p.depth(device, size);
    p.pipeline(device, format);
    // Per model: its GPU data, frame bind group and environment bind group.
    let mut draws = Vec::with_capacity(node.models.len());
    for (i, model) in node.models.iter().enumerate() {
        let key = p.upload(device, queue, &model.model);
        let map = match (
            &model.environment,
            environments.get(i).and_then(Option::as_ref),
        ) {
            (Some(e), Some((key, image))) => {
                p.env
                    .map(device, encoder, *key, image.as_ref(), e.width, node.linear)
            }
            _ => None,
        };
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
            contents: &frame_uniform(node, model, target, node.linear, levels),
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
        draws.push((key, frame, env_group));
    }
    let [w, h] = target.texel_size();
    let attachment = |view| {
        Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })
    };
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.mesh"),
        color_attachments: &[attachment(&target.view), attachment(&depth_out.view)],
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
    let mut count = 0;
    for (key, frame, env_group) in &draws {
        pass.set_bind_group(0, frame, &[]);
        pass.set_bind_group(2, env_group, &[]);
        let gpu = &p.models[key];
        for (vb, ib, n, material) in &gpu.parts {
            let Some(group) = gpu.materials.get(*material) else {
                continue;
            };
            pass.set_bind_group(1, group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
            count += 1;
        }
    }
    count
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

/// Draws the shadows of the 3D models of `lighting` into the slots `slots`
/// (slot index, light index) of its shadow map `target` (each slot
/// `size` texels; the map spans `lighting.rect` in layer pixels). Returns
/// the number of draws.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_shadows(
    pipelines: &mut Option<MeshPipelines>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    lighting: &crate::light::Lighting,
    slots: &[(u32, usize)],
    size: [u32; 2],
    target: &GpuImage,
) -> usize {
    let p = pipelines.get_or_insert_with(|| MeshPipelines::new(device));
    let keys: Vec<usize> = lighting
        .models
        .iter()
        .map(|m| p.upload(device, queue, &m.model))
        .collect();
    let (layout, pipeline) = p.shadow_pipeline(device).clone();
    let mut draws = Vec::new();
    for &(slot, li) in slots {
        let Some(light) = lighting.lights.get(li) else {
            continue;
        };
        let Some((clip, side)) = shadow_projection(lighting, light) else {
            continue;
        };
        let value = 1.0 - (light.shadow_darkness / 100.0).clamp(0.0, 1.0);
        for (m, key) in lighting.models.iter().zip(&keys) {
            let mut data: Vec<f32> = Vec::with_capacity(40);
            data.extend(clip.to_cols_array().map(|v| v as f32));
            data.extend(m.matrix.to_cols_array().map(|v| v as f32));
            data.extend(side.map(|v| v as f32));
            data.extend([value, value, value, 1.0]);
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bae-render.mesh.shadow"),
                contents: bytemuck::cast_slice(&data),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bae-render.mesh.shadow"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            draws.push((slot, *key, group));
        }
    }
    if draws.is_empty() {
        return 0;
    }
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.mesh.shadow"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(&pipeline);
    let [w, h] = size;
    let mut count = 0;
    for (slot, key, group) in &draws {
        pass.set_viewport((slot * w) as f32, 0.0, w as f32, h as f32, 0.0, 1.0);
        pass.set_scissor_rect(slot * w, 0, w, h);
        pass.set_bind_group(0, group, &[]);
        for (vb, ib, n, _) in &p.models[key].parts {
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
            count += 1;
        }
    }
    count
}

/// Comp space → shadow map slot clip space for `light` on the receiver of
/// `lighting`: the planar projection from the light onto the receiver
/// plane, then the receiver's layer pixels, then the slot. With the plane
/// as `a·x + b·y + c·z + d`, positive on the light's side. `None` for lights
/// without a position or direction (ambient, environment) or a degenerate
/// receiver.
fn shadow_projection(
    lighting: &crate::light::Lighting,
    light: &crate::light::Light,
) -> Option<(DMat4, [f64; 4])> {
    use glam::DVec4;
    let m = lighting.matrix;
    let n = m.x_axis.truncate().cross(m.y_axis.truncate());
    if n.length_squared() < 1e-18 {
        return None;
    }
    let plane = DVec4::from((n, -n.dot(m.w_axis.truncate())));
    let source = match light.kind {
        LightKind::Point | LightKind::Spot => DVec4::from((light.position(), 1.0)),
        LightKind::Parallel => DVec4::from((-light.direction(), 0.0)),
        LightKind::Ambient | LightKind::Environment => return None,
    };
    let along = plane.dot(source);
    if along.abs() < 1e-12 {
        return None;
    }
    // Blinn's planar shadow matrix: (π·L) I − L πᵀ, signed so that points
    // between the light and the plane keep w > 0.
    let s = along.signum();
    let project = DMat4::from_cols(
        DVec4::X * along - source * plane.x,
        DVec4::Y * along - source * plane.y,
        DVec4::Z * along - source * plane.z,
        DVec4::W * along - source * plane.w,
    ) * s;
    let to_layer = m.inverse();
    let r = lighting.rect;
    let (rw, rh) = (r.x1 - r.x0, r.y1 - r.y0);
    if rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let to_slot = DMat4::from_cols_array_2d(&[
        [2.0 / rw, 0.0, 0.0, -1.0 - 2.0 * r.x0 / rw],
        [0.0, -2.0 / rh, 0.0, 1.0 + 2.0 * r.y0 / rh],
        [0.0, 0.0, 0.0, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ])
    .transpose();
    let side = plane * s;
    Some((to_slot * to_layer * project, side.to_array()))
}

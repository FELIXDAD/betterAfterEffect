//! GPU resources of the compositor: pipelines, bind group layouts, the per-draw
//! record shared with `composite.wgsl`, and the 8-bit output pass.

use bae_gpu::wgpu;
use bytemuck::{Pod, Zeroable};
use glam::DAffine2;
use rustc_hash::FxHashMap;

use crate::blend::BlendMode;

/// Shader flags (must match `composite.wgsl`).
pub(crate) mod flags {
    pub const TEXTURE: u32 = 1;
    pub const STRAIGHT: u32 = 2;
    pub const MATTE: u32 = 4;
    pub const DST: u32 = 8;
    pub const PRESERVE: u32 = 16;
    pub const CLAMP: u32 = 32;
    pub const OPAQUE: u32 = 64;
}

/// One draw (layer op), laid out like `struct Draw` in `composite.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct DrawData {
    pub fwd_x: [f32; 4],
    pub fwd_y: [f32; 4],
    pub inv_x: [f32; 4],
    pub inv_y: [f32; 4],
    pub rect: [f32; 4],
    pub quad: [f32; 4],
    pub color: [f32; 4],
    pub src: [f32; 4],
    pub matte: [f32; 4],
    pub target: [f32; 4],
    pub mode: [u32; 4],
}

impl DrawData {
    /// Fills the geometry from `transform` (input texels → target texels), the
    /// content rectangle in input texels and the target size. `None` when the
    /// transform is degenerate.
    pub fn geometry(transform: &DAffine2, rect: [f64; 4], target: [u32; 2]) -> Option<DrawData> {
        let det = transform.matrix2.determinant();
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let inv = transform.inverse();
        let row = |a: &DAffine2| {
            let (m, t) = (a.matrix2, a.translation);
            (
                [m.x_axis.x as f32, m.y_axis.x as f32, t.x as f32, 0.0],
                [m.x_axis.y as f32, m.y_axis.y as f32, t.y as f32, 0.0],
            )
        };
        let (fwd_x, fwd_y) = row(transform);
        let (inv_x, inv_y) = row(&inv);
        // Perpendicular distance scale: target pixels per input texel across the
        // edges x = const (|det| / |y axis|) and y = const (|det| / |x axis|).
        let kx = det.abs() / transform.matrix2.y_axis.length();
        let ky = det.abs() / transform.matrix2.x_axis.length();
        // Quad margin: the edge ramp reaches half a footprint (max of one target
        // pixel and one input texel) beyond the edge; one full footprint is safe.
        let (mx, my) = ((1.0 / kx).max(1.0), (1.0 / ky).max(1.0));
        Some(DrawData {
            fwd_x,
            fwd_y,
            inv_x,
            inv_y,
            rect: rect.map(|v| v as f32),
            quad: [
                (rect[0] - mx) as f32,
                (rect[1] - my) as f32,
                (rect[2] + mx) as f32,
                (rect[3] + my) as f32,
            ],
            src: [0.0, 0.0, kx as f32, ky as f32],
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            ..DrawData::default()
        })
    }
}

/// How a draw's output is combined with the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BlendKind {
    /// Premultiplied source-over.
    Normal,
    /// Colour `src + dst`, alpha source-over (Add without clamping).
    Add,
    /// Colour `src + dst − src·dst`, alpha source-over.
    Screen,
    /// The shader computes the result from a backdrop copy.
    Replace,
}

impl BlendKind {
    /// The fixed-function kind for `mode`, or `Replace` when the shader must read
    /// the backdrop.
    pub fn for_mode(mode: BlendMode, clamp: bool, preserve: bool, blendable: bool) -> BlendKind {
        if preserve || !blendable {
            return BlendKind::Replace;
        }
        match mode {
            BlendMode::Normal => BlendKind::Normal,
            BlendMode::Add if !clamp => BlendKind::Add,
            BlendMode::Screen => BlendKind::Screen,
            _ => BlendKind::Replace,
        }
    }

    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
        let over = C {
            src_factor: F::One,
            dst_factor: F::OneMinusSrcAlpha,
            operation: O::Add,
        };
        let color = match self {
            BlendKind::Normal => over,
            BlendKind::Add => C {
                src_factor: F::One,
                dst_factor: F::One,
                operation: O::Add,
            },
            BlendKind::Screen => C {
                src_factor: F::OneMinusDst,
                dst_factor: F::One,
                operation: O::Add,
            },
            BlendKind::Replace => return None,
        };
        Some(wgpu::BlendState { color, alpha: over })
    }
}

/// Compiled pipelines and shared objects.
pub(crate) struct Pipelines {
    module: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    pub draws_layout: wgpu::BindGroupLayout,
    pub textures_layout: wgpu::BindGroupLayout,
    composite: FxHashMap<(wgpu::TextureFormat, BlendKind), wgpu::RenderPipeline>,
    pub sampler: wgpu::Sampler,
    pub dummy: wgpu::TextureView,
    pub empty_textures: wgpu::BindGroup,
    pub output_layout: wgpu::BindGroupLayout,
    pub output: wgpu::RenderPipeline,
}

impl Pipelines {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let draws_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.draws"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
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
        let textures_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.textures"),
            entries: &[
                tex(0),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.composite"),
            bind_group_layouts: &[Some(&draws_layout), Some(&textures_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.dummy"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let empty_textures = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.empty"),
            layout: &textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let output_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.output"),
            source: wgpu::ShaderSource::Wgsl(include_str!("output.wgsl").into()),
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.output"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let output_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("bae-render.output"),
                bind_group_layouts: &[Some(&output_layout)],
                immediate_size: 0,
            });
        let output = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bae-render.output"),
            layout: Some(&output_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &output_module,
                entry_point: Some("vs_full"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &output_module,
                entry_point: Some("fs_out"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            module,
            layout,
            draws_layout,
            textures_layout,
            composite: FxHashMap::default(),
            sampler,
            dummy,
            empty_textures,
            output_layout,
            output,
        }
    }

    /// The composite pipeline for a target format and blend kind (compiled on
    /// first use).
    pub fn composite(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        kind: BlendKind,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.layout);
        self.composite.entry((format, kind)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("bae-render.composite"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: kind.state(),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    /// A texture bind group (unused slots get the dummy texture).
    pub fn textures(
        &self,
        device: &wgpu::Device,
        src: Option<&wgpu::TextureView>,
        matte: Option<&wgpu::TextureView>,
        dst: Option<&wgpu::TextureView>,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.textures"),
            layout: &self.textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(matte.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(dst.unwrap_or(&self.dummy)),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;

    #[test]
    fn draw_record_matches_wgsl_layout() {
        // 11 vec4s: the WGSL struct stride.
        assert_eq!(std::mem::size_of::<DrawData>(), 176);
    }

    #[test]
    fn geometry_margins_follow_scale() {
        let t = DAffine2::from_scale(DVec2::new(2.0, 0.5));
        let d = DrawData::geometry(&t, [0.0, 0.0, 10.0, 10.0], [100, 100]).unwrap();
        // Margin = max(1 target pixel, 1 input texel): 1 texel along x (magnified),
        // 2 texels (= 1 target pixel) along y.
        assert_eq!(d.quad, [-1.0, -2.0, 11.0, 12.0]);
        assert_eq!(&d.src[2..], &[2.0, 0.5]);
        assert!(DrawData::geometry(&DAffine2::from_scale(DVec2::ZERO), [0.0; 4], [1, 1]).is_none());
    }
}

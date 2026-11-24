//! GPU resources of the compositor: pipelines, bind group layouts, the per-draw
//! record shared with `composite.wgsl`, the mask passes (`mask.wgsl`) and the
//! 8-bit output pass.

use bae_gpu::wgpu;
use bytemuck::{Pod, Zeroable};
use glam::{DAffine2, DMat3, DVec2};
use rustc_hash::FxHashMap;

use crate::three_d::Homography;

use crate::blend::{BlendMode, MaskMode};

/// Coverage accumulation buffers of masks (R: path winding, G: expansion outline).
pub(crate) const MASK_ACCUM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// Mask coverage and combined mask images.
pub(crate) const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

/// A WGSL file without comments, indentation and blank lines, as a `&'static str`
/// built at compile time: the shaders keep their comments in the source but do
/// not ship them inside the executable (about 6.8 KB).
macro_rules! wgsl {
    ($file:literal) => {{
        const SRC: &[u8] = include_bytes!($file);
        const LEN: usize = crate::gpu::minified_len(SRC);
        const OUT: [u8; LEN] = crate::gpu::minify::<LEN>(SRC);
        const TEXT: &str = match std::str::from_utf8(&OUT) {
            Ok(text) => text,
            Err(_) => panic!("WGSL source is not UTF-8"),
        };
        TEXT
    }};
}

/// Line `start..` of `src`: its content without `//` comment and surrounding
/// whitespace (`a..b`) and where the next line starts. WGSL has no string
/// literals, so `//` always starts a comment.
const fn wgsl_line(src: &[u8], start: usize) -> (usize, usize, usize) {
    let mut end = start;
    while end < src.len() && src[end] != b'\n' {
        end += 1;
    }
    let next = if end < src.len() { end + 1 } else { end };
    let mut cut = start;
    while cut < end && !(src[cut] == b'/' && cut + 1 < end && src[cut + 1] == b'/') {
        cut += 1;
    }
    let mut a = start;
    while a < cut && is_blank(src[a]) {
        a += 1;
    }
    let mut b = cut;
    while b > a && is_blank(src[b - 1]) {
        b -= 1;
    }
    (a, b, next)
}

const fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b'\r'
}

/// Length of [`minify`]'s output.
pub(crate) const fn minified_len(src: &[u8]) -> usize {
    let (mut i, mut n) = (0, 0);
    while i < src.len() {
        let (a, b, next) = wgsl_line(src, i);
        if b > a {
            n += b - a + 1;
        }
        i = next;
    }
    n
}

/// `src` without comments, indentation and blank lines (one line per line).
pub(crate) const fn minify<const N: usize>(src: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let (mut i, mut o) = (0, 0);
    while i < src.len() {
        let (a, b, next) = wgsl_line(src, i);
        if b > a {
            let mut k = a;
            while k < b {
                out[o] = src[k];
                o += 1;
                k += 1;
            }
            out[o] = b'\n';
            o += 1;
        }
        i = next;
    }
    out
}

/// Shader flags (must match `composite.wgsl`).
pub(crate) mod flags {
    pub const TEXTURE: u32 = 1;
    pub const STRAIGHT: u32 = 2;
    pub const MATTE: u32 = 4;
    pub const DST: u32 = 8;
    pub const PRESERVE: u32 = 16;
    pub const CLAMP: u32 = 32;
    pub const OPAQUE: u32 = 64;
    pub const MASK: u32 = 128;
    /// `fwd`/`inv` are full 3×3 homographies (3D layers).
    pub const PROJECTIVE: u32 = 256;
    /// Motion blur: average the samples `extra.y .. extra.y + extra.z`.
    pub const MOTION: u32 = 512;
}

/// Most layer-space clip half-planes per draw (3D intersections).
pub(crate) const MAX_CLIPS: usize = crate::three_d::MAX_CLIPS;

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
    /// Input texel `u` → mask texture uv: `u · xy + zw`.
    pub mask_uv: [f32; 4],
    /// Mask value `a + b · sample` (x, y).
    pub mask_val: [f32; 4],
    /// The mask's region in its atlas page (uv min x, y, max x, y).
    pub mask_clip: [f32; 4],
    /// Forward homography w row (input texel → target texel) and, in `w`, the
    /// near-plane weight below which geometry is clipped.
    pub fwd_w: [f32; 4],
    /// Inverse homography w row.
    pub inv_w: [f32; 4],
    /// Half-planes `a·u + b·v + c ≥ 0` in input texels (`extra.x` of them).
    pub clips: [[f32; 4]; MAX_CLIPS],
    /// Clip count, first motion sample, sample count, unused.
    pub extra: [u32; 4],
}

/// One motion-blur sample of a draw: target texel → input texel homography,
/// laid out like `struct Sample` in `composite.wgsl`. `inv_w.w` is
/// `1 / near` for 3D samples (pixels closer than the near plane are empty),
/// 0 otherwise.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct SampleData {
    pub inv_x: [f32; 4],
    pub inv_y: [f32; 4],
    pub inv_w: [f32; 4],
}

impl SampleData {
    /// The sample record for `to_target` (input texels → target texels).
    pub fn new(to_target: &Homography, near: Option<f64>) -> Option<SampleData> {
        let inv = to_target.inverse()?;
        let r = |i: usize| inv.0.row(i).as_vec3().extend(0.0).to_array();
        let mut inv_w = r(2);
        inv_w[3] = near.map_or(0.0, |n| (1.0 / n) as f32);
        Some(SampleData {
            inv_x: r(0),
            inv_y: r(1),
            inv_w,
        })
    }
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
            fwd_w: [0.0, 0.0, 1.0, 0.0],
            inv_w: [0.0, 0.0, 1.0, 0.0],
            ..DrawData::default()
        })
    }

    /// Geometry of a projective draw: `h` maps input texels to homogeneous
    /// target texels with `w` = camera depth; geometry closer than `near` is
    /// clipped. `None` when degenerate.
    pub fn projective(
        h: &Homography,
        rect: [f64; 4],
        target: [u32; 2],
        near: f64,
    ) -> Option<DrawData> {
        if !h.is_invertible() {
            return None;
        }
        let inv = h.inverse()?;
        let row = |m: &DMat3, i: usize| m.row(i).as_vec3().extend(0.0).to_array();
        // Quad margin: one target pixel beyond the edges where the layer is
        // smallest on screen (at least one input texel), from the local scales
        // at the corners in front of the camera.
        let corners = [
            DVec2::new(rect[0], rect[1]),
            DVec2::new(rect[2], rect[1]),
            DVec2::new(rect[2], rect[3]),
            DVec2::new(rect[0], rect[3]),
        ];
        let mut k = DVec2::splat(f64::INFINITY);
        for c in corners {
            if h.w(c) >= near {
                k = k.min(h.axis_scales_at(c));
            }
        }
        let limit = (rect[2] - rect[0]).max(rect[3] - rect[1]).max(1.0);
        let margin = |k: f64| {
            if k.is_finite() && k > 0.0 {
                (1.0 / k).clamp(1.0, limit)
            } else {
                1.0
            }
        };
        let (mx, my) = (margin(k.x), margin(k.y));
        let mut fwd_w = row(&h.0, 2);
        fwd_w[3] = near as f32;
        Some(DrawData {
            fwd_x: row(&h.0, 0),
            fwd_y: row(&h.0, 1),
            inv_x: row(&inv.0, 0),
            inv_y: row(&inv.0, 1),
            rect: rect.map(|v| v as f32),
            quad: [
                (rect[0] - mx) as f32,
                (rect[1] - my) as f32,
                (rect[2] + mx) as f32,
                (rect[3] + my) as f32,
            ],
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            mode: [0, flags::PROJECTIVE, 0, 0],
            fwd_w,
            inv_w: row(&inv.0, 2),
            ..DrawData::default()
        })
    }

    /// Geometry of a motion-blurred draw: the quad covers `bounds` (target
    /// texels) directly; the samples map each pixel back into the input.
    pub fn motion(bounds: [f64; 4], rect: [f64; 4], target: [u32; 2]) -> DrawData {
        DrawData {
            fwd_x: [1.0, 0.0, 0.0, 0.0],
            fwd_y: [0.0, 1.0, 0.0, 0.0],
            fwd_w: [0.0, 0.0, 1.0, 0.0],
            inv_x: [1.0, 0.0, 0.0, 0.0],
            inv_y: [0.0, 1.0, 0.0, 0.0],
            inv_w: [0.0, 0.0, 1.0, 0.0],
            rect: rect.map(|v| v as f32),
            quad: bounds.map(|v| v as f32),
            target: [target[0] as f32, target[1] as f32, 1.0, 0.0],
            mode: [0, flags::MOTION, 0, 0],
            ..DrawData::default()
        }
    }

    /// Adds layer-space clip half-planes given in input texels.
    pub fn set_clips(&mut self, clips: &[[f64; 3]]) {
        let n = clips.len().min(MAX_CLIPS);
        for (dst, c) in self.clips.iter_mut().zip(&clips[..n]) {
            *dst = [c[0] as f32, c[1] as f32, c[2] as f32, 0.0];
        }
        self.extra[0] = n as u32;
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
    /// The output conversion into `Rgba8Unorm`.
    pub output: wgpu::RenderPipeline,
    output_module: wgpu::ShaderModule,
    output_pipeline_layout: wgpu::PipelineLayout,
    /// Output conversions into other formats (export), created on first use.
    outputs: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Mask passes, created on first use.
    masks: Option<MaskPipelines>,
    /// Adjustment layer mix pass, created on first use.
    pub adjust: Option<crate::exec::adjust::MixPipelines>,
}

impl Pipelines {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.composite"),
            source: wgpu::ShaderSource::Wgsl(wgsl!("composite.wgsl").into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        // Draw records and motion-blur samples.
        let draws_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.draws"),
            entries: &[storage(0), storage(1)],
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
                tex(4),
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&dummy),
                },
            ],
        });

        let output_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.output"),
            source: wgpu::ShaderSource::Wgsl(wgsl!("output.wgsl").into()),
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
        let output = output_pipeline(
            device,
            &output_module,
            &output_pipeline_layout,
            wgpu::TextureFormat::Rgba8Unorm,
        );

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
            output_module,
            output_pipeline_layout,
            outputs: FxHashMap::default(),
            masks: None,
            adjust: None,
        }
    }

    /// The output conversion pipeline rendering into `format`.
    pub fn output_for(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        if format == wgpu::TextureFormat::Rgba8Unorm {
            return &self.output;
        }
        let (module, layout) = (&self.output_module, &self.output_pipeline_layout);
        self.outputs
            .entry(format)
            .or_insert_with(|| output_pipeline(device, module, layout, format))
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
        mask: Option<&wgpu::TextureView>,
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(mask.unwrap_or(&self.dummy)),
                },
            ],
        })
    }

    /// The mask pipelines (compiled on first use).
    pub fn masks(&mut self, device: &wgpu::Device) -> &mut MaskPipelines {
        self.masks.get_or_insert_with(|| MaskPipelines::new(device))
    }
}

/// How a mask pass writes its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MaskBlend {
    /// Overwrite (resolve and blur passes).
    Replace,
    /// Merge into the accumulator with a mask mode (AE-CMP-038).
    Mode(MaskMode),
}

impl MaskBlend {
    fn state(self) -> Option<wgpu::BlendState> {
        use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
        let c = |src_factor, dst_factor, operation| C {
            src_factor,
            dst_factor,
            operation,
        };
        let component = match self {
            MaskBlend::Replace | MaskBlend::Mode(MaskMode::None) => return None,
            // acc + m − acc·m
            MaskBlend::Mode(MaskMode::Add) => c(F::One, F::OneMinusSrc, O::Add),
            // acc · (1 − m)
            MaskBlend::Mode(MaskMode::Subtract) => c(F::Zero, F::OneMinusSrc, O::Add),
            // acc · m
            MaskBlend::Mode(MaskMode::Intersect) => c(F::Zero, F::Src, O::Add),
            MaskBlend::Mode(MaskMode::Lighten) => c(F::One, F::One, O::Max),
            MaskBlend::Mode(MaskMode::Darken) => c(F::One, F::One, O::Min),
            // m·(1 − acc) + acc·(1 − m) = acc + m − 2·acc·m
            MaskBlend::Mode(MaskMode::Difference) => c(F::OneMinusDst, F::OneMinusSrc, O::Add),
        };
        Some(wgpu::BlendState {
            color: component,
            alpha: component,
        })
    }
}

/// One mask edge, laid out like `struct Edge` in `mask.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct EdgeData {
    pub p0: [f32; 2],
    pub p1: [f32; 2],
    pub xmax: f32,
    pub chan: u32,
    pub size: [f32; 2],
}

/// One full-screen mask pass, laid out like `struct Pass` in `mask.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct MaskPassData {
    pub map: [f32; 4],
    pub src: [f32; 4],
    pub blur: [f32; 4],
    pub clip: [f32; 4],
    pub value: [f32; 4],
    pub mode: [u32; 4],
    pub src_rect: [f32; 4],
}

/// Pipelines of `mask.wgsl`.
pub(crate) struct MaskPipelines {
    module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pub layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    pub edges: wgpu::RenderPipeline,
    passes: FxHashMap<MaskBlend, wgpu::RenderPipeline>,
}

impl MaskPipelines {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.mask"),
            source: wgpu::ShaderSource::Wgsl(wgsl!("mask.wgsl").into()),
        });
        let storage = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.mask"),
            entries: &[
                storage(0),
                storage(1),
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
            label: Some("bae-render.mask"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.mask"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let additive = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let edges = mask_pipeline(
            device,
            &module,
            &pipeline_layout,
            ("vs_edge", "fs_edge"),
            MASK_ACCUM_FORMAT,
            Some(wgpu::BlendState {
                color: additive,
                alpha: additive,
            }),
        );
        Self {
            module,
            pipeline_layout,
            layout,
            sampler,
            edges,
            passes: FxHashMap::default(),
        }
    }

    /// The full-screen pass writing `MASK_FORMAT` with `blend`.
    pub fn pass(&mut self, device: &wgpu::Device, blend: MaskBlend) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.passes.entry(blend).or_insert_with(|| {
            mask_pipeline(
                device,
                module,
                layout,
                ("vs_full", "fs_pass"),
                MASK_FORMAT,
                blend.state(),
            )
        })
    }
}

fn mask_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (vs, fs): (&str, &str),
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.mask"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// A full-screen pass converting a working buffer into `format` (`output.wgsl`).
fn output_pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.output"),
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
            entry_point: Some("fs_out"),
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
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;

    #[test]
    fn draw_record_matches_wgsl_layout() {
        // 21 vec4s: the WGSL struct stride.
        assert_eq!(std::mem::size_of::<DrawData>(), 336);
        assert_eq!(std::mem::size_of::<SampleData>(), 48);
        assert_eq!(std::mem::size_of::<EdgeData>(), 32);
        assert_eq!(std::mem::size_of::<MaskPassData>(), 112);
    }

    #[test]
    fn shaders_ship_without_comments() {
        const SRC: &[u8] = b"// header\n  let a = 1; // note\n\n\tfn f() {}\n";
        const LEN: usize = minified_len(SRC);
        assert_eq!(&minify::<LEN>(SRC), b"let a = 1;\nfn f() {}\n");
        let composite: &str = wgsl!("composite.wgsl");
        assert!(!composite.contains("//") && composite.contains("fn fs_main"));
        assert!(composite.len() < include_str!("composite.wgsl").len() * 3 / 4);
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

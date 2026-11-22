//! GPU side of decoding: uploads CPU frames and converts decoder textures into
//! the `Rgba16Float` working format ([`WORKING_FORMAT`]).
//!
//! One [`MediaGpu`] per [`GpuContext`]; it holds the conversion pipelines. The
//! conversion is a compute pass that reads the source texel (8-bit unorm, BGRA,
//! 16-bit uint, half or float), scales it to `0..1`, applies the alpha
//! conversion (premultiply / unpremultiply / force opaque) and stores half
//! floats. Values are not color-converted (that is `bae-color`'s job).
//!
//! Hardware video decoders hand their native textures (IOSurface-backed Metal
//! textures, D3D shared textures) to [`MediaGpu::convert_texture`], so a frame
//! goes from the decoder to the working texture without touching the CPU.

use std::sync::Arc;

use bae_gpu::wgpu::util::DeviceExt as _;
use bae_gpu::{GpuContext, GpuError, transfer, wgpu};

use crate::frame::{AlphaOutput, CpuFrame, FrameRequest, GpuFrame, PixelFormat, WORKING_FORMAT};
use crate::meta::{AlphaMode, ColorInfo, PixelAspect};
use crate::source::MediaError;

const SHADER: &str = r"
struct Params {
    // 0 = keep, 1 = premultiply, 2 = unpremultiply, 3 = force opaque
    alpha_op: u32,
    // Multiplier applied to the loaded texel (1/65535 for 16-bit uint input).
    scale: f32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var src_float: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var src_uint: texture_2d<u32>;

fn finish(texel: vec4<f32>) -> vec4<f32> {
    var c = texel * params.scale;
    switch params.alpha_op {
        case 1u: { c = vec4<f32>(c.rgb * c.a, c.a); }
        case 2u: { if (c.a > 0.0) { c = vec4<f32>(c.rgb / c.a, c.a); } }
        case 3u: { c.a = 1.0; }
        default: {}
    }
    return c;
}

@compute @workgroup_size(8, 8)
fn convert_float(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(dst);
    if (id.x >= size.x || id.y >= size.y) { return; }
    textureStore(dst, id.xy, finish(textureLoad(src_float, id.xy, 0)));
}

@compute @workgroup_size(8, 8)
fn convert_uint(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(dst);
    if (id.x >= size.x || id.y >= size.y) { return; }
    textureStore(dst, id.xy, finish(vec4<f32>(textureLoad(src_uint, id.xy, 0))));
}
";

/// Alpha operation of the conversion pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum AlphaOp {
    Keep = 0,
    Premultiply = 1,
    Unpremultiply = 2,
    ForceOpaque = 3,
}

/// What the conversion does to alpha, and the alpha tag of the result.
fn alpha_plan(source: AlphaMode, output: AlphaOutput) -> (AlphaOp, AlphaMode) {
    match (source, output) {
        (AlphaMode::Opaque | AlphaMode::Ignore, _) => (AlphaOp::ForceOpaque, AlphaMode::Opaque),
        (AlphaMode::Straight, AlphaOutput::Premultiplied) => {
            (AlphaOp::Premultiply, AlphaMode::Premultiplied)
        }
        (AlphaMode::Straight, AlphaOutput::Straight) => (AlphaOp::Keep, AlphaMode::Straight),
        (AlphaMode::Premultiplied, AlphaOutput::Premultiplied) => {
            (AlphaOp::Keep, AlphaMode::Premultiplied)
        }
        (AlphaMode::Premultiplied, AlphaOutput::Straight) => {
            (AlphaOp::Unpremultiply, AlphaMode::Straight)
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    alpha_op: u32,
    scale: f32,
    _pad: [u32; 2],
}

/// Per-device media resources: conversion pipelines.
#[derive(Debug)]
pub struct MediaGpu {
    ctx: Arc<GpuContext>,
    float_layout: wgpu::BindGroupLayout,
    uint_layout: wgpu::BindGroupLayout,
    float_pipeline: wgpu::ComputePipeline,
    uint_pipeline: wgpu::ComputePipeline,
    /// Export conversion pipelines, created on first use per output format.
    export_pipelines: parking_lot::Mutex<
        std::collections::HashMap<PixelFormat, (wgpu::BindGroupLayout, wgpu::ComputePipeline)>,
    >,
}

const EXPORT_SHADER: &str = r"
struct Params {
    // 0 keep, 1 premultiply, 2 unpremultiply, 3 force opaque,
    // 4 premultiply then force opaque (straight → RGB over black)
    op: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<FORMAT, write>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(dst);
    if (id.x >= size.x || id.y >= size.y) { return; }
    var c = textureLoad(src, id.xy, 0);
    switch params.op {
        case 1u: { c = vec4<f32>(c.rgb * c.a, c.a); }
        case 2u: { if (c.a > 0.0) { c = vec4<f32>(c.rgb / c.a, c.a); } }
        case 3u: { c.a = 1.0; }
        case 4u: { c = vec4<f32>(c.rgb * c.a, 1.0); }
        default: {}
    }
    textureStore(dst, id.xy, STORE);
}
";

/// Storage format, store expression and texture format of an export target.
fn export_target(format: PixelFormat) -> (&'static str, &'static str, wgpu::TextureFormat) {
    match format {
        PixelFormat::Rgba8 => ("rgba8unorm", "c", wgpu::TextureFormat::Rgba8Unorm),
        // Swizzled into an RGBA8 texture: the bytes come out B G R A, no
        // BGRA storage feature needed.
        PixelFormat::Bgra8 => ("rgba8unorm", "c.bgra", wgpu::TextureFormat::Rgba8Unorm),
        PixelFormat::Rgba16 => (
            "rgba16uint",
            "vec4<u32>(round(clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * 65535.0))",
            wgpu::TextureFormat::Rgba16Uint,
        ),
        PixelFormat::Rgba16Float => ("rgba16float", "c", wgpu::TextureFormat::Rgba16Float),
        PixelFormat::Rgba32Float => ("rgba32float", "c", wgpu::TextureFormat::Rgba32Float),
    }
}

/// The alpha operation turning `from` into `to` for export.
fn export_alpha_op(from: AlphaMode, to: AlphaMode) -> u32 {
    let from_opaque = matches!(from, AlphaMode::Opaque | AlphaMode::Ignore);
    match to {
        AlphaMode::Opaque | AlphaMode::Ignore if from == AlphaMode::Straight => 4,
        AlphaMode::Opaque | AlphaMode::Ignore => 3,
        _ if from_opaque => 3,
        AlphaMode::Straight if from == AlphaMode::Premultiplied => 2,
        AlphaMode::Premultiplied if from == AlphaMode::Straight => 1,
        _ => 0,
    }
}

/// A frame converted for an encoder on the GPU, ready to be read back
/// ([`FrameReadback::submit_export`](crate::FrameReadback::submit_export)).
#[derive(Clone, Debug)]
pub struct ExportTexture {
    /// The converted texture (`COPY_SRC`).
    pub texture: wgpu::Texture,
    /// Memory layout of the texture's bytes.
    pub format: PixelFormat,
    /// Alpha of the converted pixels.
    pub alpha: AlphaMode,
    /// Color tags (unchanged).
    pub color: ColorInfo,
    /// Pixel aspect (unchanged).
    pub pixel_aspect: PixelAspect,
}

/// Description of a texture to convert with [`MediaGpu::convert_texture`].
#[derive(Clone, Debug)]
pub struct SourceTexture<'a> {
    /// A view of the decoder's texture. Float-sampled formats (`*Unorm`,
    /// `*Float`) and `Rgba16Uint` are accepted.
    pub view: &'a wgpu::TextureView,
    /// Format of the viewed texture.
    pub format: wgpu::TextureFormat,
    /// Size of the picture (may be smaller than the texture, e.g. 1080 lines in
    /// a 1088-line decoder buffer).
    pub width: u32,
    /// See [`Self::width`].
    pub height: u32,
    /// Alpha of the texels.
    pub alpha: AlphaMode,
    /// Color tags passed through to the frame.
    pub color: ColorInfo,
    /// Pixel aspect passed through to the frame.
    pub pixel_aspect: PixelAspect,
}

impl MediaGpu {
    /// Creates the pipelines on `ctx`'s device.
    pub fn new(ctx: Arc<GpuContext>) -> Self {
        let device = ctx.device();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae.media.convert"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = |label: &str, src_binding: u32, sample_type: wgpu::TextureSampleType| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: src_binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: WORKING_FORMAT,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            })
        };
        let float_layout = layout(
            "bae.media.convert.float",
            0,
            wgpu::TextureSampleType::Float { filterable: false },
        );
        let uint_layout = layout("bae.media.convert.uint", 3, wgpu::TextureSampleType::Uint);
        let pipeline = |label: &str, bind_layout: &wgpu::BindGroupLayout, entry: &str| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(bind_layout)],
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
        };
        let float_pipeline = pipeline("bae.media.convert.float", &float_layout, "convert_float");
        let uint_pipeline = pipeline("bae.media.convert.uint", &uint_layout, "convert_uint");
        MediaGpu {
            ctx,
            float_layout,
            uint_layout,
            float_pipeline,
            uint_pipeline,
            export_pipelines: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn export_pipeline(
        &self,
        format: PixelFormat,
    ) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
        let mut pipelines = self.export_pipelines.lock();
        if let Some(entry) = pipelines.get(&format) {
            return entry.clone();
        }
        let (storage, store, texture_format) = export_target(format);
        let device = self.ctx.device();
        let source = EXPORT_SHADER
            .replace("FORMAT", storage)
            .replace("STORE", store);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae.media.export"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae.media.export"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: texture_format,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae.media.export"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("bae.media.export"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        pipelines.insert(format, (layout.clone(), pipeline.clone()));
        (layout, pipeline)
    }

    /// Converts a frame on the GPU into the layout and alpha an encoder wants
    /// ([`Encoder::input_format`](crate::Encoder::input_format)), so readback
    /// moves only the final bytes and the CPU does no per-pixel work.
    /// `AlphaMode::Opaque` keeps color premultiplied over black and sets alpha
    /// to 1 (AE's "RGB" output).
    pub fn convert_for_export(
        &self,
        frame: &GpuFrame,
        format: PixelFormat,
        alpha: AlphaMode,
    ) -> Result<ExportTexture, MediaError> {
        let (layout, pipeline) = self.export_pipeline(format);
        let (_, _, texture_format) = export_target(format);
        let device = self.ctx.device();
        let size = wgpu::Extent3d {
            width: frame.width(),
            height: frame.height(),
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bae.media.export"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let params: [u32; 4] = [export_alpha_op(frame.alpha, alpha), 0, 0, 0];
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bae.media.export.params"),
            contents: bytemuck::cast_slice(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let src = frame.view();
        let dst = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae.media.export"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dst),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bae.media.export"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bae.media.export"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(size.width.div_ceil(8), size.height.div_ceil(8), 1);
        }
        self.ctx.queue().submit([encoder.finish()]);
        Ok(ExportTexture {
            texture,
            format,
            alpha: if alpha == AlphaMode::Ignore {
                AlphaMode::Opaque
            } else {
                alpha
            },
            color: frame.color.clone(),
            pixel_aspect: frame.pixel_aspect,
        })
    }

    /// The shared GPU context.
    pub fn context(&self) -> &Arc<GpuContext> {
        &self.ctx
    }

    /// Creates an empty working-format texture.
    pub fn create_frame_texture(
        &self,
        width: u32,
        height: u32,
    ) -> Result<wgpu::Texture, MediaError> {
        let max = self.ctx.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(MediaError::Unsupported(format!(
                "{width}×{height} frame (GPU texture limit {max})"
            )));
        }
        Ok(self.ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("bae.media.frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: WORKING_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }))
    }

    /// Uploads a CPU frame and converts it to the working format, honoring
    /// `request.alpha_override` and `request.alpha_output`.
    pub fn upload(&self, frame: &CpuFrame, request: &FrameRequest) -> Result<GpuFrame, MediaError> {
        if frame.data.len() != frame.expected_len() {
            return Err(MediaError::Unsupported(format!(
                "{} bytes for a {}×{} {:?} frame",
                frame.data.len(),
                frame.width,
                frame.height,
                frame.format
            )));
        }
        let source_alpha = request.alpha_override.unwrap_or(frame.alpha);
        let (op, alpha) = alpha_plan(source_alpha, request.alpha_output);
        let out = self.create_frame_texture(frame.width, frame.height)?;
        let gpu_frame = GpuFrame {
            texture: out,
            alpha,
            color: frame.color.clone(),
            pixel_aspect: frame.pixel_aspect,
        };
        let full = transfer::TextureRegion::full(&gpu_frame.texture);
        if frame.format == PixelFormat::Rgba16Float && op == AlphaOp::Keep {
            // Already in the working format: one copy, no pass.
            transfer::upload_texture(&self.ctx, &gpu_frame.texture, full, &frame.data)?;
            return Ok(gpu_frame);
        }
        let upload_format = frame.format.upload_format();
        let staging = transfer::create_texture_with_data(
            &self.ctx,
            &wgpu::TextureDescriptor {
                label: Some("bae.media.upload"),
                size: full.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: upload_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            &frame.data,
        )?;
        let view = staging.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bae.media.upload"),
                });
        self.record_convert(&mut encoder, &view, upload_format, &gpu_frame.texture, op)?;
        self.ctx.queue().submit([encoder.finish()]);
        Ok(gpu_frame)
    }

    /// Converts a decoder texture into a new working-format frame (the zero-copy
    /// path of hardware video decoders). Submits its own command buffer.
    pub fn convert_texture(
        &self,
        source: &SourceTexture<'_>,
        request: &FrameRequest,
    ) -> Result<GpuFrame, MediaError> {
        let source_alpha = request.alpha_override.unwrap_or(source.alpha);
        let (op, alpha) = alpha_plan(source_alpha, request.alpha_output);
        let texture = self.create_frame_texture(source.width, source.height)?;
        let mut encoder =
            self.ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bae.media.convert"),
                });
        self.record_convert(&mut encoder, source.view, source.format, &texture, op)?;
        self.ctx.queue().submit([encoder.finish()]);
        Ok(GpuFrame {
            texture,
            alpha,
            color: source.color.clone(),
            pixel_aspect: source.pixel_aspect,
        })
    }

    fn record_convert(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        src_format: wgpu::TextureFormat,
        dst: &wgpu::Texture,
        op: AlphaOp,
    ) -> Result<(), MediaError> {
        let (layout, pipeline, src_binding, scale) = match src_format.sample_type(None, None) {
            Some(wgpu::TextureSampleType::Float { .. }) => {
                (&self.float_layout, &self.float_pipeline, 0, 1.0)
            }
            Some(wgpu::TextureSampleType::Uint)
                if src_format == wgpu::TextureFormat::Rgba16Uint =>
            {
                (&self.uint_layout, &self.uint_pipeline, 3, 1.0 / 65535.0)
            }
            _ => {
                return Err(MediaError::Unsupported(format!(
                    "conversion from {src_format:?}"
                )));
            }
        };
        let params = Params {
            alpha_op: op as u32,
            scale,
            _pad: [0; 2],
        };
        let device = self.ctx.device();
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bae.media.convert.params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae.media.convert"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: src_binding,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dst_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("bae.media.convert"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(dst.width().div_ceil(8), dst.height().div_ceil(8), 1);
        Ok(())
    }

    /// Reads a frame back to the CPU (blocking): thumbnails, tests, Agent
    /// screenshots. The result is [`PixelFormat::Rgba16Float`].
    pub fn read_back(&self, frame: &GpuFrame) -> Result<CpuFrame, MediaError> {
        let image = transfer::read_texture(&self.ctx, &frame.texture)?;
        if image.format != WORKING_FORMAT {
            return Err(GpuError::Unsupported(format!("read back {:?}", image.format)).into());
        }
        Ok(CpuFrame {
            width: image.width,
            height: image.height,
            format: PixelFormat::Rgba16Float,
            data: image.data,
            alpha: frame.alpha,
            color: frame.color.clone(),
            pixel_aspect: frame.pixel_aspect,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_plans() {
        use AlphaMode as M;
        use AlphaOutput as O;
        assert_eq!(
            alpha_plan(M::Straight, O::Premultiplied),
            (AlphaOp::Premultiply, M::Premultiplied)
        );
        assert_eq!(
            alpha_plan(M::Premultiplied, O::Straight),
            (AlphaOp::Unpremultiply, M::Straight)
        );
        assert_eq!(
            alpha_plan(M::Premultiplied, O::Premultiplied),
            (AlphaOp::Keep, M::Premultiplied)
        );
        assert_eq!(
            alpha_plan(M::Ignore, O::Straight),
            (AlphaOp::ForceOpaque, M::Opaque)
        );
        assert_eq!(alpha_plan(M::Opaque, O::Premultiplied).1, M::Opaque);
    }

    #[test]
    fn export_alpha_ops() {
        use AlphaMode as M;
        assert_eq!(export_alpha_op(M::Premultiplied, M::Opaque), 3);
        assert_eq!(export_alpha_op(M::Straight, M::Opaque), 4);
        assert_eq!(export_alpha_op(M::Premultiplied, M::Straight), 2);
        assert_eq!(export_alpha_op(M::Straight, M::Premultiplied), 1);
        assert_eq!(export_alpha_op(M::Premultiplied, M::Premultiplied), 0);
        assert_eq!(export_alpha_op(M::Opaque, M::Straight), 3);
    }
}

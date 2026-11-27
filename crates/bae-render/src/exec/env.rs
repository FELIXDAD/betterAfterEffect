//! Environment light images (`gpu/env.wgsl`): the source layer's picture
//! resampled into an equirectangular mip chain (reflections: rougher
//! surfaces read smaller levels) and a small diffuse irradiance map, built
//! once per source image and kept while it stays the same; and the picture
//! drawn behind the layers through the camera ([`crate::graph::SkyNode`]).

use bae_gpu::wgpu;
use bae_gpu::wgpu::util::DeviceExt;
use glam::{DMat3, DMat4};
use rustc_hash::FxHashMap;

use crate::camera::Projection;
use crate::graph::SkyNode;
use crate::source::GpuImage;

/// Format of the environment maps (linear light, overbright kept).
pub(crate) const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Size of the irradiance map (diffuse light varies slowly).
const IRRADIANCE: [u32; 2] = [32, 16];
/// Width of the level the irradiance integrates (2048 texels per texel).
const IRRADIANCE_SOURCE: u32 = 64;
/// Smallest level width.
const SMALLEST: u32 = 8;
/// Maps kept (several environment lights or a changing source); maps a
/// frame uses are never dropped during it.
const CACHED: usize = 2;

/// One environment light's maps.
#[derive(Clone)]
pub(crate) struct EnvMap {
    key: u64,
    /// The frame that last used it.
    used: u64,
    /// Every level.
    pub(crate) view: wgpu::TextureView,
    pub(crate) irradiance: wgpu::TextureView,
    pub(crate) levels: u32,
}

/// The environment pipelines and the maps built.
pub(crate) struct EnvPipelines {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    level0: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    irradiance: wgpu::RenderPipeline,
    sky: FxHashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    /// Source images (clamped, bilinear).
    clamped: wgpu::Sampler,
    /// Environment maps: u wraps around, v stops at the poles; trilinear.
    pub(crate) sampler: wgpu::Sampler,
    maps: Vec<EnvMap>,
    frame: u64,
}

impl EnvPipelines {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bae-render.env"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../gpu/env.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bae-render.env"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bae-render.env"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, format: wgpu::TextureFormat, blend| {
            fullscreen(device, &module, &pipeline_layout, entry, format, blend)
        };
        let level0 = pipeline("fs_level0", FORMAT, None);
        let down = pipeline("fs_down", FORMAT, None);
        let irradiance = pipeline("fs_irradiance", FORMAT, None);
        let clamped = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.env.source"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bae-render.env.map"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            module,
            layout,
            pipeline_layout,
            level0,
            down,
            irradiance,
            sky: FxHashMap::default(),
            clamped,
            sampler,
            maps: Vec::new(),
            frame: 0,
        }
    }

    /// Compiles the background pipeline for `format` (the warm-up; the map
    /// pipelines are built with `self`).
    pub(crate) fn warm(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        self.sky_pipeline(device, format);
    }

    fn sky_pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        let (module, layout) = (&self.module, &self.pipeline_layout);
        self.sky
            .entry(format)
            .or_insert_with(|| fullscreen(device, module, layout, "fs_sky", format, None))
    }

    fn bind(
        &self,
        device: &wgpu::Device,
        params: &[f32; 24],
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bae-render.env"),
            contents: bytemuck::cast_slice(params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.env"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Starts a frame: the maps it reports as built ([`Self::has`]) stay
    /// until it ends.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Whether the maps of the picture node `key` are built (the frame then
    /// does not draw the picture again).
    pub(crate) fn has(&mut self, key: u64, width: u32, linear: bool) -> bool {
        let key = crate::graph::hash_of(&(key, width, linear));
        let frame = self.frame;
        self.maps
            .iter_mut()
            .find(|m| m.key == key)
            .map(|m| m.used = frame)
            .is_some()
    }

    /// The maps of `source` (an equirectangular picture), `width` texels wide
    /// at level 0: built on first use, then kept while `key` (the source
    /// node's key) stays the same. `None` when they are not built and the
    /// picture was not drawn.
    pub(crate) fn map(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        key: u64,
        source: Option<&GpuImage>,
        width: u32,
        linear: bool,
    ) -> Option<EnvMap> {
        let key = crate::graph::hash_of(&(key, width, linear));
        let frame = self.frame;
        if let Some(m) = self.maps.iter_mut().find(|m| m.key == key) {
            m.used = frame;
            return Some(m.clone());
        }
        let source = source?;
        let width = width.next_power_of_two().clamp(SMALLEST * 2, 4096);
        let levels = (width / SMALLEST).ilog2() + 1;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bae-render.env.map"),
            size: wgpu::Extent3d {
                width,
                height: width / 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let level = |k: u32| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: k,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let [tw, th] = source.texture_size();
        let [sw, sh] = source.texel_size();
        let mut params = [0.0f32; 24];
        params[0] = sw as f32 / tw.max(1) as f32;
        params[1] = sh as f32 / th.max(1) as f32;
        params[2] = f32::from(u8::from(!linear));
        params[3] = (sw as f32 / width as f32).max(1.0);
        let group = self.bind(device, &params, &source.view, &self.clamped);
        draw(encoder, &self.level0, &group, &level(0));
        for k in 1..levels {
            let group = self.bind(device, &[0.0; 24], &level(k - 1), &self.clamped);
            draw(encoder, &self.down, &group, &level(k));
        }
        let irradiance = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("bae-render.env.irradiance"),
                size: wgpu::Extent3d {
                    width: IRRADIANCE[0],
                    height: IRRADIANCE[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let from = (width / IRRADIANCE_SOURCE).max(1).ilog2().min(levels - 1);
        let group = self.bind(device, &[0.0; 24], &level(from), &self.clamped);
        draw(encoder, &self.irradiance, &group, &irradiance);
        let map = EnvMap {
            key,
            used: frame,
            view: texture.create_view(&Default::default()),
            irradiance,
            levels,
        };
        if self.maps.len() >= CACHED
            && let Some(oldest) = self
                .maps
                .iter()
                .enumerate()
                .filter(|(_, m)| m.used != frame)
                .min_by_key(|(_, m)| m.used)
                .map(|(i, _)| i)
        {
            self.maps.remove(oldest);
        }
        self.maps.push(map.clone());
        Some(map)
    }

    /// Draws the picture of `map` behind the layers into `target`
    /// (transparent without a map).
    pub(crate) fn sky(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        format: wgpu::TextureFormat,
        node: &SkyNode,
        map: Option<&EnvMap>,
        target: &GpuImage,
    ) {
        let Some(map) = map else {
            clear(encoder, &target.view);
            return;
        };
        let s = target.pixel_scale;
        let r = target.rect;
        let rays = DMat3::from_mat4(node.environment.rotation)
            * DMat3::from_mat4(node.camera.view()).transpose();
        let rows = rays.transpose();
        let mut params = [0.0f32; 24];
        for (i, row) in [rows.x_axis, rows.y_axis, rows.z_axis].iter().enumerate() {
            params[4 + i * 4..7 + i * 4].copy_from_slice(&row.to_array().map(|v| v as f32));
        }
        let (w, h) = (f64::from(node.comp_size[0]), f64::from(node.comp_size[1]));
        params[16] = ((w / 2.0 - f64::from(r.x0)) / s) as f32;
        params[17] = ((h / 2.0 - f64::from(r.y0)) / s) as f32;
        match node.camera.projection {
            Projection::Perspective => params[18] = (s / node.camera.zoom.max(1e-6)) as f32,
            Projection::Orthographic { .. } => params[19] = 1.0,
        }
        params[20] = node.opacity;
        params[21] = node.blur * (map.levels - 1) as f32;
        params[22] = f32::from(u8::from(!node.linear));
        let group = self.bind(device, &params, &map.view, &self.sampler);
        let pipeline = self.sky_pipeline(device, format).clone();
        let [tw, th] = target.texel_size();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bae-render.env.sky"),
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
        pass.set_viewport(0.0, 0.0, tw.max(1) as f32, th.max(1) as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, tw.max(1), th.max(1));
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Comp directions → environment directions: the light's rotation undone
/// (its scale ignored).
pub(crate) fn rotation(world: &DMat4) -> DMat4 {
    let m = DMat3::from_mat4(*world);
    let r = DMat3::from_cols(
        m.x_axis.normalize_or(glam::DVec3::X),
        m.y_axis.normalize_or(glam::DVec3::Y),
        m.z_axis.normalize_or(glam::DVec3::Z),
    );
    DMat4::from_mat3(r.transpose())
}

fn fullscreen(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    entry: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bae-render.env"),
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
            entry_point: Some(entry),
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

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    target: &wgpu::TextureView,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.env"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
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
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}

fn clear(encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bae-render.env.clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
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
}

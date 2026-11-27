//! Display colour management of viewers (AE's View ▸ Use Display Color
//! Management and Simulate Output): a `bae-color` transform from buffer
//! values to the display, spliced into the viewer's output pass
//! (`output.wgsl` calls `display_color`). Only viewer output applies it;
//! exports and read-backs never do.
//!
//! The WGSL of a transform depends only on its structure
//! ([`ColorTransform::structure_key`]): one pipeline per structure, shared by
//! every renderer of the device, compiled at warm-up for the common working
//! spaces or on a background thread when another one comes (frames are
//! shown unconverted until it is ready, then the viewers redraw). Uniform
//! values and LUT textures are per transform.

use std::sync::Arc;

use bae_color::{ColorTransform, GpuSnippet, LutDimension};
use bae_gpu::wgpu;
use bae_gpu::wgpu::util::DeviceExt;
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

const PREFIX: &str = "dc_";

/// `output.wgsl`'s hook in the plain pipeline: buffer values as they are.
pub(crate) const IDENTITY: &str = "fn display_color(c:vec4<f32>)->vec4<f32>{return c;}";

/// A compiled output pipeline for one transform structure.
pub(crate) struct Compiled {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

/// The display pipelines of a device, by transform structure (`None`: being
/// compiled).
#[derive(Default)]
pub(crate) struct DisplayColorPipelines {
    compiled: Mutex<FxHashMap<u64, Option<Arc<Compiled>>>>,
}

/// A transform ready to draw with: its pipeline and bound values.
pub(crate) struct DisplayColor {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) group: wgpu::BindGroup,
}

fn source(snippet: &GpuSnippet) -> String {
    let mut s = String::from(include_str!("gpu/output.wgsl"));
    s.push('\n');
    s.push_str(&snippet.wgsl);
    s.push_str(&format!(
        "\n@group(1) @binding(0) var<uniform> dc_params: {};\n",
        snippet.params_type()
    ));
    let mut names = Vec::new();
    for (i, t) in snippet.textures.iter().enumerate() {
        let ty = match t.dimension {
            LutDimension::D2 => "texture_2d<f32>",
            LutDimension::D3 => "texture_3d<f32>",
        };
        s.push_str(&format!(
            "@group(1) @binding({}) var dc_lut{i}: {ty};\n",
            i + 1
        ));
        names.push(format!("dc_lut{i}"));
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    s.push_str(&format!(
        "fn display_color(c: vec4<f32>) -> vec4<f32> {{ return {}; }}\n",
        snippet.call("c", "dc_params", &names, true)
    ));
    s
}

fn compile(
    device: &wgpu::Device,
    output_layout: &wgpu::BindGroupLayout,
    snippet: &GpuSnippet,
) -> Compiled {
    let mut entries = vec![wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    for (i, t) in snippet.textures.iter().enumerate() {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: i as u32 + 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: match t.dimension {
                    LutDimension::D2 => wgpu::TextureViewDimension::D2,
                    LutDimension::D3 => wgpu::TextureViewDimension::D3,
                },
                multisampled: false,
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bae-render.display-color"),
        entries: &entries,
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bae-render.display-color"),
        source: wgpu::ShaderSource::Wgsl(source(snippet).into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bae-render.display-color"),
        bind_group_layouts: &[Some(output_layout), Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = crate::gpu::output_pipeline(
        device,
        &module,
        &pipeline_layout,
        wgpu::TextureFormat::Rgba8Unorm,
    );
    Compiled { layout, pipeline }
}

impl DisplayColorPipelines {
    /// Compiles the pipeline of `transform`'s structure now (the warm-up).
    pub(crate) fn warm(
        &self,
        device: &wgpu::Device,
        output_layout: &wgpu::BindGroupLayout,
        transform: &ColorTransform,
    ) {
        let key = transform.structure_key();
        if self.compiled.lock().contains_key(&key) {
            return;
        }
        let compiled = Arc::new(compile(device, output_layout, &transform.to_wgsl(PREFIX)));
        self.compiled.lock().insert(key, Some(compiled));
    }

    /// `transform` ready to draw with, or `None` while its pipeline compiles
    /// on a background thread (the viewers redraw when it is ready).
    pub(crate) fn prepare(
        self: &Arc<Self>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_layout: &wgpu::BindGroupLayout,
        transform: &ColorTransform,
    ) -> Option<DisplayColor> {
        let key = transform.structure_key();
        let snippet = transform.to_wgsl(PREFIX);
        let compiled = {
            let mut all = self.compiled.lock();
            match all.get(&key) {
                Some(Some(c)) => c.clone(),
                Some(None) => return None,
                None => {
                    all.insert(key, None);
                    let (me, device, layout, snippet) = (
                        self.clone(),
                        device.clone(),
                        output_layout.clone(),
                        snippet.clone(),
                    );
                    let spawned = std::thread::Builder::new()
                        .name("bae-display-color".into())
                        .spawn(move || {
                            let compiled = Arc::new(compile(&device, &layout, &snippet));
                            me.compiled.lock().insert(key, Some(compiled));
                            bae_gpu::warm_up::request_redraw();
                        });
                    if spawned.is_err() {
                        all.remove(&key);
                    }
                    return None;
                }
            }
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bae-render.display-color"),
            contents: &snippet.params_bytes(),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let views: Vec<wgpu::TextureView> = snippet
            .textures
            .iter()
            .map(|t| {
                let (dimension, view_dimension) = match t.dimension {
                    LutDimension::D2 => {
                        (wgpu::TextureDimension::D2, wgpu::TextureViewDimension::D2)
                    }
                    LutDimension::D3 => {
                        (wgpu::TextureDimension::D3, wgpu::TextureViewDimension::D3)
                    }
                };
                device
                    .create_texture_with_data(
                        queue,
                        &wgpu::TextureDescriptor {
                            label: Some("bae-render.display-color.lut"),
                            size: wgpu::Extent3d {
                                width: t.width,
                                height: t.height,
                                depth_or_array_layers: t.depth,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension,
                            format: wgpu::TextureFormat::Rgba32Float,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        },
                        wgpu::util::TextureDataOrder::LayerMajor,
                        &t.bytes(),
                    )
                    .create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(view_dimension),
                        ..Default::default()
                    })
            })
            .collect();
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }];
        for (i, view) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32 + 1,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bae-render.display-color"),
            layout: &compiled.layout,
            entries: &entries,
        });
        Some(DisplayColor {
            pipeline: compiled.pipeline.clone(),
            group,
        })
    }
}

/// The display transforms the warm-up compiles: the working spaces of the
/// Project Settings dialog (and their linearized forms) on an sRGB display.
pub(crate) fn common_transforms() -> Vec<ColorTransform> {
    use bae_color::{ColorContext, WorkingSpace, builtin};
    let spaces = [
        builtin::SRGB,
        builtin::REC709,
        builtin::ADOBE_RGB,
        builtin::DISPLAY_P3,
        builtin::ACESCG,
    ];
    spaces
        .into_iter()
        .flat_map(|s| [false, true].map(move |lin| (s.clone(), lin)))
        .filter_map(|(space, linear)| {
            ColorContext::new(WorkingSpace::managed(space).linearized(linear))
                .display_transform()
                .ok()
        })
        .filter(|t| !t.is_identity())
        .collect()
}

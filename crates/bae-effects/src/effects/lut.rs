//! Apply an embedded LUT with the reader, WGSL and texture data from bae-color.
use super::image_grid;
use crate::gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter};
use crate::{Builtin, EffectTarget, TargetEffect, catalog::DefBuilder, rust_builtin};
use bae_color::LutDimension;
use bae_gpu::{
    GpuContext,
    wgpu::{self, util::DeviceExt},
};
use bae_model::{EffectDef, ParamKind, Value};
use bae_render::{Effect, EffectParams, EncodeContext, GpuImage, IRect, RenderError};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

pub(crate) fn builtin() -> Builtin {
    let mut b = DefBuilder::new("ADBE Apply Color LUT2", "Apply Color LUT", "Utility");
    b.push(
        1,
        "<LUT not set>",
        "<未设置 LUT>",
        ParamKind::ArbitraryData,
        Some(Value::Bytes(Vec::new())),
    );
    b.push(2, "", "", ParamKind::Button, None);
    let (mut def, zh) = b.build();
    def.params[1].flags.hidden = true;
    for param in &mut def.params {
        param.flags.cannot_animate = true;
        param.flags.cannot_time_vary = true;
    }
    rust_builtin(
        (
            Arc::new(ApplyLut {
                def,
                cache: LutCache::default(),
                ocio: None,
            }),
            zh,
        ),
        "apply-color-lut",
        "应用颜色 LUT",
        &["颜色查找表", "cube", "spi1d", "spi3d"],
    )
}

#[derive(Debug)]
struct ApplyLut {
    def: EffectDef,
    cache: LutCache,
    ocio: Option<crate::categories::color::ocio::Operation>,
}

#[derive(Debug, Default)]
pub(super) struct LutCache(Mutex<VecDeque<Arc<Prepared>>>);

type Processor = (crate::categories::color::ocio::Operation, [i64; 9]);

pub(super) enum LutSource<'a> {
    File(&'a [u8]),
    Ocio(&'a [u8], Processor),
}

#[derive(Debug)]
struct Prepared {
    device: wgpu::Device,
    format: wgpu::TextureFormat,
    data: Vec<u8>,
    processor: Option<Processor>,
    kernel: Arc<Kernel>,
    values: Vec<u8>,
    views: Vec<wgpu::TextureView>,
}

impl Prepared {
    fn new(
        gpu: &Arc<GpuContext>,
        format: wgpu::TextureFormat,
        data: &[u8],
        processor: Option<Processor>,
        transform: bae_color::ColorTransform,
    ) -> Result<Self, RenderError> {
        let snippet = transform.to_wgsl("lut");
        let values = snippet.params_bytes();
        let key = KernelKey {
            owner: format!("bae.effects/lut/{}", snippet.key).into(),
            entry: "main".into(),
            variant: gpu::format_key(format),
        };
        let kernel = gpu::runtime(gpu).kernel(&key, || {
            let mut bindings = vec![
                (0, Binding::Uniform((64 + values.len()) as u32)),
                (1, Binding::Texture),
                (2, Binding::Storage(format)),
            ];
            let mut source = [
                image_grid::CHANNEL_MATH,
                include_str!("../shaders/image_grid.wgsl"),
                &snippet.wgsl,
            ]
            .concat();
            let mut names = Vec::new();
            for (i, t) in snippet.textures.iter().enumerate() {
                let n = i + 3;
                let (dim, binding) = match t.dimension {
                    LutDimension::D2 => ("2d", Binding::Texture),
                    LutDimension::D3 => ("3d", Binding::Texture3d),
                };
                bindings.push((n as u32, binding));
                source += &format!("@group(0) @binding({n}) var table{i}:texture_{dim}<f32>;\n");
                names.push(format!("table{i}"));
            }
            let names: Vec<_> = names.iter().map(String::as_str).collect();
            source += &include_str!("lut.wgsl")
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?)
                .replace(
                    "APPLY_LUT",
                    &snippet.call("c.rgb", "u.color", &names, false),
                );
            Ok(KernelSource {
                label: "Apply Color LUT",
                wgsl: source,
                entry: "main",
                bindings,
                workgroup: [8, 8, 1],
            })
        })?;
        let mut views = Vec::new();
        for tex in snippet.textures {
            let dimension = match tex.dimension {
                LutDimension::D2 => wgpu::TextureDimension::D2,
                LutDimension::D3 => wgpu::TextureDimension::D3,
            };
            let limit = if dimension == wgpu::TextureDimension::D3 {
                gpu.limits().max_texture_dimension_3d
            } else {
                gpu.limits().max_texture_dimension_2d
            };
            if tex.width.max(tex.height).max(tex.depth) > limit {
                return Err(RenderError::TooLarge {
                    width: tex.width,
                    height: tex.height,
                    limit,
                });
            }
            let texture = gpu.device().create_texture_with_data(
                gpu.queue(),
                &wgpu::TextureDescriptor {
                    label: Some("color LUT"),
                    size: wgpu::Extent3d {
                        width: tex.width,
                        height: tex.height,
                        depth_or_array_layers: tex.depth,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension,
                    format: wgpu::TextureFormat::Rgba32Float,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &tex.bytes(),
            );
            views.push(texture.create_view(&Default::default()));
        }
        Ok(Self {
            device: gpu.device().clone(),
            format,
            data: data.into(),
            processor,
            kernel,
            values,
            views,
        })
    }
}

pub(crate) fn ocio_effect(
    def: EffectDef,
    op: crate::categories::color::ocio::Operation,
) -> Arc<dyn TargetEffect> {
    Arc::new(ApplyLut {
        def,
        cache: LutCache::default(),
        ocio: Some(op),
    })
}

impl Effect for ApplyLut {
    fn def(&self) -> &EffectDef {
        &self.def
    }
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        self.encode(ctx, p, input, roi)
    }
}
impl TargetEffect for ApplyLut {
    fn encode(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
    ) -> Result<GpuImage, RenderError> {
        let id = self.ocio.map_or(1, |op| op.data_id());
        let data = match p.value(id) {
            Some(Value::Bytes(bytes)) => bytes.as_slice(),
            _ => &[],
        };
        let source = match self.ocio {
            Some(op) => LutSource::Ocio(data, (op, op.selection(&self.def, p))),
            None => LutSource::File(data),
        };
        self.cache.apply(target, p, input, roi, source, 1.)
    }
}

impl LutCache {
    pub(super) fn apply(
        &self,
        target: &mut dyn EffectTarget,
        p: &EffectParams<'_>,
        input: &GpuImage,
        roi: IRect,
        source: LutSource<'_>,
        intensity: f32,
    ) -> Result<GpuImage, RenderError> {
        let (data, processor) = match source {
            LutSource::File(data) => (data, None),
            LutSource::Ocio(data, processor) => (data, Some(processor)),
        };
        let needs_data = processor.is_none_or(|(op, _)| op.data_id() != 0);
        if (needs_data && data.is_empty()) || roi.is_empty() || intensity == 0. {
            return Ok(input.clone());
        }
        let lut = {
            let mut cache = self.0.lock();
            if let Some(entry) = cache.iter().find(|l| {
                &l.device == target.gpu().device()
                    && l.format == target.format()
                    && l.data == data
                    && l.processor == processor
            }) {
                entry.clone()
            } else {
                let transform = match processor {
                    Some((op, selection)) => op.transform(data, selection)?,
                    None => bae_color::lutfile::from_embedded(data)
                        .map_err(|e| RenderError::Other(e.to_string()))?,
                };
                if transform.is_identity() {
                    return Ok(input.clone());
                }
                let entry = Arc::new(Prepared::new(
                    target.gpu(),
                    target.format(),
                    data,
                    processor,
                    transform,
                )?);
                // Keep adjacent looks in a stack resident across frames.
                if cache.len() == 4 {
                    cache.pop_front();
                }
                cache.push_back(entry.clone());
                entry
            }
        };
        let grid = image_grid::grid_scale(p, input);
        let out = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
        let rt = gpu::runtime(target.gpu());
        let mut w = UniformWriter::with_size(64 + lut.values.len());
        image_grid::image_coords(&mut w, 0, input, grid);
        w.i32s(32, &[roi.x0, roi.y0])
            .u32(40, roi.width())
            .u32(44, roi.height())
            .u32(48, u32::from(p.float))
            .f32(52, intensity);
        w.bytes[64..].copy_from_slice(&lut.values);
        let mut resources = vec![
            Resource::Uniform,
            Resource::View(&input.view),
            Resource::View(&out.view),
        ];
        resources.extend(lut.views.iter().map(Resource::View));
        let group = rt.bind_group(&lut.kernel, &resources);
        let offset = gpu::push_uniform(&rt, &w);
        let mut pass = target.encoder().begin_compute_pass(&Default::default());
        gpu::dispatch(&mut pass, &lut.kernel, &group, &[offset], roi.size());
        drop(pass);
        Ok(out)
    }
}

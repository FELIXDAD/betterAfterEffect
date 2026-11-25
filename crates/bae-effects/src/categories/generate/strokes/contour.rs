//! Marching-squares topology and dash coverage stay on the device. Each
//! lattice edge has one vertex id shared by the two adjacent cells.
use crate::{
    EffectTarget,
    effects::image_grid,
    gpu::{self, Binding, Kernel, KernelKey, KernelSource, Resource, UniformWriter},
    params::Params,
};
use bae_gpu::wgpu;
use bae_render::{GpuImage, IRect, RenderError};
use std::sync::Arc;

const LABEL: &str = "bae.effects.vegas.trace";
const UNIFORM: u32 = 160;
const STAGES: [&str; 7] = [
    "initialize",
    "connect",
    "trace",
    "count_blocks",
    "rank_blocks",
    "raster",
    "composite",
];

fn kernel(
    rt: &gpu::Runtime,
    stage: &'static str,
    format: wgpu::TextureFormat,
) -> Result<Arc<Kernel>, RenderError> {
    let composite = stage == "composite";
    // Only the final pass writes the working texture.
    let format = if composite {
        format
    } else {
        wgpu::TextureFormat::Rgba16Float
    };
    rt.kernel(
        &KernelKey {
            owner: LABEL.into(),
            entry: stage.into(),
            variant: gpu::format_key(format),
        },
        || {
            let mut bindings = vec![(0, Binding::Uniform(UNIFORM))];
            if composite {
                bindings.extend([
                    (7, Binding::ReadWriteBuffer),
                    (8, Binding::Texture),
                    (9, Binding::Storage(format)),
                ]);
            } else {
                bindings.push((1, Binding::Texture));
                let last = if stage == "raster" { 7 } else { 6 };
                bindings.extend((2..=last).map(|slot| (slot, Binding::ReadWriteBuffer)));
            }
            let workgroup = if matches!(stage, "count_blocks" | "rank_blocks") {
                [64, 1, 1]
            } else {
                [8, 8, 1]
            };
            Ok(KernelSource {
                label: LABEL,
                entry: stage,
                bindings,
                workgroup,
                wgsl: [
                    image_grid::CHANNEL_MATH,
                    include_str!("../../../shaders/image_grid.wgsl"),
                    include_str!("dash.wgsl"),
                    include_str!("contour.wgsl"),
                ]
                .concat()
                .replace("WORKING_FORMAT", crate::blur::format_name(format)?),
            })
        },
    )
}
pub(super) fn prewarm(rt: &gpu::Runtime, format: wgpu::TextureFormat) -> Result<(), RenderError> {
    for stage in STAGES {
        kernel(rt, stage, format)?;
    }
    Ok(())
}

// Preserve pixel resolution up to the device's storage binding limit. On
// larger images the threshold field uses the finest grid that fits.
pub(super) fn grid_size([w, h]: [u32; 2], limit: u64) -> [u32; 2] {
    let scale = ((limit as f64) / (8. * f64::from(w) * f64::from(h)))
        .sqrt()
        .min(1.);
    [
        (f64::from(w) * scale).floor() as u32,
        (f64::from(h) * scale).floor() as u32,
    ]
}
pub(super) fn render(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
    roi: IRect,
    rect: IRect,
    field: &wgpu::Texture,
) -> Result<GpuImage, RenderError> {
    let rt = gpu::runtime(target.gpu());
    let grid = image_grid::grid_scale(v.p, input);
    let size = [field.width(), field.height()];
    let words = 2 * u64::from(size[0]) * u64::from(size[1]);
    let blocks = words.div_ceil(256) as u32;
    let allocate = |label, words: u64| {
        rt.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: 4 * words,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    };
    let parent = allocate("Vegas component roots", words);
    let links = allocate("Vegas path links", words);
    let arcs = allocate("Vegas arc lengths", words);
    let lengths = allocate("Vegas contour lengths", words);
    let ranks = allocate(
        "Vegas contour counts",
        1 + u64::from(blocks + blocks.div_ceil(256)),
    );
    let output = target.allocate(image_grid::pixel_rect(roi, grid), grid)?;
    let field_view = field.create_view(&Default::default());
    let format = target.format();
    let mut u = UniformWriter::with_size(UNIFORM as usize);
    image_grid::image_coords(&mut u, 0, input, grid);
    let mid = v.f(40).clamp(0.001, 0.999);
    let a = v.f(36);
    let b = v.f(42);
    let gradient = [a, (a + (b - a) * mid + v.f(38)).clamp(0., 1.), b, mid].map(|x| x as f32);
    let flags = u32::from(v.popup(26) == 1)
        | (u32::from(v.popup(48) == 2) << 1)
        | (u32::from(v.b(32)) << 2)
        | (u32::from(v.p.float) << 3);
    u.i32s(48, &[rect.x0, rect.y0])
        .u32(56, size[0])
        .u32(60, size[1])
        .f32s(
            64,
            &[
                rect.width() as f32 / size[0] as f32,
                rect.height() as f32 / size[1] as f32,
            ],
        )
        .f32(72, (v.f(20).clamp(0.5, 100.) / grid) as f32)
        .f32(76, v.f(22) as f32)
        .f32s(80, &gradient)
        .f32s(96, &super::color(v, 18))
        .f32(112, (v.f(30) / 360.) as f32)
        .f32(116, v.f(24).clamp(0., 1.) as f32)
        .f32(120, (v.f(16).clamp(0., 5.) / grid) as f32)
        .u32(124, v.f(28).round().clamp(1., 250.) as u32)
        .u32(128, super::vegas::style(v))
        .u32(132, flags)
        .u32(136, if v.popup(44) == 2 { v.f(46) as u32 } else { 0 })
        .u32(140, v.f(34) as u32)
        .u32(144, blocks)
        .i32s(152, &[roi.x0, roi.y0]);
    let field_resources = [
        Resource::Uniform,
        Resource::View(&field_view),
        Resource::Buffer(&parent),
        Resource::Buffer(&links),
        Resource::Buffer(&arcs),
        Resource::Buffer(&lengths),
        Resource::Buffer(&ranks),
    ];
    let record = |target: &mut dyn EffectTarget,
                  name,
                  resources: &[Resource<'_>],
                  offset,
                  size|
     -> Result<(), RenderError> {
        let pipeline = kernel(&rt, name, format)?;
        let group = rt.bind_group(&pipeline, resources);
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(name),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &pipeline, &group, &[offset], size);
        Ok(())
    };
    let offset = gpu::push_uniform(&rt, &u);
    for stage in [
        "initialize",
        "connect",
        "trace",
        "count_blocks",
        "rank_blocks",
    ] {
        let extent = match stage {
            "count_blocks" => [blocks.div_ceil(256), 1],
            "rank_blocks" => [blocks, 1],
            _ => size,
        };
        record(target, stage, &field_resources, offset, extent)?;
    }
    let max_rows = (rt.device.limits().max_storage_buffer_binding_size
        / (4 * u64::from(roi.width())))
    .min(u64::from(roi.height())) as i32;
    let mut y = roi.y0;
    while y < roi.y1 {
        let stripe = IRect::new(roi.x0, y, roi.x1, (y + max_rows).min(roi.y1));
        let coverage = allocate("Vegas stroke coverage", stripe.area());
        u.i32s(32, &image_grid::rect4(stripe));
        let offset = gpu::push_uniform(&rt, &u);
        let mut raster_resources = field_resources.to_vec();
        raster_resources.push(Resource::Buffer(&coverage));
        record(target, "raster", &raster_resources, offset, size)?;
        record(
            target,
            "composite",
            &[
                Resource::Uniform,
                Resource::Buffer(&coverage),
                Resource::View(&input.view),
                Resource::View(&output.view),
            ],
            offset,
            stripe.size(),
        )?;
        y = stripe.y1;
    }
    Ok(output)
}

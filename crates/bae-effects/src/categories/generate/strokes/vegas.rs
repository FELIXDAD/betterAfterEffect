//! AE Vegas: dashes on a mask or on traced isocontours of a selected channel.
use std::sync::Arc;

use bae_gpu::wgpu;
use bae_model::{ParamKind, Value};
use bae_render::{GpuImage, IRect, RenderError};

use super::paint::{Mark, Paint};
use super::{Generator, Path, color, percent, slider};
use crate::{
    Builtin, EffectTarget,
    blur::{self, Edge},
    catalog::DefBuilder,
    effects::{image_grid, layer_input},
    gpu::{self, Kernel, Resource, UniformWriter},
    params::Params,
};

const LABEL: &str = "bae.effects.vegas.contour";
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
const UNIFORM_SIZE: u32 = 80;

pub(crate) fn vegas() -> Builtin {
    let mut b = DefBuilder::new("APC Vegas", "Vegas", "Generate").popup(
        52,
        "Stroke",
        "描边",
        1,
        &["Image Contours", "Mask/Path"],
    );
    b.push(
        54,
        "Image Contours",
        "图像等高线",
        ParamKind::GroupStart,
        None,
    );
    b = b
        .layer(2, "Input Layer", "输入图层")
        .layer_stage(2)
        .checkbox(4, "Invert Input", "反转输入", false)
        .popup(
            6,
            "If Layer Sizes Differ",
            "如果图层大小不同",
            1,
            &["Center", "Stretch to Fit"],
        )
        .popup(
            10,
            "Channel",
            "通道",
            1,
            &[
                "Intensity",
                "Red",
                "Green",
                "Blue",
                "Alpha",
                "Hue",
                "Lightness",
                "Saturation",
                "Value",
            ],
        )
        .int(12, "Threshold", "阈值", 127, [0, 255]);
    b = slider(b, 14, "Pre-Blur", "预模糊", 0., [0., 50.], "pixels");
    b = slider(b, 16, "Tolerance", "容差", 0.5, [0., 5.], "pixels")
        .popup(
            44,
            "Render",
            "渲染",
            1,
            &["All Contours", "Selected Contour"],
        )
        .int(46, "Selected Contour", "选定等高线", 1, [1, 1000])
        .popup(
            48,
            "Shorter Contours Have",
            "设置较短的等高线",
            1,
            &["Same Number of Segments", "Fewer Segments"],
        );
    b.push(27, "", "", ParamKind::GroupEnd, None);
    b.push(56, "Mask/Path", "蒙版/路径", ParamKind::GroupStart, None);
    b.push(50, "Path", "路径", ParamKind::Path, None);
    b = b.static_param();
    b.push(33, "", "", ParamKind::GroupEnd, None);
    b.push(35, "Segments", "片段", ParamKind::GroupStart, None);
    b = b.int(28, "Segments", "片段", 32, [1, 250]);
    b = percent(b, 24, "Length", "长度", 1., 1.).popup(
        26,
        "Segment Distribution",
        "片段分布",
        2,
        &["Bunched", "Even"],
    );
    b.push(
        30,
        "Rotation",
        "旋转",
        ParamKind::Angle,
        Some(Value::Float(0.)),
    );
    b = b.checkbox(32, "Random Phase", "随机相位", false).int(
        34,
        "Random Seed",
        "随机植入",
        1,
        [1, 1000],
    );
    b.push(49, "", "", ParamKind::GroupEnd, None);
    b.push(51, "Rendering", "正在渲染", ParamKind::GroupStart, None);
    b = b.popup(
        8,
        "Blend Mode",
        "混合模式",
        2,
        &["Transparent", "Over", "Under", "Stencil"],
    );
    b.push(
        18,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([1., 1., 0., 1.])),
    );
    b = slider(b, 20, "Width", "宽度", 2., [0.5, 100.], "pixels");
    b = percent(b, 22, "Hardness", "硬度", 0., 1.);
    b = percent(b, 36, "Start Opacity", "起始点不透明度", 1., 1.);
    b = slider(
        b,
        38,
        "Mid-point Opacity",
        "中点不透明度",
        0.,
        [-1., 1.],
        "percent",
    );
    b = slider(
        b,
        40,
        "Mid-point Position",
        "中点位置",
        0.5,
        [65. / 65536., 65470. / 65536.],
        "percent",
    );
    b = percent(b, 42, "End Opacity", "结束点不透明度", 0., 1.);
    b.push(69, "", "", ParamKind::GroupEnd, None);
    Generator::Vegas.builtin(b, "vegas", "勾画")
}

pub(super) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        52 => Some(&["图像等高线", "蒙版/路径"]),
        6 => Some(&["中心", "伸缩以适合"]),
        10 => Some(&[
            "强度",
            "红色",
            "绿色",
            "蓝色",
            "Alpha",
            "色相",
            "亮度",
            "饱和度",
            "值",
        ]),
        44 => Some(&["所有等高线", "选定等高线"]),
        48 => Some(&["相同数目片段", "少数片段"]),
        26 => Some(&["成簇分布", "均匀分布"]),
        8 => Some(&["透明", "超过", "曝光不足", "模板"]),
        _ => None,
    }
}

pub(super) fn style(v: Params<'_>) -> u32 {
    match v.popup(8) {
        1 => 2,
        3 => 4,
        4 => 3,
        _ => 1,
    }
}

pub(super) fn paint(v: Params<'_>) -> Paint {
    let mut paint = Paint::new(style(v), 3, 1.);
    let length = v.f(24).clamp(0., 1.);
    if length == 0. {
        return paint;
    }
    let paths: Vec<_> =
        v.p.mask_paths
            .iter()
            .map(|mask| {
                let shape = bae_render::mask::to_bez_path(&mask.shape);
                let elements = shape.elements();
                // Vegas omits the terminal segment of an open mask, as measured
                // with an open three-vertex path and a separate two-vertex path.
                let count = if mask.shape.closed {
                    elements.len()
                } else {
                    elements.len().saturating_sub(1)
                };
                Path::from_elements(&elements[..count])
            })
            .collect();
    let segments = v.f(28).round().clamp(1., 250.);
    let angle = v.f(30) / 360.;
    let midpoint = v.f(40).clamp(0.001, 0.999);
    let first_alpha = v.f(36);
    let last_alpha = v.f(42);
    let mid_alpha = (first_alpha + (last_alpha - first_alpha) * midpoint + v.f(38)).clamp(0., 1.);
    paint.gradient = [
        first_alpha as f32,
        mid_alpha as f32,
        last_alpha as f32,
        midpoint as f32,
    ];
    for (i, path) in paths.iter().enumerate() {
        let len = path.length();
        if len == 0. {
            continue;
        }
        let count = segments;
        let period = len / count;
        let active = period * length;
        let phase_shift = if v.b(32) {
            let n = (v.f(34) as u32)
                .wrapping_add(i as u32)
                .wrapping_mul(0x9e37_79b9);
            f64::from(n.rotate_left(13)) / f64::from(u32::MAX)
        } else {
            0.
        };
        let spacing = if v.popup(26) == 1 { active } else { period };
        let phase = ((angle + phase_shift) * len).rem_euclid(len);
        for j in 0..count as i32 {
            let distance = (f64::from(j) * spacing + phase).rem_euclid(len);
            for start in [distance - len, distance] {
                let from = start.max(0.);
                let to = (start + active).min(len);
                if from >= to {
                    continue;
                }
                for (p, q, l, r) in path.segments(from, to) {
                    paint.marks.push(Mark {
                        start: p + 0.5,
                        end: q + 0.5,
                        radius: v.f(20).clamp(0.5, 100.),
                        rgb: color(v, 18),
                        hardness: v.f(22).clamp(0., 1.) as f32,
                        alpha: [((l - start) / active) as f32, ((r - start) / active) as f32],
                        dash_length: active as f32,
                    });
                }
            }
        }
    }
    paint
}

pub(super) fn contour_kernel(rt: &gpu::Runtime) -> Result<Arc<Kernel>, RenderError> {
    image_grid::kernel_with_inputs(
        rt,
        FORMAT,
        LABEL,
        include_str!("vegas.wgsl"),
        UNIFORM_SIZE,
        1,
    )
}

pub(super) fn render_contours(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
    roi: IRect,
) -> Result<GpuImage, RenderError> {
    let mut source = if let Some(dep) = layer_input::dependency(v.p, 2, false) {
        let Some(map) = target.effect_layer(&dep) else {
            return Paint::new(style(v), 3, 1.).render(target, v.p, input, roi);
        };
        map.clone()
    } else {
        input.clone()
    };
    let grid = image_grid::grid_scale(v.p, input);
    let rect = if v.p.layer_bounds.is_empty() {
        image_grid::pixel_rect(input.rect, 1. / grid)
    } else {
        image_grid::layer_rect(v.p)
    };
    let [w, h] = super::contour::grid_size(
        rect.size(),
        target
            .gpu()
            .device()
            .limits()
            .max_storage_buffer_binding_size,
    );
    if w < 2 || h < 2 || v.f(24) <= 0. {
        return Paint::new(style(v), 3, 1.).render(target, v.p, input, roi);
    }
    let sigma = v.f(14).clamp(0., 50.) * 0.4 / source.pixel_scale;
    if blur::radius(sigma) != 0 {
        source = blur::plan_gaussian(source.rect, [sigma; 2], Edge::Repeat, source.rect)
            .encode(target, &source)?;
    }
    let rt = gpu::runtime(target.gpu());
    let pipe = contour_kernel(&rt)?;
    let texture = rt.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(LABEL),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mut uniforms = UniformWriter::with_size(UNIFORM_SIZE as usize);
    image_grid::image_coords(&mut uniforms, 0, &source, grid);
    uniforms
        .i32s(32, &image_grid::rect4(rect))
        .u32(48, v.popup(10) as u32)
        .u32(52, u32::from(v.b(4)))
        .u32(56, u32::from(v.popup(6) == 2))
        .f32(64, (v.f(12).clamp(0., 255.) / 255.) as f32)
        .u32(72, w)
        .u32(76, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let group = rt.bind_group(
        &pipe,
        &[
            Resource::Uniform,
            Resource::View(&source.view),
            Resource::View(&view),
        ],
    );
    let offset = gpu::push_uniform(&rt, &uniforms);
    {
        let mut pass = target
            .encoder()
            .begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(LABEL),
                timestamp_writes: None,
            });
        gpu::dispatch(&mut pass, &pipe, &group, &[offset], [w, h]);
    }
    super::contour::render(target, v, input, roi, rect, &texture)
}

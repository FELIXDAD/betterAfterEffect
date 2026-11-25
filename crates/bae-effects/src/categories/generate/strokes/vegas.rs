//! AE Vegas: dashes on a mask or on traced isocontours of a selected channel.
use std::sync::Arc;

use bae_gpu::{
    transfer::{TextureReadback, TextureRegion},
    wgpu,
};
use bae_model::{ParamKind, Value};
use bae_render::{GpuImage, IRect, RenderError};
use glam::{DVec2, IVec2};

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

pub(super) fn paint(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
) -> Result<Paint, RenderError> {
    let style = match v.popup(8) {
        1 => 2,
        3 => 4,
        4 => 3,
        _ => 1,
    };
    let mut paint = Paint::new(style, 3, 1.);
    let length = v.f(24).clamp(0., 1.);
    if length == 0. {
        return Ok(paint);
    }
    let image_mode = v.popup(52) == 1;
    let origin_offset = if image_mode { 0. } else { 0.5 };
    let paths = if image_mode {
        contours(target, v, input)?
    } else {
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
            .collect()
    };
    let segments = v.f(28).round().clamp(1., 250.);
    let longest = paths.iter().map(Path::length).fold(0., f64::max);
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
        if image_mode && v.popup(44) == 2 && i + 1 != v.f(46) as usize {
            continue;
        }
        let len = path.length();
        if len == 0. {
            continue;
        }
        let count = if image_mode && v.popup(48) == 2 {
            (segments * len / longest).round().max(1.)
        } else {
            segments
        };
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
                        start: p + origin_offset,
                        end: q + origin_offset,
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
    Ok(paint)
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

fn contours(
    target: &mut dyn EffectTarget,
    v: Params<'_>,
    input: &GpuImage,
) -> Result<Vec<Path>, RenderError> {
    let mut source = if let Some(dep) = layer_input::dependency(v.p, 2, false) {
        let Some(map) = target.effect_layer(&dep) else {
            return Ok(Vec::new());
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
    let [w, h] = rect.size();
    if w < 2 || h < 2 {
        return Ok(Vec::new());
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
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut uniforms = UniformWriter::with_size(UNIFORM_SIZE as usize);
    image_grid::image_coords(&mut uniforms, 0, &source, grid);
    uniforms
        .i32s(32, &image_grid::rect4(rect))
        .u32(48, v.popup(10) as u32)
        .u32(52, u32::from(v.b(4)))
        .u32(56, u32::from(v.popup(6) == 2))
        .f32(64, (v.f(12).clamp(0., 255.) / 255.) as f32);
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
    let gpu = target.gpu().clone();
    let copy = TextureReadback::record(
        &gpu,
        target.encoder(),
        &texture,
        TextureRegion::full(&texture),
    )?;
    // The connected contours need a global traversal before they can be
    // measured and dashed. Synchronize only the single-channel threshold field.
    let encoder = std::mem::replace(
        target.encoder(),
        rt.device.create_command_encoder(&Default::default()),
    );
    rt.queue.submit([encoder.finish()]);
    let data = copy.map().wait(&gpu)?;
    let mut trace = Trace {
        field: data
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&x| f32::from_le_bytes(x))
            .collect(),
        visited: vec![0; ((w - 1) * (h - 1)) as usize],
        size: IVec2::new(w as i32 - 1, h as i32 - 1),
        rect,
        grid,
    };
    Ok(trace.paths(v.f(16).clamp(0., 5.)))
}

struct Trace {
    field: Vec<f32>,
    visited: Vec<u8>,
    size: IVec2,
    rect: IRect,
    grid: f64,
}

// Edges run clockwise, beginning at the top. A cell can contain two arcs.
const CORNERS: [IVec2; 4] = [IVec2::ZERO, IVec2::X, IVec2::ONE, IVec2::Y];
const NEIGHBORS: [IVec2; 4] = [IVec2::NEG_Y, IVec2::X, IVec2::Y, IVec2::NEG_X];

impl Trace {
    fn inside(&self, p: IVec2) -> bool {
        p.cmpge(IVec2::ZERO).all() && p.cmplt(self.size).all()
    }

    fn values(&self, p: IVec2) -> [f32; 4] {
        CORNERS.map(|corner| {
            let q = p + corner;
            self.field[(q.y * (self.size.x + 1) + q.x) as usize]
        })
    }

    fn pair(v: [f32; 4], entering: usize) -> Option<usize> {
        let above = v.map(|n| n >= 0.);
        let crosses = |i: usize| above[i] != above[(i + 1) % 4];
        if !crosses(entering) {
            return None;
        }
        if crosses((entering + 2) % 4) && crosses((entering + 1) % 4) {
            let center = v.iter().sum::<f32>() >= 0.;
            Some(
                (entering
                    + if above[(entering + 1) % 4] == center {
                        3
                    } else {
                        1
                    })
                    % 4,
            )
        } else {
            (1..4).map(|i| (entering + i) % 4).find(|&i| crosses(i))
        }
    }

    fn point(&self, p: IVec2, v: [f32; 4], edge: usize) -> DVec2 {
        let end = (edge + 1) % 4;
        let t = f64::from(v[edge] / (v[edge] - v[end]));
        let xy = (p + CORNERS[edge])
            .as_dvec2()
            .lerp((p + CORNERS[end]).as_dvec2(), t);
        (xy + DVec2::new(f64::from(self.rect.x0) + 0.5, f64::from(self.rect.y0) + 0.5)) * self.grid
    }

    fn follow(&mut self, mut p: IVec2, mut edge: usize) -> Vec<DVec2> {
        let mut path = Vec::new();
        while self.inside(p) {
            let visit = (p.y * self.size.x + p.x) as usize;
            if self.visited[visit] & (1 << edge) != 0 {
                break;
            }
            let v = self.values(p);
            let Some(next) = Self::pair(v, edge) else {
                break;
            };
            self.visited[visit] |= (1 << edge) | (1 << next);
            if path.is_empty() {
                path.push(self.point(p, v, edge));
            }
            path.push(self.point(p, v, next));
            p += NEIGHBORS[next];
            edge = (next + 2) % 4;
        }
        path
    }

    fn paths(&mut self, tolerance: f64) -> Vec<Path> {
        let mut paths = Vec::new();
        // Start open paths at the frame edge so both halves of every interior
        // connected component remain one contour.
        for x in 0..self.size.x {
            paths.push(self.follow(IVec2::new(x, 0), 0));
            paths.push(self.follow(IVec2::new(x, self.size.y - 1), 2));
        }
        for y in 0..self.size.y {
            paths.push(self.follow(IVec2::new(0, y), 3));
            paths.push(self.follow(IVec2::new(self.size.x - 1, y), 1));
        }
        for y in 0..self.size.y {
            for x in 0..self.size.x {
                let p = IVec2::new(x, y);
                let v = self.values(p);
                if v.iter().all(|n| (*n >= 0.) == (v[0] >= 0.)) {
                    continue;
                }
                for edge in 0..4 {
                    let path = self.follow(p, edge);
                    if !path.is_empty() {
                        paths.push(path);
                    }
                }
            }
        }
        let mut paths: Vec<_> = paths
            .into_iter()
            .filter(|p| p.len() >= 2)
            .map(|mut points| {
                if points[0].distance_squared(*points.last().unwrap()) < 1e-6 {
                    points.pop();
                    let top = points
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x))
                        })
                        .unwrap()
                        .0;
                    points.rotate_left(top);
                    points.push(points[0]);
                    if points.windows(2).map(|p| p[0].perp_dot(p[1])).sum::<f64>() < 0. {
                        points.reverse();
                    }
                }
                Path::new(simplify(points, tolerance))
            })
            .collect();
        let top = |path: &Path| {
            path.points
                .iter()
                .copied()
                .min_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)))
                .unwrap()
        };
        paths.sort_by(|a, b| {
            let (a, b) = (top(a), top(b));
            a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x))
        });
        paths
    }
}

// Ramer–Douglas–Peucker keeps the tolerance against the original samples.
fn simplify(points: Vec<DVec2>, tolerance: f64) -> Vec<DVec2> {
    let mut keep = vec![false; points.len()];
    let mut spans = vec![(0, points.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        keep[first] = true;
        keep[last] = true;
        let a = points[first];
        let direction = points[last] - a;
        let length_squared = direction.length_squared().max(f64::EPSILON);
        let mut furthest = None;
        let mut limit = tolerance * tolerance;
        for (i, p) in points.iter().enumerate().take(last).skip(first + 1) {
            let from = *p - a;
            let t = (from.dot(direction) / length_squared).clamp(0., 1.);
            let error = (from - t * direction).length_squared();
            if error > limit {
                limit = error;
                furthest = Some(i);
            }
        }
        if let Some(i) = furthest {
            spans.extend([(first, i), (i, last)]);
        }
    }
    points
        .into_iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(p))
        .collect()
}

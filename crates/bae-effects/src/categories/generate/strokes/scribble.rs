//! AE Scribble: clips alternating hatch strokes to the selected mask region.
use bae_model::{ParamKind, Time, Value};
use bae_render::{blend::MaskMode, input::RenderMask};
use glam::DVec2;
use kurbo::{BezPath, Cap, Join, PathEl, Stroke, StrokeOpts};

use super::paint::{Mark, Paint};
use super::{Generator, Path, color, percent, ranges, slider, style, trim};
use crate::{Builtin, catalog::DefBuilder, params::Params};

pub(crate) fn scribble() -> Builtin {
    let mut b = DefBuilder::new("ADBE Scribble Fill", "Scribble", "Generate")
        .popup(
            64,
            "Scribble",
            "涂抹",
            3,
            &[
                "None",
                "(-",
                "Single Mask",
                "All Masks",
                "All Masks Using Modes",
            ],
        )
        .static_param();
    b.push(2, "Mask", "蒙版", ParamKind::Path, None);
    b = b.static_param().popup(
        50,
        "Fill Type",
        "填充类型",
        1,
        &[
            "Inside",
            "Centered Edge",
            "Inside Edge",
            "Outside Edge",
            "Left Edge",
            "Right Edge",
        ],
    );
    b.push(9, "Edge Options", "边缘选项", ParamKind::GroupStart, None);
    b = slider(b, 52, "Edge Width", "边缘宽度", 20., [1., 1000.], "pixels")
        .popup(
            56,
            "End Cap",
            "末端端点",
            1,
            &["Round", "Butt", "Projecting"],
        )
        .popup(54, "Join", "连接", 1, &["Round", "Bevel", "Miter"]);
    b = slider(b, 58, "Miter Limit", "尖角限制", 4., [1., 500.], "none").popup(
        68,
        "Start/End Apply To",
        "开始/末端应用于",
        2,
        &["Mask Path", "Scribble Result"],
    );
    b.push(21, "", "", ParamKind::GroupEnd, None);
    b.push(
        6,
        "Color",
        "颜色",
        ParamKind::Color,
        Some(Value::Color([1.; 4])),
    );
    b = percent(b, 24, "Opacity", "不透明度", 1., 1.);
    b.push(
        10,
        "Angle",
        "角度",
        ParamKind::Angle,
        Some(Value::Float(60.)),
    );
    b = slider(
        b,
        8,
        "Stroke Width",
        "描边宽度",
        2.,
        [6553. / 65536., 50.],
        "pixels",
    );
    b.push(
        31,
        "Stroke Options",
        "描边选项",
        ParamKind::GroupStart,
        None,
    );
    b = percent(b, 12, "Curviness", "曲度", 3276. / 65536., 1.);
    b = percent(b, 66, "Curviness Variation", "曲度变化", 655. / 65536., 1.);
    b = slider(
        b,
        60,
        "Spacing",
        "间距",
        5.,
        [6553. / 65536., 200.],
        "pixels",
    );
    b = slider(
        b,
        62,
        "Spacing Variation",
        "间距变化",
        1.,
        [0., 200.],
        "pixels",
    );
    b = slider(
        b,
        38,
        "Path Overlap",
        "路径重叠",
        0.,
        [-1000., 1000.],
        "pixels",
    );
    b = slider(
        b,
        40,
        "Path Overlap Variation",
        "路径重叠变化",
        5.,
        [0., 1000.],
        "pixels",
    );
    b.push(45, "", "", ParamKind::GroupEnd, None);
    b = slider(b, 30, "Start", "起始", 0., [0., 100.], "none");
    b = slider(b, 32, "End", "结束", 100., [0., 100.], "none");
    b = b
        .checkbox(28, "Fill Paths Sequentially", "顺序填充路径", true)
        .popup(
            48,
            "Wiggle Type",
            "摆动类型",
            3,
            &["Static", "Jumpy", "Smooth"],
        );
    b = slider(b, 44, "Wiggles/Second", "摇摆/秒", 10., [0., 30.], "none").int(
        46,
        "Random Seed",
        "随机植入",
        1,
        [1, 1000],
    );
    Generator::Scribble.builtin(style(b, 26, "Composite", "合成", 2), "scribble", "涂写")
}

pub(super) fn options_zh(id: u32) -> Option<&'static [&'static str]> {
    match id {
        64 => Some(&["无", "(-", "单个蒙版", "所有蒙版", "所有蒙版使用模式"]),
        50 => Some(&["内部", "中心边缘", "在边缘内", "外面边缘", "左边", "右边"]),
        56 => Some(&["圆角", "平头", "投影"]),
        54 => Some(&["圆角", "斜角", "尖角"]),
        68 => Some(&["蒙版路径", "涂写结果"]),
        48 => Some(&["静态", "跳跃性", "平滑"]),
        _ => None,
    }
}

struct Region {
    outline: Vec<Vec<DVec2>>,
    clip: Vec<DVec2>,
    retain_inside: bool,
    mode: MaskMode,
    inverted: bool,
}

impl Region {
    fn new(v: Params<'_>, mask: &RenderMask, path: &Path, from: f64, to: f64) -> Self {
        let fill = v.popup(50);
        let closed = mask.shape.closed;
        let uses_modes = v.popup(64) == 5;
        let mut region = Self {
            outline: Vec::new(),
            clip: Vec::new(),
            retain_inside: true,
            mode: if uses_modes { mask.mode } else { MaskMode::Add },
            inverted: uses_modes && mask.inverted,
        };
        if fill == 1 {
            if closed {
                region.outline.push(path.points.clone());
            }
            return region;
        }
        let mut bez = BezPath::new();
        if from == 0. && to == path.length() {
            bez = bae_render::mask::to_bez_path(&mask.shape);
        } else {
            let start = path.at(from);
            bez.move_to((start.x, start.y));
            for (_, end, _, _) in path.segments(from, to) {
                bez.line_to((end.x, end.y));
            }
        }
        let width = v.f(52).max(1.);
        let stroke = Stroke::new(width * if fill == 2 { 1. } else { 2. })
            .with_caps(match v.popup(56) {
                2 => Cap::Butt,
                3 => Cap::Square,
                _ => Cap::Round,
            })
            .with_join(match v.popup(54) {
                2 => Join::Bevel,
                3 => Join::Miter,
                _ => Join::Round,
            })
            .with_miter_limit(v.f(58).max(1.));
        let outline = kurbo::stroke(bez.iter(), &stroke, &StrokeOpts::default(), 0.1);
        kurbo::flatten(outline.iter(), 0.1, |item| match item {
            PathEl::MoveTo(p) => region.outline.push(vec![DVec2::new(p.x, p.y)]),
            PathEl::LineTo(p) => region
                .outline
                .last_mut()
                .unwrap()
                .push(DVec2::new(p.x, p.y)),
            PathEl::ClosePath => {
                let loop_ = region.outline.last_mut().unwrap();
                loop_.push(loop_[0]);
            }
            _ => unreachable!(),
        });
        if fill > 2 {
            let clockwise = path
                .points
                .windows(2)
                .map(|p| p[0].perp_dot(p[1]))
                .sum::<f64>()
                > 0.;
            region.retain_inside = match fill {
                3 => true,
                4 => false,
                5 => !clockwise,
                _ => clockwise,
            };
            region.clip = path.points.clone();
            if !closed && fill >= 5 {
                let first = path.points[0];
                let last = *path.points.last().unwrap();
                let perpendicular = |d: DVec2| DVec2::new(d.y, -d.x).normalize() * width * 4.;
                region.retain_inside = fill == 5;
                region.clip.extend([
                    last + perpendicular(last - path.points[path.points.len() - 2]),
                    first + perpendicular(path.points[1] - first),
                    first,
                ]);
            }
        }
        region
    }
}

// Deterministic variation indexed by row, so smooth wiggle mixes identical
// row identities in consecutive random states even if the spacing changes.
fn random(mut n: u32) -> f64 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    2. * f64::from(n) / f64::from(u32::MAX) - 1.
}

fn variation(v: Params<'_>, time: Time, row: u32, stream: u32) -> f64 {
    let phase = if v.popup(48) == 1 {
        0.
    } else {
        time.as_seconds() * v.f(44)
    };
    let phase_id = phase.floor() as i64 as u32;
    let key = (v.f(46) as u32).wrapping_mul(9719) ^ row.wrapping_mul(2897) ^ stream;
    let first = random(key ^ phase_id.wrapping_mul(10651));
    if v.popup(48) != 3 {
        return first;
    }
    let f = phase - phase.floor();
    let next = random(key ^ phase_id.wrapping_add(1).wrapping_mul(10651));
    first + (next - first) * (f * f * (3. - 2. * f))
}

fn crossings(
    points: &[DVec2],
    y: f64,
    region: usize,
    clip: usize,
    events: &mut Vec<(f64, usize, usize)>,
) {
    for pair in points.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        if (a.y > y) != (b.y > y) {
            events.push((a.x + (b.x - a.x) * ((y - a.y) / (b.y - a.y)), region, clip));
        }
    }
}

fn inside(regions: &[Region], state: &[[bool; 2]]) -> bool {
    let mut result = None;
    for (region, [outer, inner]) in regions.iter().zip(state.iter().copied()) {
        let area = outer && (region.clip.is_empty() || inner == region.retain_inside);
        let area = area != region.inverted;
        result = Some(match region.mode {
            MaskMode::None => continue,
            MaskMode::Add | MaskMode::Lighten => result.unwrap_or(false) || area,
            MaskMode::Subtract => result.unwrap_or(true) && !area,
            MaskMode::Intersect | MaskMode::Darken => result.unwrap_or(true) && area,
            MaskMode::Difference => result.unwrap_or(false) != area,
        });
    }
    result.unwrap_or(false)
}

fn hatch(v: Params<'_>, bounds: bae_render::IRect, mut regions: Vec<Region>) -> Vec<Path> {
    if inside(&regions, &vec![[false; 2]; regions.len()]) {
        let corners = bounds.to_rect().corners();
        let mut domain = corners.to_vec();
        domain.push(corners[0]);
        regions.push(Region {
            outline: vec![domain],
            clip: Vec::new(),
            retain_inside: true,
            mode: MaskMode::Intersect,
            inverted: false,
        });
    }
    let (sin, cos) = v.f(10).to_radians().sin_cos();
    let snap = |n: f64| if n.abs() < 1e-12 { 0. } else { n };
    let direction = DVec2::new(-snap(sin), snap(cos));
    let normal = DVec2::new(direction.y, -direction.x);
    let to_hatch = |p: DVec2| DVec2::new(p.dot(direction), p.dot(normal));
    let from_hatch = |p: DVec2| direction * p.x + normal * p.y + 0.5;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for region in &mut regions {
        for p in region.outline.iter_mut().flatten().chain(&mut region.clip) {
            *p = to_hatch(*p);
            lo = lo.min(p.y);
            hi = hi.max(p.y);
        }
    }
    let spacing = v.f(60).max(0.1);
    let mut y = lo;
    let mut row = 0u32;
    let mut lines: Vec<Vec<DVec2>> = Vec::new();
    let mut previous: Vec<([f64; 2], usize)> = Vec::new();
    let mut next = Vec::new();
    let mut events = Vec::new();
    let mut state = vec![[false; 2]; regions.len()];
    while y < hi {
        events.clear();
        state.fill([false; 2]);
        for (i, r) in regions.iter().enumerate() {
            for contour in &r.outline {
                crossings(contour, y, i, 0, &mut events);
            }
            crossings(&r.clip, y, i, 1, &mut events);
        }
        events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut edges = Vec::new();
        let mut was_inside = inside(&regions, &state);
        for &(x, region, clip) in &events {
            state[region][clip] = !state[region][clip];
            let now_inside = inside(&regions, &state);
            if now_inside != was_inside {
                edges.push(x);
                was_inside = now_inside;
            }
        }

        next.clear();
        for &span in edges.as_chunks::<2>().0 {
            let overlap = |stream| {
                (v.f(38) + v.f(40) * variation(v, v.p.time, row, stream))
                    .max(-(span[1] - span[0]) * 0.5)
            };
            let start = DVec2::new(span[1] + overlap(421), y);
            let end = DVec2::new(span[0] - overlap(673), y);
            let continuation = previous
                .iter()
                .position(|&([l, r], _)| l <= span[1] && r >= span[0]);
            let path_index = if let Some(i) = continuation {
                previous.swap_remove(i).1
            } else {
                lines.push(Vec::new());
                lines.len() - 1
            };
            lines[path_index].extend([start, end]);
            next.push((span, path_index));
        }
        std::mem::swap(&mut previous, &mut next);
        y += (spacing + v.f(62) * variation(v, v.p.time, row, 199)).max(spacing * 0.1);
        row += 1;
    }

    lines
        .into_iter()
        .map(|line| {
            let mut smooth = vec![from_hatch(line[0])];
            for (i, corner) in line.windows(3).enumerate() {
                let [a, b, c] = [corner[0], corner[1], corner[2]];
                let t =
                    (v.f(12) + v.f(66) * variation(v, v.p.time, i as u32, 337)).clamp(0., 1.) * 0.5;
                let lhs = b.lerp(a, t);
                let rhs = b.lerp(c, t);
                for j in 0..=4 {
                    let f = f64::from(j) / 4.;
                    smooth.push(from_hatch(lhs.lerp(b, f).lerp(b.lerp(rhs, f), f)));
                }
            }
            smooth.push(from_hatch(*line.last().unwrap()));
            Path::new(smooth)
        })
        .collect()
}

pub(super) fn paint(v: Params<'_>, input: &bae_render::GpuImage) -> Paint {
    let mut paint = Paint::new(v.popup(26) as u32, 2, v.f(24).clamp(0., 1.) as f32);
    let bounds = if v.p.layer_bounds.is_empty() {
        input.rect
    } else {
        v.p.layer_bounds
    };
    let masks = v.p.mask_paths;
    let paths: Vec<_> = masks.iter().map(Path::from_mask).collect();
    let trim_range = trim(v, 30, 32);
    let trim_mask = v.popup(50) != 1 && v.popup(68) == 1;
    let sequential = v.b(28) && v.popup(64) >= 4;
    let intervals = if trim_mask { trim_range } else { [0., 1.] };
    let regions = ranges(&paths, intervals, sequential)
        .map(|(i, path, from, to)| Region::new(v, &masks[i], path, from, to));
    let scribbles: Vec<_> = if v.popup(64) == 5 {
        hatch(v, bounds, regions.collect())
    } else {
        regions
            .flat_map(|region| hatch(v, bounds, vec![region]))
            .collect()
    };
    // Each member is a continuous drawing path.
    let result_trim = if trim_mask { [0., 1.] } else { trim_range };
    for (_, path, from, to) in ranges(&scribbles, result_trim, sequential) {
        for (start, end, _, _) in path.segments(from, to) {
            paint.marks.push(Mark {
                start,
                end,
                radius: v.f(8).clamp(0.1, 50.) * 0.5,
                rgb: color(v, 6),
                hardness: 1.,
                alpha: [1.; 2],
                dash_length: 0.,
            });
        }
    }
    paint
}

//! Stroke segments and stackless bounding-volume hierarchy in pixel coordinates.
use crate::params::Params;
use bae_model::{BezierPath, Value};
type Point = [f32; 2];
fn plus(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}
fn minus(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: Point, s: f32) -> Point {
    [a[0] * s, a[1] * s]
}
fn length(a: Point) -> f32 {
    a[0].hypot(a[1])
}
fn unit(p: Point) -> Point {
    mul(p, length(p).max(1e-8).recip())
}
fn lerp(a: Point, b: Point, t: f32) -> Point {
    plus(a, mul(minus(b, a), t))
}
fn flatten(mask: &BezierPath, scale: f32) -> Vec<Point> {
    if !mask.is_consistent() || mask.vertices.len() < 2 {
        return Vec::new();
    }
    let pt = |v: [f64; 2]| v.map(|c| c as f32 * scale);
    let mut path = vec![pt(mask.vertices[0])];
    let n = mask.vertices.len();
    for i in 0..if mask.closed { n } else { n - 1 } {
        let j = (i + 1) % n;
        let a = pt(mask.vertices[i]);
        let d = pt(mask.vertices[j]);
        let b = plus(a, pt(mask.out_tangents[i]));
        let c = plus(d, pt(mask.in_tangents[j]));
        let steps = if a == b && c == d {
            1
        } else {
            ((length(minus(b, a)) + length(minus(c, b)) + length(minus(d, c))) / 4.0)
                .ceil()
                .clamp(4.0, 128.0) as usize
        };
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let x = lerp(a, b, t);
            let y = lerp(b, c, t);
            let z = lerp(c, d, t);
            path.push(lerp(lerp(x, y, t), lerp(y, z, t), t));
        }
    }
    path
}
#[derive(Clone, Copy)]
struct Line {
    a: Point,
    b: Point,
    phase: f32,
}
/// Each node contains its bounds, its segment, then [end index, leaf flag, hue, unused].
/// A rejected branch skips directly to its end; the GPU needs no traversal stack.
fn tree(lines: &[Line], radius: f32, nodes: &mut Vec<[f32; 12]>) -> [f32; 4] {
    let index = nodes.len();
    nodes.push([0.0; 12]);
    let bounds = if let [line] = lines {
        let b = [
            line.a[0].min(line.b[0]) - radius,
            line.a[1].min(line.b[1]) - radius,
            line.a[0].max(line.b[0]) + radius,
            line.a[1].max(line.b[1]) + radius,
        ];
        nodes[index][4..8].copy_from_slice(&[line.a[0], line.a[1], line.b[0], line.b[1]]);
        nodes[index][9] = 1.0;
        nodes[index][10] = line.phase;
        b
    } else {
        let (left, right) = lines.split_at(lines.len() / 2);
        let l = tree(left, radius, nodes);
        let r = tree(right, radius, nodes);
        [
            l[0].min(r[0]),
            l[1].min(r[1]),
            l[2].max(r[2]),
            l[3].max(r[3]),
        ]
    };
    nodes[index][..4].copy_from_slice(&bounds);
    nodes[index][8] = nodes.len() as f32;
    bounds
}

pub(super) fn nodes(p: Params<'_>, spectrum: bool, radius: f32) -> Vec<[f32; 12]> {
    let Some(Value::FloatList(values)) = p.value(bae_render::effect::AUDIO_VISUAL_VALUES) else {
        return vec![];
    };
    if values.is_empty() {
        return vec![];
    }
    let scale = p.p.scale as f32;
    let point = |id| match p.value(id) {
        Some(Value::Vec2(v)) => v.map(|c| c as f32 * scale),
        _ => [0.0; 2],
    };
    let a = point(2);
    let b = point(3);
    let center = lerp(a, b, 0.5);
    let delta = minus(b, a);
    let polar = spectrum && p.b(5);
    let path = match p.value(bae_render::effect::AUDIO_VISUAL_PATH) {
        Some(Value::Bezier(mask)) => flatten(mask, scale),
        _ => vec![],
    };
    let mut distances = vec![0.0];
    for pair in path.windows(2) {
        distances.push(distances.last().copied().unwrap() + length(minus(pair[1], pair[0])));
    }
    let total = *distances.last().unwrap();
    let locate = |t: f32| {
        if path.len() > 1 && total > 0.0 {
            let s = t * total;
            let i = distances
                .partition_point(|&x| x <= s)
                .saturating_sub(1)
                .min(path.len() - 2);
            let pos = lerp(
                path[i],
                path[i + 1],
                (s - distances[i]) / (distances[i + 1] - distances[i]).max(1e-8),
            );
            let axis = unit(minus(path[i + 1], path[i]));
            (
                pos,
                if polar {
                    unit(minus(pos, center))
                } else {
                    [axis[1], -axis[0]]
                },
            )
        } else if polar {
            let angle = t * std::f32::consts::TAU + (-delta[1]).atan2(-delta[0]);
            let dir = [angle.cos(), angle.sin()];
            (plus(center, mul(dir, length(delta) * 0.5)), dir)
        } else {
            let axis = unit(delta);
            (lerp(a, b, t), [axis[1], -axis[0]])
        }
    };
    let height = p.f(if spectrum { 9 } else { 6 }) as f32 * scale;
    let mode = p.popup(if spectrum { 20 } else { 15 });
    let side = if spectrum { p.popup(21) } else { 1 };
    let mut lines = Vec::with_capacity(values.len());
    let mut previous = [None, None];
    let seed = if spectrum { 0 } else { p.f(11) as u32 };
    let spin = if spectrum && p.b(18) {
        p.p.comp_time.as_seconds() as f32 * (p.f(17) as f32 / 360.0)
    } else {
        0.0
    };
    for (i, &value) in values.iter().enumerate() {
        let t = i as f32 / values.len().saturating_sub(1).max(1) as f32;
        let jitter = if seed > 0 && mode != 1 {
            let hash = (i as u32 ^ seed).wrapping_mul(2654435761);
            (hash as f32 / u32::MAX as f32 - 0.5) / values.len() as f32
        } else {
            0.0
        };
        let (pos, n) = locate((t + jitter).clamp(0.0, 1.0));
        let magnitude = height * value as f32;
        let phase = if spectrum {
            let u = if p.b(19) {
                1.0 - (2.0 * t - 1.0).abs()
            } else {
                t
            };
            u * p.f(17) as f32 / 360.0 + spin
        } else {
            0.0
        };
        for (s, prev) in previous.iter_mut().enumerate() {
            if (s == 0 && side == 2) || (s == 1 && side == 1) {
                continue;
            }
            let tip = plus(pos, mul(n, if s == 0 { magnitude } else { -magnitude }));
            match mode {
                1 => lines.push(Line {
                    a: pos,
                    b: tip,
                    phase,
                }),
                2 => {
                    if let Some(old) = *prev {
                        lines.push(Line {
                            a: old,
                            b: tip,
                            phase,
                        });
                    } else {
                        lines.push(Line {
                            a: tip,
                            b: tip,
                            phase,
                        });
                    }
                }
                _ => lines.push(Line {
                    a: tip,
                    b: tip,
                    phase,
                }),
            }
            *prev = Some(tip);
        }
    }
    let mut nodes = Vec::with_capacity(lines.len() * 2);
    if !lines.is_empty() {
        tree(&lines, radius + 1.0, &mut nodes);
    }
    nodes
}

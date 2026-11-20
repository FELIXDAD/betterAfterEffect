//! Acceptance benchmark (05 task book): 1000 complex paths at 1080p.
//!
//! `cargo run --release -p bae-vector --example bench_paths [-- <paths> <frames>]`
//!
//! Every frame rebuilds the display list from the shape tree (as when every
//! path is animated), encodes it for Vello and renders 1920×1080 on the GPU,
//! waiting for completion. Prints the average and worst time per stage.

use std::time::{Duration, Instant};

use bae_gpu::{GpuContext, GpuContextDescriptor, wgpu};
use bae_model::BezierPath;
use bae_vector::render::{VelloRenderer, encode};
use bae_vector::{
    BlendMode, Brush, Composite, Direction, FillRule, Gradient, GradientKind, GradientStop,
    GroupTransform, LineJoin, Operator, Paint, PaintKind, PathItem, PathSource, ShapeGroup,
    ShapeItem, StarShape, StrokeStyle, TrimPaths, VectorScene,
};
use kurbo::{Affine, Point, Rect};

/// Deterministic pseudo-random numbers (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A closed blob of `n` smooth vertices.
fn blob(rng: &mut Rng, n: usize, radius: f64) -> BezierPath {
    let mut p = BezierPath {
        closed: true,
        ..BezierPath::default()
    };
    for i in 0..n {
        let a = std::f64::consts::TAU * i as f64 / n as f64;
        let r = radius * (0.6 + 0.4 * rng.next());
        let (x, y) = (r * a.cos(), r * a.sin());
        let t = r * 0.35;
        p.vertices.push([x, y]);
        p.in_tangents.push([t * a.sin(), -t * a.cos()]);
        p.out_tangents.push([-t * a.sin(), t * a.cos()]);
    }
    p
}

fn shapes(count: usize, time: f64) -> ShapeGroup {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut root = ShapeGroup::new();
    for i in 0..count {
        let x = rng.next() * 1920.0;
        let y = rng.next() * 1080.0;
        let hue = rng.next() as f32;
        let color = [hue, 0.5 + 0.5 * (1.0 - hue), 1.0 - hue, 0.85];
        let source = if i % 2 == 0 {
            let radius = 20.0 + 40.0 * rng.next();
            PathSource::Bezier(blob(&mut rng, 12, radius))
        } else {
            PathSource::Star(StarShape {
                points: 5.0 + (i % 7) as f64,
                outer_radius: 20.0 + 30.0 * rng.next(),
                inner_radius: 10.0 + 10.0 * rng.next(),
                outer_roundness: 30.0,
                ..StarShape::default()
            })
        };
        let brush = if i % 3 == 0 {
            Brush::Gradient(Gradient {
                kind: GradientKind::Linear,
                start: Point::new(-40.0, 0.0),
                end: Point::new(40.0, 0.0),
                stops: vec![
                    GradientStop { offset: 0.0, color },
                    GradientStop {
                        offset: 1.0,
                        color: [1.0, 1.0, 1.0, 0.9],
                    },
                ],
            })
        } else {
            Brush::Solid(color)
        };
        let mut items = vec![
            ShapeItem::Path(PathItem {
                source,
                direction: Direction::Normal,
            }),
            ShapeItem::Paint(Paint {
                kind: PaintKind::Stroke(StrokeStyle {
                    width: 3.0,
                    join: LineJoin::Round,
                    ..StrokeStyle::default()
                }),
                brush: Brush::Solid([1.0, 1.0, 1.0, 1.0]),
                opacity: 1.0,
                blend: BlendMode::Normal,
                composite: Composite::Below,
            }),
            ShapeItem::Paint(Paint {
                kind: PaintKind::Fill(FillRule::NonZero),
                brush,
                opacity: 1.0,
                blend: BlendMode::Normal,
                composite: Composite::Below,
            }),
        ];
        if i % 5 == 0 {
            items.push(ShapeItem::Operator(Operator::Trim(TrimPaths {
                end: 50.0 + 50.0 * (time + i as f64 * 0.1).sin().abs(),
                ..TrimPaths::default()
            })));
        }
        root.items.push(ShapeItem::Group(ShapeGroup {
            transform: GroupTransform {
                position: [x, y],
                rotation: time * 30.0 + i as f64,
                ..GroupTransform::default()
            },
            items,
            ..ShapeGroup::new()
        }));
    }
    root
}

fn main() {
    let mut args = std::env::args().skip(1);
    let count: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(1000);
    let frames: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);
    let gpu = GpuContext::new_headless(GpuContextDescriptor::default().with_env())
        .expect("a GPU adapter");
    let renderer = VelloRenderer::shared(&gpu).unwrap();
    let (w, h) = (1920u32, 1080u32);
    let viewport = Rect::new(0.0, 0.0, f64::from(w), f64::from(h));

    let mut build = Vec::new();
    let mut enc = Vec::new();
    let mut gpu_time = Vec::new();
    let mut total = Vec::new();
    for frame in 0..frames + 5 {
        let t = frame as f64 / 30.0;
        let tree = shapes(count, t);
        let start = Instant::now();
        let scene = VectorScene::from_group(&tree);
        let built = Instant::now();
        let mut vs = vello::Scene::new();
        encode(&mut vs, &scene, Affine::IDENTITY, viewport);
        let encoded = Instant::now();
        let _texture = renderer.render(&gpu, &vs, w, h).unwrap();
        gpu.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let done = Instant::now();
        if frame >= 5 {
            build.push(built - start);
            enc.push(encoded - built);
            gpu_time.push(done - encoded);
            total.push(done - start);
        }
    }
    let stat = |v: &[Duration]| {
        let avg = v.iter().sum::<Duration>().as_secs_f64() * 1000.0 / v.len() as f64;
        let max = v.iter().max().copied().unwrap_or_default().as_secs_f64() * 1000.0;
        format!("avg {avg:6.2} ms  max {max:6.2} ms")
    };
    println!(
        "{count} paths (fill + stroke, 1/3 gradients, 1/5 trimmed), {w}×{h}, {frames} frames, {}",
        gpu.adapter_info().name
    );
    println!("  display list : {}", stat(&build));
    println!("  vello encode : {}", stat(&enc));
    println!("  gpu render   : {}", stat(&gpu_time));
    println!("  total        : {}", stat(&total));
    let avg = total.iter().sum::<Duration>().as_secs_f64() / total.len() as f64;
    println!("  → {:.0} fps", 1.0 / avg);
}

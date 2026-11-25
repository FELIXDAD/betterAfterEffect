//! Shape rendering on the GPU through Vello. Tests skip (pass with a note) when
//! the machine has no usable adapter; `BAE_REQUIRE_GPU=1` makes that a failure.

use std::sync::Arc;

use bae_gpu::GpuContext;
use bae_model::Time;
use bae_render::{IRect, LayerSource, SourceRequest, testing};
use bae_vector::render::{ShapeSource, VelloRenderer};
use bae_vector::{
    BlendMode, Brush, Composite, Direction, EllipseShape, FillRule, Gradient, GradientKind,
    GradientStop, GroupTransform, Operator, Paint, PaintKind, PathItem, PathSource, RectShape,
    ShapeGroup, ShapeItem, StrokeStyle, TrimPaths, VectorScene,
};
use kurbo::Point;

fn gpu() -> Option<Arc<GpuContext>> {
    let ctx = bae_gpu::testing::shared_context()?;
    // Vello's compute pipelines crash the process (STATUS_ACCESS_VIOLATION) on WARP,
    // the software D3D12 adapter of GPU-less Windows CI runners: skip software
    // adapters like the no-GPU case (see tests/evaluated.rs).
    if ctx.adapter_info().device_type == vello::wgpu::DeviceType::Cpu {
        eprintln!(
            "skipping Vello GPU test on software adapter {:?}",
            ctx.adapter_info().name
        );
        return None;
    }
    Some(ctx)
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn rect(w: f64, h: f64, x: f64, y: f64) -> ShapeItem {
    ShapeItem::Path(PathItem {
        source: PathSource::Rect(RectShape {
            size: [w, h],
            position: [x, y],
            roundness: 0.0,
        }),
        direction: Direction::Normal,
    })
}

fn fill(color: [f32; 4]) -> ShapeItem {
    ShapeItem::Paint(Paint {
        kind: PaintKind::Fill(FillRule::NonZero),
        brush: Brush::Solid(color),
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    })
}

/// Texels of a source rendered through `LayerSource::render` (bae-render's
/// testing entry): straight RGBA8 values of `roi ∩ bounds`, row major.
struct Image {
    texels: Vec<[f32; 4]>,
    /// Covered layer pixels.
    rect: IRect,
    /// Texels per row and rows.
    size: [u32; 2],
}

impl Image {
    fn texel(&self, x: u32, y: u32) -> [u8; 4] {
        let p = self.texels[(y * self.size[0] + x) as usize];
        p.map(|v| (v * 255.0).round() as u8)
    }
}

fn render_source(gpu: &Arc<GpuContext>, source: &ShapeSource, roi: IRect, scale: f64) -> Image {
    let request = SourceRequest {
        roi,
        scale,
        time: Time::ZERO,
    };
    let image = testing::render_source(gpu, source, &request).expect("render");
    Image {
        texels: testing::read_pixels(gpu, &image).expect("readback"),
        rect: roi.intersect(&source.bounds()),
        size: image.texel_size(),
    }
}

/// Renders `root` over `roi` and returns the pixels with the covered rect.
fn render(gpu: &Arc<GpuContext>, root: &ShapeGroup, roi: IRect, scale: f64) -> (Image, IRect) {
    let source = ShapeSource::new(Arc::new(VectorScene::from_group(root)));
    let image = render_source(gpu, &source, roi, scale);
    let rect = image.rect;
    (image, rect)
}

/// RGBA8 of layer pixel (x, y) in an image covering `rect` at scale 1.
fn at(img: &Image, rect: IRect, x: i32, y: i32) -> [u8; 4] {
    img.texel((x - rect.x0) as u32, (y - rect.y0) as u32)
}

#[test]
fn filled_rectangle_around_the_layer_origin() {
    let Some(gpu) = gpu() else { return };
    let root = ShapeGroup::new()
        .with(rect(40.0, 20.0, 0.0, 0.0))
        .with(fill(RED));
    let source = ShapeSource::new(Arc::new(VectorScene::from_group(&root)));
    // Shapes are drawn around the layer origin: bounds start at negative pixels.
    assert_eq!(source.bounds(), IRect::new(-21, -11, 21, 11));
    assert_eq!(source.size(), [21, 11]);
    let (img, r) = render(&gpu, &root, source.bounds(), 1.0);
    assert_eq!(img.size, [42, 22]);
    assert_eq!(at(&img, r, 0, 0), [255, 0, 0, 255]);
    assert_eq!(at(&img, r, -20, -10), [255, 0, 0, 255]);
    assert_eq!(at(&img, r, 19, 9), [255, 0, 0, 255]);
    assert_eq!(at(&img, r, 20, 0)[3], 0, "outside the right edge");
    assert_eq!(at(&img, r, 0, -11)[3], 0, "outside the top edge");
}

#[test]
fn half_resolution_and_partial_roi() {
    let Some(gpu) = gpu() else { return };
    let root = ShapeGroup::new()
        .with(rect(40.0, 40.0, 0.0, 0.0))
        .with(fill(BLUE));
    let source = ShapeSource::new(Arc::new(VectorScene::from_group(&root)));
    let img = render_source(&gpu, &source, IRect::new(0, 0, 40, 40), 0.5);
    // Clipped to the content bounds, half resolution: 21 px → 11 texels.
    assert_eq!(img.rect, IRect::new(0, 0, 21, 21));
    assert_eq!(img.size, [11, 11]);
    assert_eq!(img.texel(0, 0), [0, 0, 255, 255]);
    assert_eq!(img.texel(9, 9), [0, 0, 255, 255]);
    assert_eq!(img.texel(10, 10)[3], 0);
}

#[test]
fn trimmed_stroke_draws_only_the_kept_part() {
    let Some(gpu) = gpu() else { return };
    let root = ShapeGroup::new()
        .with(ShapeItem::Path(PathItem {
            source: PathSource::Ellipse(EllipseShape {
                size: [100.0, 100.0],
                position: [0.0, 0.0],
            }),
            direction: Direction::Normal,
        }))
        .with(ShapeItem::Paint(Paint {
            kind: PaintKind::Stroke(StrokeStyle {
                width: 10.0,
                ..StrokeStyle::default()
            }),
            brush: Brush::Solid(RED),
            opacity: 1.0,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        }))
        .with(ShapeItem::Operator(Operator::Trim(TrimPaths {
            end: 50.0,
            ..TrimPaths::default()
        })));
    let roi = IRect::new(-60, -60, 60, 60);
    let (img, r) = render(&gpu, &root, roi, 1.0);
    // From 12 o'clock clockwise to 6 o'clock: the right half only. The left
    // half is not even inside the content bounds.
    assert!(r.x0 > -30, "{r:?}");
    assert_eq!(at(&img, r, 50, 0), [255, 0, 0, 255]);
    assert_eq!(at(&img, r, 2, -50), [255, 0, 0, 255]);
    assert_eq!(at(&img, r, -3, -50)[3], 0, "butt cap at 12 o'clock");
    assert_eq!(at(&img, r, 0, 0)[3], 0, "stroke, not fill");
}

#[test]
fn even_odd_fill_leaves_a_hole() {
    let Some(gpu) = gpu() else { return };
    let root = ShapeGroup::new()
        .with(rect(60.0, 60.0, 0.0, 0.0))
        .with(rect(20.0, 20.0, 0.0, 0.0))
        .with(ShapeItem::Paint(Paint {
            kind: PaintKind::Fill(FillRule::EvenOdd),
            brush: Brush::Solid(RED),
            opacity: 1.0,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        }));
    let (img, r) = render(&gpu, &root, IRect::new(-31, -31, 31, 31), 1.0);
    assert_eq!(at(&img, r, 0, 0)[3], 0);
    assert_eq!(at(&img, r, 20, 0), [255, 0, 0, 255]);
    // Non-zero with both rectangles clockwise fills the hole.
    let mut nonzero = root.clone();
    if let ShapeItem::Paint(p) = &mut nonzero.items[2] {
        p.kind = PaintKind::Fill(FillRule::NonZero);
    }
    let (img, r) = render(&gpu, &nonzero, IRect::new(-31, -31, 31, 31), 1.0);
    assert_eq!(at(&img, r, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn group_opacity_is_isolated() {
    let Some(gpu) = gpu() else { return };
    // Two overlapping opaque fills in a 50 % group: the overlap stays at 50 %.
    let group = ShapeGroup {
        transform: GroupTransform {
            opacity: 50.0,
            ..GroupTransform::default()
        },
        ..ShapeGroup::new()
    }
    .with(rect(20.0, 20.0, -5.0, 0.0))
    .with(fill(RED))
    .with(rect(20.0, 20.0, 5.0, 0.0))
    .with(fill(BLUE));
    let root = ShapeGroup::new().with(ShapeItem::Group(group));
    let (img, r) = render(&gpu, &root, IRect::new(-20, -20, 20, 20), 1.0);
    let overlap = at(&img, r, 0, 0);
    let single = at(&img, r, 12, 0);
    assert!((i32::from(overlap[3]) - 128).abs() <= 1, "{overlap:?}");
    assert!((i32::from(single[3]) - 128).abs() <= 1, "{single:?}");
    // Red is drawn last (top of the list), so the overlap is red.
    assert_eq!(&overlap[..3], &[255, 0, 0]);
}

#[test]
fn linear_gradient_in_group_space() {
    let Some(gpu) = gpu() else { return };
    let root = ShapeGroup::new()
        .with(rect(100.0, 10.0, 50.0, 0.0))
        .with(ShapeItem::Paint(Paint {
            kind: PaintKind::Fill(FillRule::NonZero),
            brush: Brush::Gradient(Gradient {
                kind: GradientKind::Linear,
                start: Point::new(0.0, 0.0),
                end: Point::new(100.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: RED,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: BLUE,
                    },
                ],
            }),
            opacity: 1.0,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        }));
    let (img, r) = render(&gpu, &root, IRect::new(0, -5, 100, 5), 1.0);
    let left = at(&img, r, 0, 0);
    let mid = at(&img, r, 50, 0);
    let right = at(&img, r, 99, 0);
    assert!(left[0] > 250 && left[2] < 5, "{left:?}");
    assert!(right[2] > 250 && right[0] < 5, "{right:?}");
    assert!((i32::from(mid[0]) - 127).abs() <= 3, "{mid:?}");
    assert!((i32::from(mid[2]) - 127).abs() <= 3, "{mid:?}");
}

#[test]
fn shared_renderer_is_reused() {
    let Some(gpu) = gpu() else { return };
    let a = VelloRenderer::shared(&gpu).unwrap();
    let kept = Arc::downgrade(&a);
    drop(a);
    // Kept for the context's lifetime, not just while someone holds it.
    let b = VelloRenderer::shared(&gpu).unwrap();
    assert!(std::ptr::eq(kept.as_ptr(), Arc::as_ptr(&b)));
}

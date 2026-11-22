//! The CPU backend (`vello_cpu`), used on software adapters such as WARP where
//! Vello's compute pipelines crash. These tests run on every adapter, WARP
//! included; on hardware adapters the CPU result is compared with Vello's.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_render::{IRect, testing};
use bae_vector::render::{ShapeSource, VectorBackend, VelloRenderer};
use bae_vector::{
    BlendMode, Brush, Composite, Direction, EllipseShape, FillRule, Gradient, GradientKind,
    GradientStop, GroupTransform, Operator, Paint, PaintKind, PathItem, PathSource, RectShape,
    ShapeGroup, ShapeItem, StrokeStyle, TrimPaths, VectorScene,
};
use kurbo::Point;

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn paint(kind: PaintKind, brush: Brush) -> ShapeItem {
    ShapeItem::Paint(Paint {
        kind,
        brush,
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    })
}

/// Fills, strokes, a gradient, trim paths and an isolated half-opaque group.
fn scene() -> VectorScene {
    let faded = ShapeGroup {
        transform: GroupTransform {
            position: [60.0, 0.0],
            opacity: 50.0,
            ..GroupTransform::default()
        },
        ..ShapeGroup::new()
    }
    .with(ShapeItem::Path(PathItem {
        source: PathSource::Rect(RectShape {
            size: [30.0, 30.0],
            position: [-8.0, 0.0],
            roundness: 0.0,
        }),
        direction: Direction::Normal,
    }))
    .with(paint(
        PaintKind::Fill(FillRule::NonZero),
        Brush::Solid([0.0, 1.0, 0.0, 1.0]),
    ))
    .with(ShapeItem::Path(PathItem {
        source: PathSource::Rect(RectShape {
            size: [30.0, 30.0],
            position: [8.0, 0.0],
            roundness: 0.0,
        }),
        direction: Direction::Normal,
    }))
    .with(paint(
        PaintKind::Fill(FillRule::NonZero),
        Brush::Solid([0.0, 0.0, 1.0, 1.0]),
    ));
    let root = ShapeGroup::new()
        .with(ShapeItem::Path(PathItem {
            source: PathSource::Ellipse(EllipseShape {
                size: [80.0, 80.0],
                position: [-40.0, 0.0],
            }),
            direction: Direction::Normal,
        }))
        .with(paint(
            PaintKind::Stroke(StrokeStyle {
                width: 8.0,
                ..StrokeStyle::default()
            }),
            Brush::Solid([1.0, 1.0, 1.0, 1.0]),
        ))
        .with(paint(
            PaintKind::Fill(FillRule::NonZero),
            Brush::Gradient(Gradient {
                kind: GradientKind::Linear,
                start: Point::new(-80.0, 0.0),
                end: Point::new(0.0, 0.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: [1.0, 0.0, 0.0, 1.0],
                    },
                    GradientStop {
                        offset: 1.0,
                        color: [1.0, 1.0, 0.0, 1.0],
                    },
                ],
            }),
        ))
        .with(ShapeItem::Operator(Operator::Trim(TrimPaths {
            end: 75.0,
            ..TrimPaths::default()
        })))
        .with(ShapeItem::Group(faded));
    VectorScene::from_group(&root)
}

/// Premultiplied texels of `source` over its bounds at full resolution.
fn premultiplied(
    gpu: &Arc<GpuContext>,
    renderer: &VelloRenderer,
    source: &ShapeSource,
) -> Vec<[f32; 4]> {
    use bae_render::LayerSource;
    let image = source
        .render_with(renderer, gpu, source.bounds(), 1.0)
        .expect("render");
    let texels = testing::read_pixels(gpu, &image).expect("readback");
    match renderer.backend() {
        // Vello writes straight alpha, vello_cpu premultiplied.
        VectorBackend::Gpu => texels
            .into_iter()
            .map(|[r, g, b, a]| [r * a, g * a, b * a, a])
            .collect(),
        VectorBackend::Cpu => texels,
    }
}

#[test]
fn software_adapters_use_the_cpu_backend() {
    let Some(gpu) = gpu() else { return };
    if std::env::var_os(bae_vector::render::BACKEND_ENV).is_some() {
        return;
    }
    let expected = if gpu.adapter_info().device_type == wgpu::DeviceType::Cpu {
        VectorBackend::Cpu
    } else {
        VectorBackend::Gpu
    };
    assert_eq!(VectorBackend::for_adapter(&gpu), expected);
    // The CPU backend never creates Vello's pipelines, so it cannot use them.
    let cpu = VelloRenderer::with_backend(&gpu, VectorBackend::Cpu).unwrap();
    assert!(
        cpu.render(&gpu, &vello::Scene::new(), 4, 4).is_err(),
        "GPU path refused"
    );
}

#[test]
fn cpu_backend_draws_the_scene() {
    let Some(gpu) = gpu() else { return };
    let cpu = VelloRenderer::with_backend(&gpu, VectorBackend::Cpu).unwrap();
    let source = ShapeSource::new(Arc::new(scene()));
    let bounds = bae_render::LayerSource::bounds(&source);
    let px = premultiplied(&gpu, &cpu, &source);
    let width = bounds.width() as i32;
    let at = |x: i32, y: i32| px[((y - bounds.y0) * width + (x - bounds.x0)) as usize];
    let near = |p: [f32; 4], q: [f32; 4]| p.iter().zip(q).all(|(a, b)| (a - b).abs() < 0.02);
    // Gradient fill of the trimmed ellipse (red at the left, yellow towards x = 0);
    // the fill is closed by a chord, so its left part near 9 o'clock is still filled.
    let left = at(-70, 5);
    assert!(
        near(left, [1.0, left[1], 0.0, 1.0]) && left[1] < 0.5,
        "{left:?}"
    );
    // White stroke on the right of the ellipse (trim keeps 12 → 9 o'clock clockwise).
    assert!(near(at(0, 0), [1.0, 1.0, 1.0, 1.0]), "{:?}", at(0, 0));
    // The isolated 50 % group: the blue fill (lower) paints both rectangles, the
    // green fill (upper) only the first, in front; overlaps stay at 50 %.
    assert!(near(at(60, 0), [0.0, 0.5, 0.0, 0.5]), "{:?}", at(60, 0));
    assert!(near(at(75, 0), [0.0, 0.0, 0.5, 0.5]), "{:?}", at(75, 0));
    // Outside everything.
    assert_eq!(at(20, -38)[3], 0.0);
}

#[test]
fn cpu_backend_matches_vello_on_hardware_adapters() {
    let Some(gpu) = gpu() else { return };
    if gpu.adapter_info().device_type == wgpu::DeviceType::Cpu {
        eprintln!("skipping the Vello comparison on a software adapter");
        return;
    }
    let source = ShapeSource::new(Arc::new(scene()));
    let cpu = premultiplied(
        &gpu,
        &VelloRenderer::with_backend(&gpu, VectorBackend::Cpu).unwrap(),
        &source,
    );
    let vello = premultiplied(
        &gpu,
        &VelloRenderer::with_backend(&gpu, VectorBackend::Gpu).unwrap(),
        &source,
    );
    assert_eq!(cpu.len(), vello.len());
    let diffs: Vec<f32> = cpu
        .iter()
        .zip(&vello)
        .map(|(a, b)| (0..4).map(|c| (a[c] - b[c]).abs()).fold(0.0, f32::max))
        .collect();
    let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
    let big = diffs.iter().filter(|d| **d > 0.1).count();
    // Same geometry; only anti-aliasing at edges differs.
    assert!(mean < 0.01, "mean difference {mean}");
    assert!(
        big * 100 < diffs.len(),
        "{big} of {} texels differ by > 0.1",
        diffs.len()
    );
}

#[test]
fn empty_regions_render_on_the_cpu_backend() {
    let Some(gpu) = gpu() else { return };
    let cpu = VelloRenderer::with_backend(&gpu, VectorBackend::Cpu).unwrap();
    let source = ShapeSource::new(Arc::new(VectorScene::new()));
    let image = source
        .render_with(&cpu, &gpu, IRect::new(0, 0, 10, 10), 1.0)
        .unwrap();
    assert_eq!(image.texel_size(), [0, 0]);
}

/// Diagnostic for the Windows test machine (not run by default): forces Vello's
/// GPU path on whatever adapter is selected and prints a marker before each
/// stage, so a crash on WARP shows whether it happens while creating the
/// pipelines (shader compilation in WARP's JIT) or while dispatching them.
///
/// ```text
/// set BAE_GPU_ADAPTER=software
/// set BAE_GPU_VALIDATION=1
/// cargo test -p bae-vector --test cpu -- --ignored --nocapture vello_gpu_stages
/// ```
#[test]
#[ignore = "diagnostic: may crash the process on software adapters"]
fn vello_gpu_stages() {
    let Some(gpu) = gpu() else { return };
    let info = gpu.adapter_info();
    eprintln!(
        "[vello-diag] adapter {:?} ({:?}, {:?}, driver {:?} {:?})",
        info.name, info.device_type, info.backend, info.driver, info.driver_info
    );
    eprintln!("[vello-diag] stage 1: creating Vello pipelines");
    let renderer = VelloRenderer::with_backend(&gpu, VectorBackend::Gpu).unwrap();
    eprintln!("[vello-diag] stage 2: rendering an empty scene");
    renderer.render(&gpu, &vello::Scene::new(), 64, 64).unwrap();
    gpu.wait_idle().unwrap();
    eprintln!("[vello-diag] stage 3: rendering the test scene");
    let source = ShapeSource::new(Arc::new(scene()));
    let bounds = bae_render::LayerSource::bounds(&source);
    let image = source.render_with(&renderer, &gpu, bounds, 1.0).unwrap();
    gpu.wait_idle().unwrap();
    eprintln!("[vello-diag] stage 4: reading back");
    let px = testing::read_pixels(&gpu, &image).unwrap();
    let covered = px.iter().filter(|p| p[3] > 0.5).count();
    eprintln!(
        "[vello-diag] done: {covered} of {} texels covered",
        px.len()
    );
}

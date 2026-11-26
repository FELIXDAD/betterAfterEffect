//! Text on the CPU backend (`vello_cpu` with skrifa outlines), used on software
//! adapters such as WARP. Runs on every adapter; on hardware adapters the result
//! is compared with Vello's.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::TextDocument;
use bae_render::{LayerSource, testing};
use bae_text::render::TextSource;
use bae_text::{FontSystem, layout_text};
use bae_vector::render::{VectorBackend, VelloRenderer};

fn gpu() -> Option<Arc<GpuContext>> {
    bae_gpu::testing::shared_context()
}

fn source() -> Option<TextSource> {
    let mut fonts = FontSystem::new();
    let doc = {
        let mut doc = TextDocument::new("Hi 中文", "Arial", 48.0);
        doc.style.fill_color = [1., 0., 0., 1.];
        doc.style.stroke_color = Some([1.; 4]);
        doc.style.stroke_width = 4.;
        doc.style.faux_bold = true;
        doc.style.faux_italic = true;
        doc
    };
    let layout = layout_text(&mut fonts, &doc);
    if layout.is_empty() {
        eprintln!("skipping: no fonts");
        return None;
    }
    Some(TextSource::new(Arc::new(layout)))
}

fn premultiplied(
    gpu: &Arc<GpuContext>,
    renderer: &VelloRenderer,
    source: &TextSource,
) -> Vec<[f32; 4]> {
    let image = source
        .render_with(renderer, gpu, source.bounds(), 1.0)
        .expect("render");
    let texels = testing::read_pixels(gpu, &image).expect("readback");
    match renderer.backend() {
        VectorBackend::Gpu => texels
            .into_iter()
            .map(|[r, g, b, a]| [r * a, g * a, b * a, a])
            .collect(),
        VectorBackend::Cpu => texels,
    }
}

#[test]
fn cpu_backend_draws_text() {
    let Some(gpu) = gpu() else { return };
    let Some(source) = source() else { return };
    let cpu = VelloRenderer::with_backend(&gpu, VectorBackend::Cpu).unwrap();
    let px = premultiplied(&gpu, &cpu, &source);
    let opaque = px.iter().filter(|p| p[3] > 0.99).count();
    assert!(
        opaque * 10 > px.len(),
        "{opaque} of {} texels opaque",
        px.len()
    );
    // Fill (red) over stroke (white): both visible.
    let close = |p: &[f32; 4], q: [f32; 4]| p.iter().zip(q).all(|(a, b)| (a - b).abs() < 0.02);
    assert!(px.iter().any(|p| close(p, [1.0, 0.0, 0.0, 1.0])));
    assert!(px.iter().any(|p| close(p, [1.0, 1.0, 1.0, 1.0])));
}

#[test]
fn cpu_text_matches_vello_on_hardware_adapters() {
    let Some(gpu) = gpu() else { return };
    if gpu.adapter_info().device_type == wgpu::DeviceType::Cpu {
        eprintln!("skipping the Vello comparison on a software adapter");
        return;
    }
    let Some(source) = source() else { return };
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
    let coverage = |px: &[[f32; 4]]| px.iter().map(|p| p[3]).sum::<f32>();
    let (c, v) = (coverage(&cpu), coverage(&vello));
    assert!((c - v).abs() / v < 0.03, "coverage cpu {c} vs vello {v}");
    let mean = cpu
        .iter()
        .zip(&vello)
        .map(|(a, b)| (0..4).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max))
        .sum::<f32>()
        / cpu.len() as f32;
    assert!(mean < 0.03, "mean difference {mean}");
}

#[test]
fn character_opacity_composites_fill_and_stroke_together() {
    let Some(gpu) = gpu() else { return };
    let mut doc = TextDocument::new("H", "ArialMT", 48.0);
    doc.style.stroke_color = Some([1., 0., 0., 1.]);
    doc.style.stroke_width = 5.;
    let mut layout = layout_text(&mut FontSystem::new(), &doc);
    let group = bae_anim::EvaluatedPrim {
        path: bae_model::PrimPath::new("/animator").unwrap(),
        prim_type: Some(bae_model::PrimType::TEXT_ANIMATOR),
        attrs: Arc::new([("animator:opacity".into(), bae_model::Value::Float(50.))].into()),
        children: Vec::new().into(),
    };
    bae_text::animator::apply_animators(&mut layout, &[group]);
    let source = TextSource::new(Arc::new(layout));
    for backend in [VectorBackend::Cpu, VectorBackend::Gpu] {
        if backend == VectorBackend::Gpu && gpu.adapter_info().device_type == wgpu::DeviceType::Cpu
        {
            continue;
        }
        let renderer = VelloRenderer::with_backend(&gpu, backend).unwrap();
        let peak = premultiplied(&gpu, &renderer, &source)
            .iter()
            .map(|p| p[3])
            .fold(0.0, f32::max);
        assert!((peak - 0.5).abs() < 0.01, "{backend:?}: peak alpha {peak}");
    }
}

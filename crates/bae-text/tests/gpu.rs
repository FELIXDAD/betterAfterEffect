//! Text drawn on the GPU through Vello, via bae-render's testing entry (the same
//! `LayerSource::render` path the renderer uses). Skips without an adapter, on
//! software adapters (WARP: Vello's compute pipelines crash the process there),
//! or without the needed system fonts; `BAE_REQUIRE_GPU=1` makes a missing
//! adapter a failure.

use std::sync::Arc;

use bae_gpu::{GpuContext, wgpu};
use bae_model::{TextDocument, Time};
use bae_render::{IRect, LayerSource, SourceRequest, testing};
use bae_text::render::{TextProvider, TextSource};
use bae_text::{FontMatch, FontSystem, TEXT_PROVIDER, layout_text};

fn gpu() -> Option<Arc<GpuContext>> {
    let ctx = bae_gpu::testing::shared_context()?;
    if ctx.adapter_info().device_type == wgpu::DeviceType::Cpu {
        eprintln!(
            "skipping Vello GPU test on software adapter {:?}",
            ctx.adapter_info().name
        );
        return None;
    }
    Some(ctx)
}

fn doc(text: &str) -> TextDocument {
    TextDocument {
        text: text.to_owned(),
        font: "PingFangSC-Regular".to_owned(),
        font_size: 64.0,
        fill_color: [1.0, 0.0, 0.0, 1.0],
        stroke_color: None,
        stroke_width: 0.0,
        stroke_over_fill: false,
        justification: "left".to_owned(),
        tracking: 0.0,
        leading: None,
        baseline_shift: 0.0,
        faux_bold: false,
        faux_italic: false,
        all_caps: false,
        small_caps: false,
        box_size: None,
        box_position: None,
    }
}

/// Renders `roi` (layer pixels) at full resolution through the trait and
/// returns the straight RGBA texels of `roi ∩ bounds`, row major, and the width.
fn render(gpu: &Arc<GpuContext>, source: &TextSource, roi: IRect) -> (Vec<[f32; 4]>, u32) {
    let request = SourceRequest {
        roi,
        scale: 1.0,
        time: Time::ZERO,
    };
    let image = testing::render_source(gpu, source, &request).expect("render");
    let pixels = testing::read_pixels(gpu, &image).expect("readback");
    (pixels, image.texel_size()[0])
}

/// Fraction of texels that are (mostly) opaque.
fn coverage(pixels: &[[f32; 4]]) -> f64 {
    pixels.iter().filter(|p| p[3] > 0.5).count() as f64 / pixels.len() as f64
}

fn is(p: [f32; 4], rgba: [f32; 4]) -> bool {
    p.iter().zip(rgba).all(|(a, b)| (a - b).abs() < 0.01)
}

#[test]
fn chinese_text_renders_above_the_baseline() {
    let Some(gpu) = gpu() else { return };
    let mut fonts = FontSystem::new();
    if fonts.resolve("PingFangSC-Regular") == FontMatch::Missing
        && fonts.resolve("MicrosoftYaHei") == FontMatch::Missing
    {
        eprintln!("skipping: no Chinese system font");
        return;
    }
    let layout = Arc::new(layout_text(&mut fonts, &doc("中文")));
    let source = TextSource::new(layout.clone());
    let bounds = source.bounds();
    // Drawn around the origin: ink above the baseline is at negative y.
    assert!(
        bounds.y0 < -40 && bounds.y1 > 0 && bounds.y1 < 30,
        "{bounds:?}"
    );
    // Tight ink bounds: the left side bearing keeps x0 slightly right of 0.
    assert!(
        bounds.x0 >= -1 && bounds.x0 < 15 && bounds.x1 > 100,
        "{bounds:?}"
    );
    let (px, width) = render(&gpu, &source, bounds);
    assert_eq!(px.len() as u32, width * bounds.height());
    let c = coverage(&px);
    assert!(c > 0.15 && c < 0.8, "ink coverage {c}");
    // Straight alpha: every fully covered pixel carries the fill colour.
    assert!(
        px.iter()
            .filter(|p| p[3] > 0.99)
            .all(|p| is(*p, [1.0, 0.0, 0.0, 1.0]))
    );
    // The last row (the anti-aliasing margin below the ink) is empty.
    let below = &px[px.len() - width as usize..];
    assert!(below.iter().all(|p| p[3] == 0.0));
}

#[test]
fn stroke_and_faux_bold_add_ink() {
    let Some(gpu) = gpu() else { return };
    let mut fonts = FontSystem::new();
    let plain = Arc::new(layout_text(&mut fonts, &doc("Hi 中")));
    if plain.is_empty() {
        eprintln!("skipping: nothing laid out");
        return;
    }
    let mut stroked_doc = doc("Hi 中");
    stroked_doc.stroke_color = Some([1.0, 1.0, 1.0, 1.0]);
    stroked_doc.stroke_width = 6.0;
    let stroked = Arc::new(layout_text(&mut fonts, &stroked_doc));
    let mut bold_doc = doc("Hi 中");
    bold_doc.faux_bold = true;
    let bold = Arc::new(layout_text(&mut fonts, &bold_doc));

    // Render all three over the same region so coverage is comparable.
    let region = TextSource::new(stroked.clone()).bounds();
    let ink = |layout: &Arc<bae_text::TextLayout>| {
        let (px, _) = render(&gpu, &TextSource::new(layout.clone()), region);
        px.iter().filter(|p| p[3] > 0.5).count()
    };
    let (p, s, b) = (ink(&plain), ink(&stroked), ink(&bold));
    assert!(s > p, "stroke adds ink: {s} vs {p}");
    assert!(b > p, "faux bold adds ink: {b} vs {p}");
    // The stroke (white) is under the fill (red) by default: some white shows.
    let (px, _) = render(&gpu, &TextSource::new(stroked), region);
    assert!(px.iter().any(|p| is(*p, [1.0, 1.0, 1.0, 1.0])));
    assert!(px.iter().any(|p| is(*p, [1.0, 0.0, 0.0, 1.0])));
}

#[test]
fn provider_lays_out_documents_and_caches_by_hash() {
    use bae_model::PrimPath;
    use bae_render::{CustomContent, SourceProvider};
    let provider = TextProvider::new();
    let content = CustomContent {
        provider: TEXT_PROVIDER.to_owned(),
        prim: PrimPath::new("/comps/main/layers/title").unwrap(),
        size: None,
        hash: 7,
        data: Arc::new(doc("Title 标题")),
    };
    let a = provider.source(&content).unwrap();
    let b = provider.source(&content).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    let bad = CustomContent {
        data: Arc::new(1_u8),
        hash: 8,
        ..content
    };
    assert!(provider.source(&bad).is_err());
}

#[test]
fn evaluated_text_layer_reaches_the_provider() {
    use bae_anim::evaluate_comp;
    use bae_model::{FrameRate, PrimPath, Time, Value};
    use bae_render::fixture::SceneBuilder;
    use bae_render::{CustomContent, SourceProvider};

    let mut b = SceneBuilder::new();
    let comp = b
        .comp(
            "main",
            640,
            360,
            FrameRate::FPS_30,
            Time::from_flicks(Time::FLICKS_PER_SECOND),
        )
        .unwrap();
    let layer = b.layer(&comp, "title", "text", None).unwrap();
    b.set(
        &layer,
        "text:source",
        Value::TextDocument(Box::new(doc("你好 BAE"))),
    )
    .unwrap();
    let mut stage = b.into_stage();
    let registry = bae_model::CommandRegistry::with_builtins();
    let author = bae_model::AuthorId::new("user:test").unwrap();
    let group = registry
        .execute(
            &mut stage,
            &author,
            "text.create_animator",
            serde_json::json!({"layer":layer, "name":"rise"}),
        )
        .unwrap()
        .output;
    registry.execute(&mut stage,&author,"attr.set",serde_json::json!({"path":group["path"], "attr":"animator:position","value":Value::Vec3([10.0,20.0,0.0])})).unwrap();
    let evaluated = evaluate_comp(&stage, &comp, Time::ZERO).unwrap();
    let l = evaluated.layer(&layer).unwrap();
    assert_eq!(l.text.as_ref().map(|d| d.text.as_str()), Some("你好 BAE"));

    let provider = TextProvider::new();
    let content = CustomContent {
        provider: TEXT_PROVIDER.to_owned(),
        prim: PrimPath::new(layer.as_str()).unwrap(),
        size: None,
        hash: 1,
        data: Arc::new(l.clone()),
    };
    let layout = provider.layout_of(&content).unwrap();
    assert_eq!(layout.chars.len(), 6);
    let base = provider.layout(l.text.as_deref().unwrap());
    let delta = kurbo::Vec2::new(10., 20.);
    assert_eq!(layout.bounds(true), base.bounds(true).map(|r| r + delta));
    assert_ne!(layout.content_hash(), base.content_hash());
    assert!(provider.source(&content).is_ok());
}

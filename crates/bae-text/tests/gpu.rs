//! Text drawn on the GPU through Vello. Skips without an adapter (or without the
//! needed system fonts); `BAE_REQUIRE_GPU=1` makes a missing adapter a failure.

use std::sync::Arc;

use bae_gpu::transfer::read_texture;
use bae_gpu::{GpuContext, GpuContextDescriptor, GpuError};
use bae_model::TextDocument;
use bae_render::{IRect, ImageAlpha, LayerSource};
use bae_text::render::{TextProvider, TextSource};
use bae_text::{FontMatch, FontSystem, TEXT_PROVIDER, layout_text};

fn gpu() -> Option<Arc<GpuContext>> {
    match GpuContext::new_headless(GpuContextDescriptor::default().with_env()) {
        Ok(ctx) => Some(ctx),
        Err(error @ GpuError::NoAdapter { .. }) => {
            if std::env::var_os("BAE_REQUIRE_GPU").is_some_and(|v| v == "1") {
                panic!("BAE_REQUIRE_GPU=1 but {error}");
            }
            eprintln!("skipping GPU test: {error}");
            None
        }
        Err(error) => panic!("GPU context creation failed: {error}"),
    }
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

/// Fraction of pixels in the image that are (mostly) opaque.
fn coverage(pixels: &bae_gpu::transfer::ImageData) -> f64 {
    let opaque = pixels.data.chunks(4).filter(|p| p[3] > 128).count();
    opaque as f64 / f64::from(pixels.width * pixels.height)
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
    let image = source.render_on(&gpu, bounds, 1.0).unwrap();
    assert_eq!(image.alpha, ImageAlpha::Straight);
    let px = read_texture(&gpu, &image.texture).unwrap();
    let c = coverage(&px);
    assert!(c > 0.15 && c < 0.8, "ink coverage {c}");
    // Every opaque pixel is the fill colour.
    assert!(
        px.data
            .chunks(4)
            .filter(|p| p[3] == 255)
            .all(|p| p[0] == 255 && p[1] == 0 && p[2] == 0)
    );
    // Below the descent there is nothing.
    let below = px.row(px.height - 1);
    assert!(below.chunks(4).all(|p| p[3] == 0));
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
        let image = TextSource::new(layout.clone())
            .render_on(&gpu, region, 1.0)
            .unwrap();
        let px = read_texture(&gpu, &image.texture).unwrap();
        px.data.chunks(4).filter(|p| p[3] > 128).count()
    };
    let (p, s, b) = (ink(&plain), ink(&stroked), ink(&bold));
    assert!(s > p, "stroke adds ink: {s} vs {p}");
    assert!(b > p, "faux bold adds ink: {b} vs {p}");
    // The stroke (white) is under the fill (red) by default: some white shows.
    let image = TextSource::new(stroked)
        .render_on(&gpu, region, 1.0)
        .unwrap();
    let px = read_texture(&gpu, &image.texture).unwrap();
    assert!(px.data.chunks(4).any(|p| p == [255, 255, 255, 255]));
    assert!(px.data.chunks(4).any(|p| p == [255, 0, 0, 255]));
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
    let _ = IRect::EMPTY;
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
    let stage = b.into_stage();
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
    assert!(provider.source(&content).is_ok());
}

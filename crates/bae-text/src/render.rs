//! GPU drawing of text with Vello (feature `render`).
//!
//! * [`encode`] appends a [`TextLayout`] to a `vello::Scene` (glyph outlines,
//!   fill and stroke in AE's per-character paint order, faux bold/italic).
//! * [`TextSource`] / [`TextProvider`] plug text layers into bae-render
//!   ([`crate::TEXT_PROVIDER`]); rendering goes through bae-vector's shared
//!   [`VelloRenderer`].

use std::sync::Arc;

use bae_anim::EvaluatedLayer;
use bae_gpu::GpuContext;
use bae_model::{TextDocument, Time};
use bae_render::{
    CustomContent, EncodeContext, GpuImage, IRect, LayerSource, RenderError, SourceProvider,
    SourceRequest,
};
use bae_vector::render::VelloRenderer;
use kurbo::{Affine, Diagonal2};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use vello::peniko::{self, Color};
use vello::{FontEmbolden, Glyph, NormalizedCoord};

use crate::fonts::FontSystem;
use crate::layout::{GlyphRun, PositionedGlyph, TextLayout, layout_text};

/// Appends `text` to `scene` with `transform` (layer space → target pixels).
pub fn encode(scene: &mut vello::Scene, text: &TextLayout, transform: Affine) {
    let fill = text.paint.fill.map(|c| peniko::Brush::Solid(Color::new(c)));
    let stroke = text.paint.stroke.map(|s| {
        let style = bae_vector::StrokeStyle {
            width: s.width,
            join: s.join,
            miter_limit: s.miter_limit,
            ..bae_vector::StrokeStyle::default()
        }
        .to_kurbo();
        (style, peniko::Brush::Solid(Color::new(s.color)))
    });
    for run in &text.runs {
        match (&fill, &stroke) {
            (None, None) => {}
            (Some(f), None) => draw(scene, run, transform, &run.glyphs, Paint::Fill(f)),
            (None, Some((style, brush))) => draw(
                scene,
                run,
                transform,
                &run.glyphs,
                Paint::Stroke(style, brush),
            ),
            (Some(f), Some((style, brush))) => {
                // Per character: a later character's paint covers an earlier one's.
                for g in &run.glyphs {
                    let one = std::slice::from_ref(g);
                    if text.paint.stroke_over_fill {
                        draw(scene, run, transform, one, Paint::Fill(f));
                        draw(scene, run, transform, one, Paint::Stroke(style, brush));
                    } else {
                        draw(scene, run, transform, one, Paint::Stroke(style, brush));
                        draw(scene, run, transform, one, Paint::Fill(f));
                    }
                }
            }
        }
    }
}

enum Paint<'a> {
    Fill(&'a peniko::Brush),
    Stroke(&'a kurbo::Stroke, &'a peniko::Brush),
}

fn draw(
    scene: &mut vello::Scene,
    run: &GlyphRun,
    transform: Affine,
    glyphs: &[PositionedGlyph],
    paint: Paint<'_>,
) {
    // vello's `NormalizedCoord` is the raw F2Dot14 bits.
    let coords: &[NormalizedCoord] = &run.normalized_coords;
    let skew = f64::from(run.skew_degrees).to_radians().tan();
    let mut builder = scene
        .draw_glyphs(&run.font)
        .font_size(run.font_size)
        .transform(transform)
        .normalized_coords(coords);
    if skew != 0.0 {
        // Lean right: points above the baseline (y < 0) move to +x.
        builder = builder.glyph_transform(Some(Affine::new([1.0, 0.0, -skew, 1.0, 0.0, 0.0])));
    }
    if run.embolden > 0.0 {
        let e = f64::from(run.embolden);
        builder = builder.font_embolden(FontEmbolden::new(Diagonal2::new(e, e)));
    }
    let glyphs = glyphs.iter().map(|g| Glyph {
        id: g.id,
        x: g.x,
        y: g.y,
    });
    match paint {
        Paint::Fill(brush) => builder.brush(brush).draw(peniko::Fill::NonZero, glyphs),
        Paint::Stroke(style, brush) => builder.brush(brush).draw(style, glyphs),
    }
}

/// Integer layer-pixel bounds of a layout (stroke and one pixel of
/// anti-aliasing included); empty when nothing is drawn.
pub fn pixel_bounds(text: &TextLayout) -> IRect {
    match text.bounds(true) {
        Some(r) if r.is_finite() => {
            let clamp = |v: f64| v.clamp(-f64::from(1 << 24), f64::from(1 << 24));
            IRect::new(
                clamp(r.x0.floor()) as i32 - 1,
                clamp(r.y0.floor()) as i32 - 1,
                clamp(r.x1.ceil()) as i32 + 1,
                clamp(r.y1.ceil()) as i32 + 1,
            )
        }
        _ => IRect::EMPTY,
    }
}

/// A laid-out text layer as a bae-render layer source.
#[derive(Debug)]
pub struct TextSource {
    layout: Arc<TextLayout>,
    bounds: IRect,
    hash: u64,
}

impl TextSource {
    /// A source drawing `layout` (layer space).
    pub fn new(layout: Arc<TextLayout>) -> Self {
        Self {
            bounds: pixel_bounds(&layout),
            hash: layout.content_hash(),
            layout,
        }
    }

    /// The layout.
    pub fn layout(&self) -> &Arc<TextLayout> {
        &self.layout
    }

    /// Renders `roi` at `scale` on `gpu` with the shared Vello renderer.
    pub fn render_on(
        &self,
        gpu: &Arc<GpuContext>,
        roi: IRect,
        scale: f64,
    ) -> Result<GpuImage, RenderError> {
        let roi = roi.intersect(&self.bounds);
        let renderer = VelloRenderer::shared(gpu)?;
        renderer.render_region(gpu, roi, scale, |scene, transform, _| {
            encode(scene, &self.layout, transform);
        })
    }
}

impl LayerSource for TextSource {
    fn size(&self) -> [u32; 2] {
        [self.bounds.x1.max(0) as u32, self.bounds.y1.max(0) as u32]
    }

    /// Text is drawn around the first baseline's start: glyphs above the baseline
    /// have negative y.
    fn bounds(&self) -> IRect {
        self.bounds
    }

    fn content_hash(&self, _time: Time) -> u64 {
        self.hash
    }

    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        request: &SourceRequest,
    ) -> Result<GpuImage, RenderError> {
        self.render_on(ctx.gpu, request.roi, request.scale)
    }
}

/// Creates [`TextSource`]s for [`crate::TEXT_PROVIDER`] content, laying text out
/// with its own [`FontSystem`]. Sources are cached by `CustomContent::hash`.
#[derive(Debug, Default)]
pub struct TextProvider {
    fonts: Mutex<Option<FontSystem>>,
    cache: Mutex<FxHashMap<u64, Arc<TextSource>>>,
}

const CACHE_ENTRIES: usize = 512;

impl TextProvider {
    /// A provider with the system fonts (scanned on first use).
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider using `fonts` (e.g. with project or test fonts registered).
    pub fn with_fonts(fonts: FontSystem) -> Self {
        Self {
            fonts: Mutex::new(Some(fonts)),
            cache: Mutex::default(),
        }
    }

    /// Lays out `doc` with the provider's fonts.
    pub fn layout(&self, doc: &TextDocument) -> TextLayout {
        let mut fonts = self.fonts.lock();
        layout_text(fonts.get_or_insert_with(FontSystem::new), doc)
    }

    /// The layout for custom content: an `EvaluatedLayer` from 03, a
    /// `TextDocument` or a ready `TextLayout`.
    pub fn layout_of(&self, content: &CustomContent) -> Result<Arc<TextLayout>, RenderError> {
        if let Some(layout) = content.data.downcast_ref::<TextLayout>() {
            return Ok(Arc::new(layout.clone()));
        }
        if let Some(doc) = content.data.downcast_ref::<TextDocument>() {
            return Ok(Arc::new(self.layout(doc)));
        }
        if let Some(layer) = content.data.downcast_ref::<EvaluatedLayer>() {
            return Ok(Arc::new(match &layer.text {
                Some(doc) => self.layout(doc),
                None => TextLayout::empty(crate::layout::TextPaint {
                    fill: None,
                    stroke: None,
                    stroke_over_fill: false,
                }),
            }));
        }
        Err(RenderError::Other(format!(
            "{}: `{}` content must be an EvaluatedLayer, a TextDocument or a TextLayout",
            content.prim, content.provider
        )))
    }
}

impl SourceProvider for TextProvider {
    fn id(&self) -> &str {
        crate::TEXT_PROVIDER
    }

    fn source(&self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        if let Some(hit) = self.cache.lock().get(&content.hash) {
            return Ok(hit.clone());
        }
        let source = Arc::new(TextSource::new(self.layout_of(content)?));
        let mut cache = self.cache.lock();
        if cache.len() >= CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(content.hash, source.clone());
        Ok(source)
    }
}

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
use bae_vector::render::{RenderMemo, SourceCache, VectorDraw, VelloRenderer, cpu_paint};
use kurbo::{Affine, BezPath, Diagonal2, Join, Rect};
use parking_lot::Mutex;
use vello::peniko::{self, Color};
use vello::{FontEmbolden, Glyph, NormalizedCoord};

use crate::fonts::FontSystem;
use crate::layout::{GlyphRun, PositionedGlyph, TextLayout, TextPaint, layout_text};

/// Appends `text` to `scene` with `transform` (layer space → target pixels).
pub fn encode(scene: &mut vello::Scene, text: &TextLayout, transform: Affine) {
    encode_clipped(scene, text, transform, None);
}

fn encode_clipped(
    scene: &mut vello::Scene,
    text: &TextLayout,
    transform: Affine,
    mut viewport: Option<Rect>,
) {
    for run in &text.runs {
        for_batches(text, run, transform, |glyphs, matrix, paint, opacity| {
            if opacity < 1.0 {
                let clip = viewport.get_or_insert_with(|| {
                    transform
                        .transform_rect_bbox(text.bounds(true).unwrap_or_default())
                        .inflate(1.0, 1.0)
                });
                scene.push_layer(
                    peniko::Fill::NonZero,
                    peniko::Mix::Normal,
                    opacity,
                    Affine::IDENTITY,
                    clip,
                );
            }
            encode_batch(scene, run, matrix, glyphs, paint);
            if opacity < 1.0 {
                scene.pop_layer();
            }
        });
    }
}

// Plain text stays batched by font run. Adjacent identical animator results can
// also share a glyph draw, including clusters shaped into several glyphs.
fn for_batches(
    text: &TextLayout,
    run: &GlyphRun,
    transform: Affine,
    mut visit: impl FnMut(&[PositionedGlyph], Affine, TextPaint, f32),
) {
    if text.styles.is_empty() {
        visit(&run.glyphs, transform, text.paint, 1.0);
        return;
    }
    for batch in run.glyphs.chunk_by(|a, b| {
        a.char_index == b.char_index
            || (text.styles[a.char_index].opacity == 1.0
                && text.styles[a.char_index] == text.styles[b.char_index])
    }) {
        let style = text.styles[batch[0].char_index];
        let mut paint = style.paint;
        let mut opacity = style.opacity;
        if batch.len() == 1 && (paint.fill.is_none() || paint.stroke.is_none()) {
            // A single paint operation needs no intermediate layer.
            if let Some(fill) = &mut paint.fill {
                fill[3] *= opacity;
            }
            if let Some(stroke) = &mut paint.stroke {
                stroke.color[3] *= opacity;
            }
            opacity = 1.0;
        }
        visit(batch, transform * style.transform, paint, opacity);
    }
}

fn encode_batch(
    scene: &mut vello::Scene,
    run: &GlyphRun,
    transform: Affine,
    glyphs: &[PositionedGlyph],
    paint: TextPaint,
) {
    let fill = paint.fill.map(|c| peniko::Brush::Solid(Color::new(c)));
    let stroke = paint.stroke.map(|s| {
        let style = bae_vector::StrokeStyle {
            width: s.width,
            join: s.join,
            miter_limit: s.miter_limit,
            ..bae_vector::StrokeStyle::default()
        }
        .to_kurbo();
        (style, peniko::Brush::Solid(Color::new(s.color)))
    });
    match (&fill, &stroke) {
        (None, None) => {}
        (Some(f), None) => draw(scene, run, transform, glyphs, Paint::Fill(f)),
        (None, Some((style, brush))) => {
            draw(scene, run, transform, glyphs, Paint::Stroke(style, brush))
        }
        (Some(f), Some((style, brush))) => {
            // Per character: a later character's paint covers an earlier one's.
            for g in glyphs {
                let one = std::slice::from_ref(g);
                if paint.stroke_over_fill {
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
        // Vello applies the glyph transform in font space (y up): leaning right
        // moves points above the baseline (y > 0) to +x.
        builder = builder.glyph_transform(Some(Affine::new([1.0, 0.0, skew, 1.0, 0.0, 0.0])));
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
    memo: RenderMemo,
}

impl TextSource {
    /// A source drawing `layout` (layer space).
    pub fn new(layout: Arc<TextLayout>) -> Self {
        Self {
            bounds: pixel_bounds(&layout),
            hash: layout.content_hash(),
            layout,
            memo: RenderMemo::default(),
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
        self.memo.get_or_render(gpu, roi, scale, || {
            VelloRenderer::shared(gpu)?.render_region(gpu, roi, scale, self.layout.as_ref())
        })
    }

    /// Renders `roi` at `scale` with a specific renderer (e.g. a forced backend).
    pub fn render_with(
        &self,
        renderer: &VelloRenderer,
        gpu: &GpuContext,
        roi: IRect,
        scale: f64,
    ) -> Result<GpuImage, RenderError> {
        renderer.render_region(
            gpu,
            roi.intersect(&self.bounds),
            scale,
            self.layout.as_ref(),
        )
    }
}

impl VectorDraw for TextLayout {
    fn encode(&self, scene: &mut vello::Scene, transform: Affine, viewport: Rect) {
        encode_clipped(scene, self, transform, Some(viewport));
    }

    fn draw_cpu(&self, ctx: &mut vello_cpu::RenderContext, transform: Affine) {
        draw_cpu(ctx, self, transform);
    }
}

/// Draws `text` with `vello_cpu` (software fallback of [`encode`]): glyph
/// outlines from skrifa, filled and stroked as paths in the same per-character
/// order. Colour (COLR/bitmap) glyphs are drawn as their outlines.
pub fn draw_cpu(ctx: &mut vello_cpu::RenderContext, text: &TextLayout, transform: Affine) {
    ctx.set_fill_rule(peniko::Fill::NonZero);
    for run in &text.runs {
        let outlines = glyph_outlines(run);
        let mut paths = outlines.iter();
        for_batches(text, run, transform, |glyphs, matrix, paint, opacity| {
            if opacity < 1.0 {
                ctx.push_layer(None, None, Some(opacity), None, None);
            }
            ctx.set_transform(matrix);
            let fill = paint
                .fill
                .map(|c| cpu_paint(peniko::Brush::Solid(Color::new(c))));
            let stroke = paint.stroke.map(|s| {
                let style = bae_vector::StrokeStyle {
                    width: s.width,
                    join: s.join,
                    miter_limit: s.miter_limit,
                    ..bae_vector::StrokeStyle::default()
                }
                .to_kurbo();
                (style, cpu_paint(peniko::Brush::Solid(Color::new(s.color))))
            });
            for path in paths.by_ref().take(glyphs.len()).flatten() {
                let paint_fill = |ctx: &mut vello_cpu::RenderContext| {
                    if let Some(f) = &fill {
                        ctx.set_paint(f.clone());
                        ctx.fill_path(path);
                    }
                };
                let paint_stroke = |ctx: &mut vello_cpu::RenderContext| {
                    if let Some((style, brush)) = &stroke {
                        ctx.set_paint(brush.clone());
                        ctx.set_stroke(style.clone());
                        ctx.stroke_path(path);
                    }
                };
                if paint.stroke_over_fill {
                    paint_fill(ctx);
                    paint_stroke(ctx);
                } else {
                    paint_stroke(ctx);
                    paint_fill(ctx);
                }
            }
            if opacity < 1.0 {
                ctx.pop_layer();
            }
        });
    }
}

/// Layer-space outlines of a run's glyphs (`None` for glyphs without one),
/// with faux italic shear and faux bold expansion applied.
fn glyph_outlines(run: &GlyphRun) -> Vec<Option<BezPath>> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, NormalizedCoord as Coord, Size};
    use skrifa::outline::DrawSettings;

    let Ok(font) = skrifa::FontRef::from_index(run.font.data.as_ref(), run.font.index) else {
        return vec![None; run.glyphs.len()];
    };
    let coords: Vec<Coord> = run
        .normalized_coords
        .iter()
        .map(|b| Coord::from_bits(*b))
        .collect();
    let outlines = font.outline_glyphs();
    let skew = f64::from(run.skew_degrees).to_radians().tan();
    // Font outlines are y up; layer space is y down. Shear leans right.
    let shape = Affine::new([1.0, 0.0, -skew, 1.0, 0.0, 0.0]) * Affine::FLIP_Y;
    run.glyphs
        .iter()
        .map(|g| {
            let outline = outlines.get(skrifa::GlyphId::new(g.id))?;
            let mut pen = KurboPen(BezPath::new());
            outline
                .draw(
                    DrawSettings::unhinted(Size::new(run.font_size), LocationRef::new(&coords)),
                    &mut pen,
                )
                .ok()?;
            if pen.0.is_empty() {
                return None;
            }
            let mut path = shape * pen.0;
            if run.embolden > 0.0 {
                let e = f64::from(run.embolden);
                path = kurbo::expand_path(&path, Diagonal2::new(e, e), Join::Miter, 4.0, 0.01);
            }
            Some(Affine::translate((f64::from(g.x), f64::from(g.y))) * path)
        })
        .collect()
}

/// Collects a skrifa outline into a kurbo path.
struct KurboPen(BezPath);

impl skrifa::outline::OutlinePen for KurboPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.curve_to(
            (f64::from(cx0), f64::from(cy0)),
            (f64::from(cx1), f64::from(cy1)),
            (f64::from(x), f64::from(y)),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
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
    /// The fonts are loaded (scanning the system fonts takes a while).
    ready: std::sync::atomic::AtomicBool,
    cache: Mutex<SourceCache<TextSource>>,
}

impl TextProvider {
    /// A provider with the system fonts (scanned on first use).
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider using `fonts` (e.g. with project or test fonts registered).
    pub fn with_fonts(fonts: FontSystem) -> Self {
        Self {
            fonts: Mutex::new(Some(fonts)),
            ready: std::sync::atomic::AtomicBool::new(true),
            cache: Mutex::default(),
        }
    }

    /// Expressions' `sourceRectAtTime` measures text and shapes with this
    /// provider (process-wide).
    pub fn measure_expressions(self: &Arc<Self>) {
        bae_anim::set_source_bounds(self.clone());
    }

    /// Scans the system fonts now (the app's warm-up) instead of at the first
    /// text layer.
    pub fn load_fonts(&self) {
        self.fonts.lock().get_or_insert_with(FontSystem::new);
        self.ready.store(true, std::sync::atomic::Ordering::Release);
    }

    /// The fonts for the render path. On an interactive thread it never waits
    /// for the warm-up still scanning them (the frame is drawn again later).
    fn render_fonts(&self) -> Result<parking_lot::MutexGuard<'_, Option<FontSystem>>, RenderError> {
        if !self.ready.load(std::sync::atomic::Ordering::Acquire)
            && bae_render::is_interactive_thread()
            && bae_gpu::warm_up::is_running()
        {
            return match self.fonts.try_lock() {
                Some(fonts) if fonts.is_some() => Ok(fonts),
                _ => Err(bae_render::warming_up("the font list")),
            };
        }
        let mut fonts = self.fonts.lock();
        fonts.get_or_insert_with(FontSystem::new);
        self.ready.store(true, std::sync::atomic::Ordering::Release);
        Ok(fonts)
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
            let mut fonts = self.render_fonts()?;
            return Ok(Arc::new(layout_text(
                fonts.get_or_insert_with(FontSystem::new),
                doc,
            )));
        }
        if let Some(layer) = content.data.downcast_ref::<EvaluatedLayer>() {
            return Ok(Arc::new(match &layer.text {
                Some(doc) => {
                    let mut fonts = self.render_fonts()?;
                    let mut layout = crate::animator::layout_animated(
                        fonts.get_or_insert_with(FontSystem::new),
                        doc,
                        &layer.text_animators,
                    );
                    // Text on a path (AE: Path Options with a mask chosen).
                    if let Some(options) = &layer.text_path
                        && let Some(mask) = layer.masks.iter().find(|m| m.path == options.mask)
                    {
                        crate::path::place_on_path(
                            &mut layout,
                            &mask.shape,
                            options,
                            &doc.justification,
                        );
                    }
                    layout
                }
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
        if let Some(hit) = self.cache.lock().get(content.hash) {
            return Ok(hit);
        }
        let source = Arc::new(TextSource::new(self.layout_of(content)?));
        self.cache.lock().insert(content.hash, source.clone());
        Ok(source)
    }
}

/// `sourceRectAtTime` for the expression engine: text laid out with the
/// provider's fonts, shapes from their geometry (the app registers the
/// provider with `bae_anim::set_source_bounds`).
impl bae_anim::SourceBounds for TextProvider {
    fn text(&self, doc: &TextDocument, include_extents: bool) -> Option<bae_anim::SourceRect> {
        // Expressions run on the UI thread: never wait for the font warm-up.
        let mut fonts = self.render_fonts().ok()?;
        let r = layout_text(fonts.as_mut()?, doc).bounds(include_extents)?;
        Some([r.x0, r.y0, r.width(), r.height()])
    }

    fn shapes(
        &self,
        contents: &[bae_anim::EvaluatedPrim],
        include_extents: bool,
    ) -> Option<bae_anim::SourceRect> {
        let mut warnings = Vec::new();
        let group = bae_vector::ShapeGroup::from_evaluated(contents, &mut warnings);
        let r = bae_vector::VectorScene::from_group(&group).bounds(include_extents)?;
        Some([r.x0, r.y0, r.width(), r.height()])
    }
}

impl bae_gpu::WarmUp for TextProvider {
    fn name(&self) -> &str {
        "fonts"
    }

    /// First of all: text needs it and the scan takes well under a
    /// millisecond (it must never wait behind the compositor or effects).
    fn priority(&self) -> u8 {
        0
    }

    fn run(&self, _gpu: &Arc<bae_gpu::GpuContext>) -> Result<(), String> {
        self.load_fonts();
        Ok(())
    }
}

//! Content stream interpreter (§8, §9): turns a page into drawn items in paint
//! order — paths with their paints, gradient fills (`sh` inside a clip,
//! shading patterns), transparency groups, point text — each tagged with its
//! clipping paths and its optional-content (layer) path. `mod.rs` groups them
//! into Illustrator layers and clip groups.

use std::collections::HashMap;
use std::rc::Rc;

use bae_model::BezierPath;
use bae_vector::kurbo::{Affine, BezPath, Point, Rect, Shape};
use bae_vector::path::from_bez_path;
use bae_vector::{
    BlendMode, Brush, Composite, FillRule, Gradient, GradientKind, GradientStop, LineCap, LineJoin,
    Paint, PaintKind, StrokeStyle,
};

use super::color::{ColorSpace, Function};
use super::file::PdfFile;
use super::font::Font;
use super::lexer::{Dict, Lexer, Obj, Token};
use crate::doc::{AiPath, AiText};

/// Warnings without repeats.
#[derive(Default)]
pub(crate) struct Warnings(pub Vec<String>);

impl Warnings {
    pub fn push(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if !self.0.contains(&msg) && self.0.len() < 200 {
            self.0.push(msg);
        }
    }
}

/// A clipping path in artboard space.
#[derive(Debug)]
pub(crate) struct Clip {
    pub paths: Vec<BezierPath>,
    pub rule: FillRule,
    /// Set when the clip is a single axis-aligned rectangle.
    pub rect: Option<Rect>,
}

/// One drawn element.
#[derive(Debug)]
pub(crate) struct Drawn {
    pub kind: DrawnKind,
    /// Clips that affect it, outermost first.
    pub clips: Vec<Rc<Clip>>,
    /// Optional content groups it is inside (object numbers), outermost first.
    pub layer: Vec<u32>,
    pub bounds: Rect,
}

#[derive(Debug)]
pub(crate) enum DrawnKind {
    Path(AiPath),
    Text(AiText),
    /// A transparency group (Illustrator group with opacity or blend mode).
    Group {
        opacity: f64,
        blend: BlendMode,
        items: Vec<Drawn>,
    },
}

#[derive(Clone, Debug)]
enum Source {
    Color(Rc<ColorSpace>, Vec<f64>),
    Pattern(Obj),
}

#[derive(Clone, Debug)]
struct GState {
    ctm: Affine,
    fill: Source,
    stroke: Source,
    line_width: f64,
    cap: LineCap,
    join: LineJoin,
    miter: f64,
    dash: Vec<f64>,
    dash_phase: f64,
    fill_alpha: f64,
    stroke_alpha: f64,
    blend: BlendMode,
    clips: Vec<Rc<Clip>>,
    font: Option<Rc<Font>>,
    font_size: f64,
    hscale: f64,
    leading: f64,
    rise: f64,
    render: i64,
}

impl GState {
    fn new(ctm: Affine) -> Self {
        let black = Source::Color(Rc::new(ColorSpace::Gray), vec![0.0]);
        GState {
            ctm,
            fill: black.clone(),
            stroke: black,
            line_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_phase: 0.0,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            blend: BlendMode::Normal,
            clips: Vec::new(),
            font: None,
            font_size: 0.0,
            hscale: 1.0,
            leading: 0.0,
            rise: 0.0,
            render: 0,
        }
    }
}

/// Text being collected between BT and ET. Illustrator writes several text
/// frames into one text object and every glyph of path text with its own
/// `Tm`, so shows are split into items by where they land.
struct TextRun {
    tm: Affine,
    tlm: Affine,
    items: Vec<TextItem>,
}

struct TextItem {
    text: AiText,
    clips: Vec<Rc<Clip>>,
    layer: Vec<u32>,
    /// Origin of the current line (artboard space).
    line_start: Point,
    /// Estimated end of the last show.
    pen: Point,
    /// Size of the last show.
    last_size: f64,
}

const MAX_FORM_DEPTH: usize = 24;
const MAX_OPERANDS: usize = 4096;

/// Interprets content streams of one page.
pub(crate) struct Interp<'a, 'f> {
    pub pdf: &'a PdfFile<'f>,
    /// Page space → artboard space (flip y, move the crop box to the origin).
    pub to_board: Affine,
    /// The artboard in artboard space.
    pub page: Rect,
    pub warnings: &'a mut Warnings,
    fonts: HashMap<u32, Rc<Font>>,
    pub images: usize,
}

impl<'a, 'f> Interp<'a, 'f> {
    pub fn new(
        pdf: &'a PdfFile<'f>,
        to_board: Affine,
        page: Rect,
        warnings: &'a mut Warnings,
    ) -> Self {
        Self {
            pdf,
            to_board,
            page,
            warnings,
            fonts: HashMap::new(),
            images: 0,
        }
    }

    /// Runs a page's content.
    pub fn run_page(&mut self, content: &[u8], resources: &Dict) -> Vec<Drawn> {
        let mut out = Vec::new();
        let mut gs = GState::new(Affine::IDENTITY);
        self.run(
            content,
            resources,
            &mut gs,
            Affine::IDENTITY,
            &[],
            &mut out,
            0,
        );
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        content: &[u8],
        resources: &Dict,
        gs: &mut GState,
        base_ctm: Affine,
        layer_prefix: &[u32],
        out: &mut Vec<Drawn>,
        depth: usize,
    ) {
        let mut stack: Vec<GState> = Vec::new();
        let mut path = BezPath::new();
        let mut current = Point::ZERO;
        let mut pending_clip: Option<FillRule> = None;
        let mut mc: Vec<Option<u32>> = Vec::new();
        let mut text: Option<TextRun> = None;
        let mut lx = Lexer::new(content, 0);
        let mut ops: Vec<Obj> = Vec::new();

        while let Some(tok) = lx.token() {
            let op = match tok {
                Token::Keyword(k) if !matches!(k.as_slice(), b"true" | b"false" | b"null") => k,
                other => {
                    if ops.len() < MAX_OPERANDS
                        && let Some(o) = lx.object_from(other, 0)
                    {
                        ops.push(o);
                    }
                    continue;
                }
            };
            let n = |i: usize| ops.get(i).and_then(Obj::as_f64).unwrap_or(0.0);
            let layer = || {
                let mut l = layer_prefix.to_vec();
                l.extend(mc.iter().flatten().copied());
                l
            };
            match op.as_slice() {
                // ---- graphics state
                b"q" => {
                    if stack.len() < 256 {
                        stack.push(gs.clone());
                    }
                }
                b"Q" => {
                    if let Some(s) = stack.pop() {
                        // Text objects never span Q in valid files; keep fonts.
                        *gs = s;
                    }
                }
                b"cm" if ops.len() >= 6 => {
                    let m = Affine::new([n(0), n(1), n(2), n(3), n(4), n(5)]);
                    gs.ctm *= m;
                }
                b"w" => gs.line_width = n(0).max(0.0),
                b"J" => {
                    gs.cap = match n(0) as i64 {
                        1 => LineCap::Round,
                        2 => LineCap::Projecting,
                        _ => LineCap::Butt,
                    }
                }
                b"j" => {
                    gs.join = match n(0) as i64 {
                        1 => LineJoin::Round,
                        2 => LineJoin::Bevel,
                        _ => LineJoin::Miter,
                    }
                }
                b"M" => gs.miter = n(0).max(1.0),
                b"d" => {
                    gs.dash = ops
                        .first()
                        .and_then(Obj::as_array)
                        .map(|a| a.iter().filter_map(Obj::as_f64).collect())
                        .unwrap_or_default();
                    gs.dash_phase = n(1);
                }
                b"gs" => {
                    if let Some(name) = ops.first().and_then(Obj::as_name) {
                        let egs = self.resource(resources, b"ExtGState", name);
                        if let Some(d) = egs.as_dict() {
                            self.ext_gstate(d, gs);
                        }
                    }
                }
                // ---- path construction
                b"m" => {
                    current = Point::new(n(0), n(1));
                    path.move_to(current);
                }
                b"l" => {
                    current = Point::new(n(0), n(1));
                    ensure_start(&mut path, current);
                    path.line_to(current);
                }
                b"c" => {
                    let p = Point::new(n(4), n(5));
                    ensure_start(&mut path, current);
                    path.curve_to(Point::new(n(0), n(1)), Point::new(n(2), n(3)), p);
                    current = p;
                }
                b"v" => {
                    let p = Point::new(n(2), n(3));
                    ensure_start(&mut path, current);
                    path.curve_to(current, Point::new(n(0), n(1)), p);
                    current = p;
                }
                b"y" => {
                    let p = Point::new(n(2), n(3));
                    ensure_start(&mut path, current);
                    path.curve_to(Point::new(n(0), n(1)), p, p);
                    current = p;
                }
                b"h" => path.close_path(),
                b"re" => {
                    let (x, y, w, h) = (n(0), n(1), n(2), n(3));
                    path.move_to((x, y));
                    path.line_to((x + w, y));
                    path.line_to((x + w, y + h));
                    path.line_to((x, y + h));
                    path.close_path();
                    current = Point::new(x, y);
                }
                // ---- painting
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n" => {
                    if matches!(op.as_slice(), b"s" | b"b" | b"b*") {
                        path.close_path();
                    }
                    let fill = match op.as_slice() {
                        b"f" | b"F" | b"B" | b"b" => Some(FillRule::NonZero),
                        b"f*" | b"B*" | b"b*" => Some(FillRule::EvenOdd),
                        _ => None,
                    };
                    let stroke = matches!(op.as_slice(), b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*");
                    let board = self.to_board * gs.ctm;
                    let shape = board * &path;
                    if fill.is_some() || stroke {
                        if let Some(run) = text.take() {
                            self.finish_text(run, out);
                        }
                        self.paint_path(&shape, fill, stroke, gs, base_ctm, layer(), out);
                    }
                    if let Some(rule) = pending_clip.take() {
                        gs.clips.push(Rc::new(make_clip(&shape, rule)));
                    }
                    path = BezPath::new();
                }
                b"W" => pending_clip = Some(FillRule::NonZero),
                b"W*" => pending_clip = Some(FillRule::EvenOdd),
                // ---- colour
                b"CS" | b"cs" => {
                    let cs = match ops.first() {
                        Some(Obj::Name(nm)) => self.color_space(resources, nm),
                        _ => ColorSpace::Gray,
                    };
                    let src = if matches!(cs, ColorSpace::Pattern) {
                        Source::Pattern(Obj::Null)
                    } else {
                        let init = cs.initial();
                        Source::Color(Rc::new(cs), init)
                    };
                    if op == b"CS" {
                        gs.stroke = src;
                    } else {
                        gs.fill = src;
                    }
                }
                b"SC" | b"SCN" | b"sc" | b"scn" => {
                    let target = if op[0] == b'S' {
                        &mut gs.stroke
                    } else {
                        &mut gs.fill
                    };
                    let comps: Vec<f64> = ops.iter().filter_map(Obj::as_f64).collect();
                    match target {
                        Source::Pattern(p) => {
                            if let Some(Obj::Name(name)) = ops.last() {
                                *p = self.resource(resources, b"Pattern", name);
                            }
                        }
                        Source::Color(_, c) => *c = comps,
                    }
                }
                b"G" | b"g" => set_color(gs, op[0] == b'G', ColorSpace::Gray, vec![n(0)]),
                b"RG" | b"rg" => {
                    set_color(gs, op[0] == b'R', ColorSpace::Rgb, vec![n(0), n(1), n(2)])
                }
                b"K" | b"k" => set_color(
                    gs,
                    op[0] == b'K',
                    ColorSpace::Cmyk,
                    vec![n(0), n(1), n(2), n(3)],
                ),
                b"sh" => {
                    if let Some(name) = ops.first().and_then(Obj::as_name) {
                        let sh = self.resource(resources, b"Shading", name);
                        if let Some(run) = text.take() {
                            self.finish_text(run, out);
                        }
                        self.paint_shading(&sh, gs, layer(), out);
                    }
                }
                // ---- XObjects
                b"Do" => {
                    if let Some(name) = ops.first().and_then(Obj::as_name) {
                        let xo = self.resource(resources, b"XObject", name);
                        if let Some(run) = text.take() {
                            self.finish_text(run, out);
                        }
                        self.do_xobject(&xo, resources, gs, &layer(), out, depth);
                    }
                }
                b"BI" => {
                    skip_inline_image(&mut lx);
                    self.images += 1;
                }
                // ---- marked content
                b"BDC" => {
                    let id = if ops.first().and_then(Obj::as_name) == Some(b"OC") {
                        match ops.get(1) {
                            Some(Obj::Name(p)) => self.ocg_id(resources, p),
                            _ => Some(u32::MAX),
                        }
                    } else {
                        None
                    };
                    mc.push(id);
                }
                b"BMC" => mc.push(None),
                b"EMC" => {
                    mc.pop();
                }
                // ---- text
                // Text items stay open across text objects (Illustrator may
                // write one frame's paragraphs in separate BT…ET) until
                // something else is drawn.
                b"BT" => match text.as_mut() {
                    Some(t) => {
                        t.tm = Affine::IDENTITY;
                        t.tlm = Affine::IDENTITY;
                    }
                    None => {
                        text = Some(TextRun {
                            tm: Affine::IDENTITY,
                            tlm: Affine::IDENTITY,
                            items: Vec::new(),
                        });
                    }
                },
                b"ET" => {}
                b"Tf" => {
                    if let Some(name) = ops.first().and_then(Obj::as_name) {
                        gs.font = Some(self.font(resources, name));
                    }
                    gs.font_size = n(1);
                }
                b"Tz" => gs.hscale = n(0) / 100.0,
                b"TL" => gs.leading = n(0),
                b"Ts" => gs.rise = n(0),
                b"Tr" => gs.render = n(0) as i64,
                b"Tc" | b"Tw" => {}
                b"Td" | b"TD" => {
                    if op == b"TD" {
                        gs.leading = -n(1);
                    }
                    if let Some(t) = text.as_mut() {
                        t.tlm *= Affine::translate((n(0), n(1)));
                        t.tm = t.tlm;
                    }
                }
                b"Tm" if ops.len() >= 6 => {
                    if let Some(t) = text.as_mut() {
                        t.tlm = Affine::new([n(0), n(1), n(2), n(3), n(4), n(5)]);
                        t.tm = t.tlm;
                    }
                }
                b"T*" => {
                    if let Some(t) = text.as_mut() {
                        t.tlm *= Affine::translate((0.0, -gs.leading));
                        t.tm = t.tlm;
                    }
                }
                b"Tj" | b"'" | b"\"" | b"TJ" => {
                    if matches!(op.as_slice(), b"'" | b"\"")
                        && let Some(t) = text.as_mut()
                    {
                        t.tlm *= Affine::translate((0.0, -gs.leading));
                        t.tm = t.tlm;
                    }
                    let bytes: Vec<u8> = match ops.last() {
                        Some(Obj::Str(s)) => s.clone(),
                        Some(Obj::Array(a)) => a
                            .iter()
                            .filter_map(|o| match o {
                                Obj::Str(s) => Some(s.as_slice()),
                                _ => None,
                            })
                            .flatten()
                            .copied()
                            .collect(),
                        _ => Vec::new(),
                    };
                    if let Some(t) = text.as_mut() {
                        // Layer and clips are taken per show: Illustrator
                        // opens BT before the layer's BDC.
                        self.show_text(t, gs, &bytes, layer());
                    }
                }
                _ => {}
            }
            ops.clear();
        }
        if let Some(run) = text.take() {
            self.finish_text(run, out);
        }
    }

    fn resource(&self, resources: &Dict, category: &[u8], name: &[u8]) -> Obj {
        match self.pdf.dict_get(resources, category) {
            Obj::Dict(d) => self.pdf.dict_get(&d, name),
            _ => Obj::Null,
        }
    }

    fn ocg_id(&self, resources: &Dict, name: &[u8]) -> Option<u32> {
        let props = self.pdf.dict_get(resources, b"Properties");
        let raw = props.as_dict()?.get(name)?.clone();
        let (id, dict) = match &raw {
            Obj::Ref(n, _) => (*n, self.pdf.get(*n)),
            other => (u32::MAX, other.clone()),
        };
        let d = dict.as_dict()?;
        // An OCMD: use its first OCG.
        if d.get(b"Type").and_then(Obj::as_name) == Some(b"OCMD") {
            return match d.get(b"OCGs") {
                Some(Obj::Ref(n, _)) => Some(*n),
                Some(Obj::Array(a)) => a.iter().find_map(|o| match o {
                    Obj::Ref(n, _) => Some(*n),
                    _ => None,
                }),
                _ => Some(id),
            };
        }
        Some(id)
    }

    fn color_space(&mut self, resources: &Dict, name: &[u8]) -> ColorSpace {
        match name {
            b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Pattern" => {
                ColorSpace::parse(self.pdf, &Obj::Name(name.to_vec()), &mut self.warnings.0)
            }
            _ => {
                let cs = self.resource(resources, b"ColorSpace", name);
                ColorSpace::parse(self.pdf, &cs, &mut self.warnings.0)
            }
        }
    }

    fn font(&mut self, resources: &Dict, name: &[u8]) -> Rc<Font> {
        let raw = match self.pdf.dict_get(resources, b"Font") {
            Obj::Dict(d) => d.get(name).cloned().unwrap_or(Obj::Null),
            _ => Obj::Null,
        };
        if let Obj::Ref(n, _) = raw
            && let Some(f) = self.fonts.get(&n)
        {
            return f.clone();
        }
        let font = Rc::new(Font::load(self.pdf, &raw));
        if let Obj::Ref(n, _) = raw {
            self.fonts.insert(n, font.clone());
        }
        font
    }

    fn ext_gstate(&mut self, d: &Dict, gs: &mut GState) {
        let pdf = self.pdf;
        if let Some(v) = pdf.dict_get(d, b"LW").as_f64() {
            gs.line_width = v.max(0.0);
        }
        if let Some(v) = pdf.dict_get(d, b"LC").as_i64() {
            gs.cap = match v {
                1 => LineCap::Round,
                2 => LineCap::Projecting,
                _ => LineCap::Butt,
            };
        }
        if let Some(v) = pdf.dict_get(d, b"LJ").as_i64() {
            gs.join = match v {
                1 => LineJoin::Round,
                2 => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
        }
        if let Some(v) = pdf.dict_get(d, b"ML").as_f64() {
            gs.miter = v.max(1.0);
        }
        // Illustrator writes 90 % as 0.899994.
        if let Some(v) = pdf.dict_get(d, b"CA").as_f64() {
            gs.stroke_alpha = round3(v.clamp(0.0, 1.0));
        }
        if let Some(v) = pdf.dict_get(d, b"ca").as_f64() {
            gs.fill_alpha = round3(v.clamp(0.0, 1.0));
        }
        match pdf.dict_get(d, b"BM") {
            Obj::Name(n) => gs.blend = self.blend(&n),
            Obj::Array(a) => {
                if let Some(n) = a.first().and_then(Obj::as_name) {
                    gs.blend = self.blend(n);
                }
            }
            _ => {}
        }
        match pdf.dict_get(d, b"SMask") {
            Obj::Null | Obj::Name(_) => {}
            _ => self
                .warnings
                .push("opacity masks and gradient opacity are not imported (shown fully opaque)"),
        }
    }

    fn blend(&mut self, name: &[u8]) -> BlendMode {
        match name {
            b"Normal" | b"Compatible" => BlendMode::Normal,
            b"Multiply" => BlendMode::Multiply,
            b"Screen" => BlendMode::Screen,
            b"Overlay" => BlendMode::Overlay,
            b"Darken" => BlendMode::Darken,
            b"Lighten" => BlendMode::Lighten,
            b"ColorDodge" => BlendMode::ColorDodge,
            b"ColorBurn" => BlendMode::ColorBurn,
            b"HardLight" => BlendMode::HardLight,
            b"SoftLight" => BlendMode::SoftLight,
            b"Difference" => BlendMode::Difference,
            b"Exclusion" => BlendMode::Exclusion,
            b"Hue" => BlendMode::Hue,
            b"Saturation" => BlendMode::Saturation,
            b"Color" => BlendMode::Color,
            b"Luminosity" => BlendMode::Luminosity,
            other => {
                self.warnings.push(format!(
                    "blend mode {} is not supported; using normal",
                    String::from_utf8_lossy(other)
                ));
                BlendMode::Normal
            }
        }
    }

    /// The brush of a fill or stroke source; `None` when nothing is painted.
    fn brush(&mut self, src: &Source, base_ctm: Affine) -> Brush {
        match src {
            Source::Color(cs, c) => Brush::Solid(rgba(cs.to_rgb(c))),
            Source::Pattern(p) => {
                let pat = self.pdf.resolve(p);
                let Some(d) = pat.as_dict() else {
                    return Brush::Solid([0.5, 0.5, 0.5, 1.0]);
                };
                if self.pdf.dict_get(d, b"PatternType").as_i64() == Some(2) {
                    let m = self.pdf.dict_get(d, b"Matrix");
                    let pm = matrix(&m).unwrap_or(Affine::IDENTITY);
                    let sh = self.pdf.dict_get(d, b"Shading");
                    if let Some(b) = self.shading_brush(&sh, self.to_board * base_ctm * pm) {
                        return b;
                    }
                } else {
                    self.warnings
                        .push("pattern fills are not imported yet (shown grey)");
                }
                Brush::Solid([0.5, 0.5, 0.5, 1.0])
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_path(
        &mut self,
        shape: &BezPath,
        fill: Option<FillRule>,
        stroke: bool,
        gs: &GState,
        base_ctm: Affine,
        layer: Vec<u32>,
        out: &mut Vec<Drawn>,
    ) {
        let mut subpaths: Vec<BezierPath> = from_bez_path(shape)
            .into_iter()
            .filter(|p| p.vertices.len() >= 2)
            .collect();
        if subpaths.is_empty() {
            return;
        }
        if !stroke {
            // Fills close sub-paths implicitly; Illustrator often omits `h`.
            subpaths.iter_mut().for_each(close_if_coincident);
        }
        let mut paints = Vec::new();
        let mut outset = 0.0;
        if stroke {
            let lin = (self.to_board * gs.ctm).as_coeffs();
            let (sx, sy) = (lin[0].hypot(lin[1]), lin[2].hypot(lin[3]));
            let det = (lin[0] * lin[3] - lin[1] * lin[2]).abs();
            let scale = det.sqrt();
            if sx > 0.0 && sy > 0.0 && (sx / sy - 1.0).abs() > 0.01 {
                self.warnings.push(
                    "strokes under non-uniform scaling are approximated with a uniform width",
                );
            }
            let style = StrokeStyle {
                width: gs.line_width * scale,
                cap: gs.cap,
                join: gs.join,
                miter_limit: gs.miter,
                dashes: gs.dash.iter().map(|d| d * scale).collect(),
                dash_offset: gs.dash_phase * scale,
            };
            outset = style.outset();
            paints.push(Paint {
                kind: PaintKind::Stroke(style),
                brush: self.brush(&gs.stroke.clone(), base_ctm),
                opacity: gs.stroke_alpha,
                blend: BlendMode::Normal,
                composite: Composite::Below,
            });
        }
        if let Some(rule) = fill {
            paints.push(Paint {
                kind: PaintKind::Fill(rule),
                brush: self.brush(&gs.fill.clone(), base_ctm),
                opacity: gs.fill_alpha,
                blend: BlendMode::Normal,
                composite: Composite::Below,
            });
        }
        let bounds = shape.bounding_box().inflate(outset, outset);
        let mut item = AiPath::new(subpaths);
        item.paints = paints;
        item.blend = gs.blend;
        out.push(Drawn {
            clips: self.effective_clips(&gs.clips, bounds),
            kind: DrawnKind::Path(item),
            layer,
            bounds,
        });
    }

    /// Clips that actually cut `bounds` (a rectangle that contains the item or
    /// the whole artboard does not).
    fn effective_clips(&self, clips: &[Rc<Clip>], bounds: Rect) -> Vec<Rc<Clip>> {
        let page = self.page.inflate(0.5, 0.5);
        clips
            .iter()
            .filter(|c| match c.rect {
                Some(r) => {
                    let r = r.inflate(0.5, 0.5);
                    !(contains(r, bounds) || contains(r, page))
                }
                None => true,
            })
            .cloned()
            .collect()
    }

    /// `sh`: the shading fills the current clip (Illustrator's gradient fills
    /// are "clip to the path, then paint the shading").
    fn paint_shading(&mut self, sh: &Obj, gs: &GState, layer: Vec<u32>, out: &mut Vec<Drawn>) {
        let Some(brush) = self.shading_brush(sh, self.to_board * gs.ctm) else {
            return;
        };
        let (shape, rule, outer) = match gs.clips.split_last() {
            Some((c, rest)) => (c.paths.clone(), c.rule, rest.to_vec()),
            None => {
                let p = self.page;
                (
                    from_bez_path(&p.to_path(0.1)),
                    FillRule::NonZero,
                    Vec::new(),
                )
            }
        };
        if shape.is_empty() {
            return;
        }
        let bounds = shape
            .iter()
            .flat_map(|p| p.vertices.iter())
            .fold(Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN), |r, v| {
                r.union_pt(Point::new(v[0], v[1]))
            });
        let mut item = AiPath::new(shape);
        item.paints.push(Paint {
            kind: PaintKind::Fill(rule),
            brush,
            opacity: gs.fill_alpha,
            blend: BlendMode::Normal,
            composite: Composite::Below,
        });
        item.blend = gs.blend;
        out.push(Drawn {
            clips: self.effective_clips(&outer, bounds),
            kind: DrawnKind::Path(item),
            layer,
            bounds,
        });
    }

    /// An axial or radial shading as a gradient brush; `m` maps shading
    /// space to artboard space.
    fn shading_brush(&mut self, sh: &Obj, m: Affine) -> Option<Brush> {
        let sh = self.pdf.resolve(sh);
        let d = sh.as_dict()?.clone();
        let pdf = self.pdf;
        let kind = pdf.dict_get(&d, b"ShadingType").as_i64().unwrap_or(0);
        if !matches!(kind, 2 | 3) {
            self.warnings.push(format!(
                "shading type {kind} (mesh or freeform gradient) is not imported; using its first colour"
            ));
        }
        let cs = ColorSpace::parse(pdf, &pdf.dict_get(&d, b"ColorSpace"), &mut self.warnings.0);
        let func = Function::parse(pdf, &pdf.dict_get(&d, b"Function"), &mut self.warnings.0);
        let coords: Vec<f64> = match pdf.dict_get(&d, b"Coords") {
            Obj::Array(a) => a.iter().filter_map(Obj::as_f64).collect(),
            _ => Vec::new(),
        };
        let domain: Vec<f64> = match pdf.dict_get(&d, b"Domain") {
            Obj::Array(a) => a.iter().filter_map(Obj::as_f64).collect(),
            _ => vec![0.0, 1.0],
        };
        let (t0, t1) = match domain.as_slice() {
            [a, b, ..] => (*a, *b),
            _ => (0.0, 1.0),
        };
        let Some(func) = func.filter(|_| matches!(kind, 2 | 3)) else {
            return Some(Brush::Solid(rgba(cs.to_rgb(&cs.initial()))));
        };
        let color_at = |t: f64| -> [f32; 4] { rgba(cs.to_rgb(&func.eval(&[t]))) };
        // Stops at the function's breakpoints (inputs in the shading domain).
        let mut ts: Vec<f64> = func
            .breakpoints()
            .into_iter()
            .filter(|t| t.is_finite())
            .map(|t| t.clamp(t0.min(t1), t0.max(t1)))
            .collect();
        ts.extend([t0, t1]);
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        if ts.len() > 64 {
            ts = (0..64)
                .map(|i| t0 + (t1 - t0) * f64::from(i) / 63.0)
                .collect();
        }
        let span = if (t1 - t0).abs() < 1e-12 {
            1.0
        } else {
            t1 - t0
        };
        let mut stops: Vec<GradientStop> = ts
            .iter()
            .map(|&t| GradientStop {
                offset: ((t - t0) / span).clamp(0.0, 1.0) as f32,
                color: color_at(t),
            })
            .collect();
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));

        let lin = m.as_coeffs();
        let (sx, sy) = (lin[0].hypot(lin[1]), lin[2].hypot(lin[3]));
        if kind == 2 {
            let [x0, y0, x1, y1] = first4(&coords)?;
            return Some(Brush::Gradient(Gradient {
                kind: GradientKind::Linear,
                start: m * Point::new(x0, y0),
                end: m * Point::new(x1, y1),
                stops,
            }));
        }
        let (x0, y0, r0, x1, y1, r1) = match coords.as_slice() {
            [a, b, c, d, e, f, ..] => (*a, *b, *c, *d, *e, *f),
            _ => return None,
        };
        if sx > 0.0 && sy > 0.0 && (sx / sy - 1.0).abs() > 0.01 {
            self.warnings
                .push("elliptical radial gradients are imported as circular ones");
        }
        // AE: start = centre, end = a point on the outer circle, highlight =
        // focal point offset.
        let (fx, fy, fr, cx, cy, cr, reversed) = if r1 >= r0 {
            (x0, y0, r0, x1, y1, r1, false)
        } else {
            (x1, y1, r1, x0, y0, r0, true)
        };
        if cr <= 0.0 {
            return Some(Brush::Solid(
                stops.last().map_or([0.0, 0.0, 0.0, 1.0], |s| s.color),
            ));
        }
        if reversed {
            for s in &mut stops {
                s.offset = 1.0 - s.offset;
            }
            stops.reverse();
        }
        if fr > 0.0 {
            // The ramp starts at the inner circle: compress it outward and pad.
            let k = (fr / cr) as f32;
            for s in &mut stops {
                s.offset = k + s.offset * (1.0 - k);
            }
            if let Some(first) = stops.first().copied() {
                stops.insert(
                    0,
                    GradientStop {
                        offset: 0.0,
                        color: first.color,
                    },
                );
            }
        }
        let start = m * Point::new(cx, cy);
        let end = m * Point::new(cx + cr, cy);
        let focal = m * Point::new(fx, fy);
        let radius = (end - start).hypot();
        let off = focal - start;
        let (highlight_length, highlight_angle) = if radius > 0.0 && off.hypot() > 1e-6 {
            let len = (off.hypot() / radius * 100.0).min(99.0);
            let dir = (end - start).atan2().to_degrees();
            let ang = off.atan2().to_degrees() - dir;
            (round3(len), round3(normalize_deg(ang)))
        } else {
            (0.0, 0.0)
        };
        Some(Brush::Gradient(Gradient {
            kind: GradientKind::Radial {
                highlight_length,
                highlight_angle,
            },
            start,
            end,
            stops,
        }))
    }

    fn do_xobject(
        &mut self,
        xo: &Obj,
        parent_resources: &Dict,
        gs: &GState,
        layer: &[u32],
        out: &mut Vec<Drawn>,
        depth: usize,
    ) {
        let Some(d) = xo.as_dict().cloned() else {
            return;
        };
        match self.pdf.dict_get(&d, b"Subtype").as_name() {
            Some(b"Form") => {
                if depth >= MAX_FORM_DEPTH {
                    self.warnings.push("form XObjects nested too deeply");
                    return;
                }
                let Ok(content) = self.pdf.stream_data(xo) else {
                    self.warnings.push("unreadable form XObject skipped");
                    return;
                };
                let resources = match self.pdf.dict_get(&d, b"Resources") {
                    Obj::Dict(r) => r,
                    _ => parent_resources.clone(),
                };
                let fm = matrix(&self.pdf.dict_get(&d, b"Matrix")).unwrap_or(Affine::IDENTITY);
                let ctm = gs.ctm * fm;
                let is_group =
                    self.pdf.dict_get(&d, b"Group").as_dict().is_some_and(|g| {
                        self.pdf.dict_get(g, b"S").as_name() == Some(b"Transparency")
                    });
                let isolated = is_group && (gs.fill_alpha < 1.0 || gs.blend != BlendMode::Normal);
                if isolated {
                    let mut inner = GState::new(ctm);
                    inner.font = gs.font.clone();
                    let mut items = Vec::new();
                    self.run(
                        &content,
                        &resources,
                        &mut inner,
                        ctm,
                        layer,
                        &mut items,
                        depth + 1,
                    );
                    if items.is_empty() {
                        return;
                    }
                    let bounds = items
                        .iter()
                        .map(|i| i.bounds)
                        .reduce(|a, b| a.union(b))
                        .unwrap_or_default();
                    out.push(Drawn {
                        clips: self.effective_clips(&gs.clips, bounds),
                        kind: DrawnKind::Group {
                            opacity: gs.fill_alpha,
                            blend: gs.blend,
                            items,
                        },
                        layer: layer.to_vec(),
                        bounds,
                    });
                } else {
                    let mut inner = gs.clone();
                    inner.ctm = ctm;
                    self.run(&content, &resources, &mut inner, ctm, layer, out, depth + 1);
                }
            }
            Some(b"Image") => self.images += 1,
            _ => {}
        }
    }

    fn show_text(&self, t: &mut TextRun, gs: &GState, bytes: &[u8], layer: Vec<u32>) {
        let Some(font) = gs.font.clone() else { return };
        let s = font.decode(bytes);
        if s.is_empty() || matches!(gs.render, 3 | 7) {
            return;
        }
        let trm = self.to_board * gs.ctm * t.tm * Affine::translate((0.0, gs.rise));
        let c = trm.as_coeffs();
        let origin = trm * Point::ZERO;
        let det = (c[0] * c[3] - c[1] * c[2]).abs();
        let size = gs.font_size.abs() * det.sqrt();
        let sx = c[0].hypot(c[1]);
        let sy = c[2].hypot(c[3]);
        let dir = if sx > 0.0 {
            bae_vector::kurbo::Vec2::new(c[0] / sx, c[1] / sx)
        } else {
            bae_vector::kurbo::Vec2::new(1.0, 0.0)
        };
        let rotation = round3(c[1].atan2(c[0]).to_degrees());
        let hscale = if sy > 0.0 { sx / sy * gs.hscale } else { 1.0 };
        let chars = s.chars().count() as f64;
        let advance = 0.5 * size * hscale.max(0.1) * chars;
        // Advance the text matrix roughly so a later Tj without Tm does not
        // look like a new frame; exact widths are not needed for a placeholder.
        if det > 0.0 {
            t.tm *= Affine::translate((advance / det.sqrt(), 0.0));
        }

        if let Some(cur) = t.items.last_mut()
            && cur.layer == layer
        {
            let scale = cur.last_size.max(size).max(1.0);
            let rot_same = (normalize_deg(rotation - cur.text.rotation)).abs() < 0.5;
            let v = origin - cur.line_start;
            let normal = bae_vector::kurbo::Vec2::new(-dir.y, dir.x);
            let down = v.dot(normal);
            let along = v.dot(dir);
            if (origin - cur.pen).hypot() < scale {
                // Same line (or the next glyph of path text).
                cur.text.text.push_str(&s);
                cur.pen = origin + dir * advance;
                cur.last_size = size;
                return;
            }
            if rot_same && down > 0.5 * scale && down < 3.0 * scale && along.abs() < 40.0 * scale {
                cur.text.text.push('\r');
                cur.text.text.push_str(&s);
                cur.line_start = origin;
                cur.pen = origin + dir * advance;
                cur.last_size = size;
                return;
            }
        }
        let color = |src: &Source| match src {
            Source::Color(cs, v) => {
                let [r, g, b] = cs.to_rgb(v);
                Some([round3(r), round3(g), round3(b), 1.0])
            }
            Source::Pattern(_) => Some([0.5, 0.5, 0.5, 1.0]),
        };
        let mut at = AiText::new(s, font.name.clone(), round3(size));
        at.position = [round3(origin.x), round3(origin.y)];
        at.rotation = rotation;
        if (hscale - 1.0).abs() > 0.005 {
            at.scale = [round3(hscale * 100.0), 100.0];
        }
        at.fill = match gs.render {
            1 | 5 => None,
            _ => color(&gs.fill),
        };
        if matches!(gs.render, 1 | 2 | 5 | 6) {
            at.stroke = color(&gs.stroke).map(|c| (c, round3(gs.line_width * det.sqrt())));
        }
        t.items.push(TextItem {
            text: at,
            clips: gs.clips.clone(),
            layer,
            line_start: origin,
            pen: origin + dir * advance,
            last_size: size,
        });
    }

    fn finish_text(&mut self, run: TextRun, out: &mut Vec<Drawn>) {
        for item in run.items {
            let at = item.text;
            if at.text.contains('\u{FFFD}') {
                self.warnings.push(
                    "some text could not be decoded from the PDF (fonts without Unicode data); use the Illustrator bridge export for exact text",
                );
            }
            let p = Point::new(at.position[0], at.position[1]);
            let bounds = Rect::from_points(p, item.pen).inflate(at.size, at.size);
            out.push(Drawn {
                clips: self.effective_clips(&item.clips, bounds),
                kind: DrawnKind::Text(at),
                layer: item.layer,
                bounds,
            });
        }
    }
}

fn ensure_start(path: &mut BezPath, current: Point) {
    if path.elements().is_empty() {
        path.move_to(current);
    }
}

fn set_color(gs: &mut GState, stroke: bool, cs: ColorSpace, comps: Vec<f64>) {
    let src = Source::Color(Rc::new(cs), comps);
    if stroke {
        gs.stroke = src;
    } else {
        gs.fill = src;
    }
}

fn rgba(c: [f64; 3]) -> [f32; 4] {
    [c[0] as f32, c[1] as f32, c[2] as f32, 1.0]
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn normalize_deg(mut a: f64) -> f64 {
    while a > 180.0 {
        a -= 360.0;
    }
    while a <= -180.0 {
        a += 360.0;
    }
    a
}

fn first4(v: &[f64]) -> Option<[f64; 4]> {
    match v {
        [a, b, c, d, ..] => Some([*a, *b, *c, *d]),
        _ => None,
    }
}

pub(crate) fn matrix(o: &Obj) -> Option<Affine> {
    let a: Vec<f64> = o.as_array()?.iter().filter_map(Obj::as_f64).collect();
    match a.as_slice() {
        [a, b, c, d, e, f] => Some(Affine::new([*a, *b, *c, *d, *e, *f])),
        _ => None,
    }
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x0 >= outer.x0 && inner.y0 >= outer.y0 && inner.x1 <= outer.x1 && inner.y1 <= outer.y1
}

/// Folds a duplicate end vertex into vertex 0 and closes the path (a filled
/// or clipping sub-path that returns to its start without `h`).
pub(crate) fn close_if_coincident(p: &mut BezierPath) {
    let n = p.vertices.len();
    if p.closed || n < 3 {
        return;
    }
    let (a, b) = (p.vertices[0], p.vertices[n - 1]);
    if (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 {
        p.vertices.pop();
        p.out_tangents.pop();
        if let Some(t) = p.in_tangents.pop() {
            p.in_tangents[0] = t;
        }
    }
    p.closed = true;
}

fn make_clip(shape: &BezPath, rule: FillRule) -> Clip {
    let mut paths: Vec<BezierPath> = from_bez_path(shape)
        .into_iter()
        .filter(|p| p.vertices.len() >= 2)
        .collect();
    paths.iter_mut().for_each(close_if_coincident);
    let rect = match paths.as_slice() {
        [p] if is_axis_rect(p) => {
            let xs = p.vertices.iter().map(|v| v[0]);
            let ys = p.vertices.iter().map(|v| v[1]);
            Some(Rect::new(
                xs.clone().fold(f64::MAX, f64::min),
                ys.clone().fold(f64::MAX, f64::min),
                xs.fold(f64::MIN, f64::max),
                ys.fold(f64::MIN, f64::max),
            ))
        }
        _ => None,
    };
    Clip { paths, rule, rect }
}

fn is_axis_rect(p: &BezierPath) -> bool {
    let n = p.vertices.len();
    if !(n == 4 || n == 5) {
        return false;
    }
    let flat = p
        .in_tangents
        .iter()
        .chain(&p.out_tangents)
        .all(|t| t[0] == 0.0 && t[1] == 0.0);
    flat && (0..n).all(|i| {
        let a = p.vertices[i];
        let b = p.vertices[(i + 1) % n];
        (a[0] - b[0]).abs() < 1e-6 || (a[1] - b[1]).abs() < 1e-6
    })
}

/// Skips `BI … ID <data> EI`.
fn skip_inline_image(lx: &mut Lexer<'_>) {
    // Dictionary up to ID.
    while let Some(t) = lx.token() {
        if t == Token::Keyword(b"ID".to_vec()) {
            break;
        }
    }
    let data = lx.data;
    let mut i = lx.pos + 1;
    while i + 2 < data.len() {
        if super::lexer::is_white(data[i - 1])
            && data[i] == b'E'
            && data[i + 1] == b'I'
            && (i + 2 == data.len() || super::lexer::is_white(data[i + 2]))
        {
            lx.pos = i + 2;
            return;
        }
        i += 1;
    }
    lx.pos = data.len();
}

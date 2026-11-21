//! Reader for the Illustrator bridge JSON: what `bridges/ai/bae-ai-export.jsx`
//! writes in Illustrator (format `bae-ai-bridge`, spec in `bridges/ai/FORMAT.md`).
//!
//! The bridge file is a faithful dump of Illustrator's scripting DOM in document
//! space (points, y down): artboards, then the layer tree with paths, compound
//! paths, groups, clip groups, text and expanded symbols. This module does the
//! mapping to [`AiDocument`]:
//!
//! - every artboard gets the top-level items (and sublayer items) whose bounds
//!   touch it, moved into artboard space; items outside every artboard are left
//!   out (Illustrator does not show them either);
//! - sublayers become named groups; layer opacity / blend wrap the layer's items
//!   in one group; non-printing (template) layers are imported hidden;
//! - Illustrator's appearance order (stroke above fill) becomes the paint order;
//!   gradient midpoints become an extra stop, radial highlights AE's
//!   highlight length / angle;
//! - point text keeps its first style run; area and path text become point text
//!   at the frame's corner, or outlines with [`BridgeOptions::text_outlines`].

use bae_model::BezierPath;
use bae_vector::{
    BlendMode, Brush, Composite, FillRule, Gradient, GradientKind, GradientStop, LineCap, LineJoin,
    Paint, PaintKind, StrokeStyle, paint::paint_color,
};
use kurbo::Point;
use serde::Deserialize;

use crate::Error;
use crate::doc::{AiDocument, AiGroup, AiItem, AiLayer, AiPath, AiText, Artboard, SourceFormat};

/// `format` of a bridge file.
pub const BRIDGE_FORMAT: &str = "bae-ai-bridge";
/// Major.minor versions this reader understands (0.x: minor bumps may break).
const SUPPORTED_VERSIONS: &[&str] = &["0.1"];

/// How a bridge file is turned into an [`AiDocument`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BridgeOptions {
    /// Import text as the outlines the export script made (exact glyph shapes,
    /// no longer editable as text) instead of text layers. Text without
    /// outlines stays text.
    pub text_outlines: bool,
}

/// Reads a bridge file with default options.
pub fn read_bridge(name: &str, bytes: &[u8]) -> Result<AiDocument, Error> {
    read_bridge_with(name, bytes, &BridgeOptions::default())
}

/// Reads a bridge file. `name` is used when the file does not name its document.
pub fn read_bridge_with(
    name: &str,
    bytes: &[u8],
    options: &BridgeOptions,
) -> Result<AiDocument, Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| Error::Malformed(format!("bridge file is not UTF-8: {e}")))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let file: FileDto =
        serde_json::from_str(text).map_err(|e| Error::Malformed(format!("bridge file: {e}")))?;
    if file.format != BRIDGE_FORMAT {
        return Err(Error::Malformed(format!(
            "format is {:?}, expected {BRIDGE_FORMAT:?}",
            file.format
        )));
    }
    let major_minor: String = file
        .format_version
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".");
    if !SUPPORTED_VERSIONS.contains(&major_minor.as_str()) {
        return Err(Error::Unsupported(format!(
            "bridge format version {} (this BAE reads {})",
            file.format_version,
            SUPPORTED_VERSIONS.join(", ")
        )));
    }
    Ok(Reader::new(options).document(name, file))
}

// ---------------------------------------------------------------- file model
// Mirrors bridges/ai/FORMAT.md. Unknown fields are ignored so newer exporters
// can add information without breaking this reader.

#[derive(Deserialize)]
struct FileDto {
    format: String,
    format_version: String,
    #[serde(default)]
    document: DocumentDto,
    #[serde(default)]
    artboards: Vec<ArtboardDto>,
    #[serde(default)]
    layers: Vec<ItemDto>,
    #[serde(default)]
    report: ReportDto,
}

#[derive(Deserialize, Default)]
struct DocumentDto {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ArtboardDto {
    #[serde(default)]
    name: String,
    /// `[left, top, right, bottom]`, document space, y down.
    rect: [f64; 4],
}

#[derive(Deserialize, Default)]
struct ReportDto {
    #[serde(default)]
    warnings: Vec<String>,
}

fn one() -> f64 {
    1.0
}
fn yes() -> bool {
    true
}
fn normal() -> String {
    "normal".to_owned()
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ItemDto {
    Layer(LayerDto),
    Group(GroupDto),
    Path(PathDto),
    Text(TextDto),
    Image(OtherDto),
    Other(OtherDto),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct LayerDto {
    #[serde(default)]
    name: String,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default)]
    locked: bool,
    #[serde(default = "yes")]
    printable: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default = "normal")]
    blend: String,
    #[serde(default)]
    items: Vec<ItemDto>,
}

#[derive(Deserialize)]
struct GroupDto {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default = "normal")]
    blend: String,
    #[serde(default)]
    bounds: Option<[f64; 4]>,
    #[serde(default)]
    clipped: bool,
    #[serde(default)]
    clip: Option<Vec<SubpathDto>>,
    #[serde(default)]
    items: Vec<ItemDto>,
}

#[derive(Deserialize)]
struct SubpathDto {
    #[serde(default)]
    closed: bool,
    /// `[x, y, in_x, in_y, out_x, out_y]`, absolute handles.
    points: Vec<[f64; 6]>,
}

#[derive(Deserialize)]
struct PathDto {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default = "normal")]
    blend: String,
    #[serde(default)]
    bounds: Option<[f64; 4]>,
    subpaths: Vec<SubpathDto>,
    #[serde(default)]
    fill_rule: Option<String>,
    #[serde(default)]
    fill: Option<PaintDto>,
    #[serde(default)]
    stroke: Option<StrokeDto>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PaintDto {
    Color {
        color: [f64; 4],
    },
    Gradient(GradientDto),
    Pattern {
        #[serde(default)]
        name: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
struct GradientDto {
    gradient: String,
    stops: Vec<StopDto>,
    start: [f64; 2],
    end: [f64; 2],
    #[serde(default)]
    focal: Option<[f64; 2]>,
    /// Gradient space → document, `[a, b, c, d, tx, ty]`, y down.
    #[serde(default)]
    matrix: Option<[f64; 6]>,
}

#[derive(Deserialize)]
struct StopDto {
    offset: f64,
    #[serde(default = "half")]
    midpoint: f64,
    color: [f64; 4],
}

fn half() -> f64 {
    0.5
}

#[derive(Deserialize)]
struct StrokeDto {
    paint: PaintDto,
    width: f64,
    #[serde(default)]
    cap: String,
    #[serde(default)]
    join: String,
    #[serde(default = "ten")]
    miter_limit: f64,
    #[serde(default)]
    dashes: Vec<f64>,
    #[serde(default)]
    dash_offset: f64,
}

fn ten() -> f64 {
    10.0
}

#[derive(Deserialize)]
struct TextDto {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hidden: bool,
    #[serde(default = "one")]
    opacity: f64,
    #[serde(default = "normal")]
    blend: String,
    #[serde(default)]
    bounds: Option<[f64; 4]>,
    #[serde(default)]
    text_kind: String,
    #[serde(default)]
    contents: String,
    #[serde(default)]
    anchor: Option<[f64; 2]>,
    /// Linear part `[a, b, c, d]`, y down.
    #[serde(default)]
    matrix: Option<[f64; 4]>,
    #[serde(default)]
    orientation: String,
    #[serde(default)]
    paragraphs: Vec<ParagraphDto>,
    #[serde(default)]
    runs: Vec<RunDto>,
    #[serde(default)]
    frame: Option<SubpathDto>,
    #[serde(default)]
    outline: Option<GroupDto>,
}

#[derive(Deserialize)]
struct ParagraphDto {
    #[serde(default)]
    justification: String,
}

#[derive(Deserialize)]
struct RunDto {
    #[serde(default)]
    font: String,
    #[serde(default)]
    size: f64,
    #[serde(default = "hundred")]
    h_scale: f64,
    #[serde(default = "hundred")]
    v_scale: f64,
    #[serde(default)]
    fill: Option<PaintDto>,
    #[serde(default)]
    stroke: Option<PaintDto>,
    #[serde(default)]
    stroke_width: Option<f64>,
}

fn hundred() -> f64 {
    100.0
}

#[derive(Deserialize)]
struct OtherDto {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    item_type: Option<String>,
    #[serde(default)]
    image_kind: Option<String>,
    #[serde(default)]
    bounds: Option<[f64; 4]>,
}

// ---------------------------------------------------------------- geometry

/// `[left, top, right, bottom]`, y down.
type Rect = [f64; 4];

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some([
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]),
        (a, None) => a,
        (None, b) => b,
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

fn subpath_bounds(subs: &[SubpathDto]) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    for s in subs {
        for p in &s.points {
            for k in 0..3 {
                let (x, y) = (p[2 * k], p[2 * k + 1]);
                r = union(r, Some([x, y, x, y]));
            }
        }
    }
    r
}

/// Where an item is, for assigning it to artboards. Illustrator's own bounds
/// of a clip group include the clipped-away artwork, so groups are measured
/// from their clip path or their children.
fn item_bounds(item: &ItemDto) -> Option<Rect> {
    match item {
        ItemDto::Layer(l) => l.items.iter().fold(None, |r, i| union(r, item_bounds(i))),
        ItemDto::Group(g) => group_bounds(g),
        ItemDto::Path(p) => p.bounds.or_else(|| subpath_bounds(&p.subpaths)),
        ItemDto::Text(t) => t
            .bounds
            .or_else(|| t.anchor.map(|a| [a[0], a[1], a[0], a[1]])),
        ItemDto::Image(o) | ItemDto::Other(o) => o.bounds,
        ItemDto::Unknown => None,
    }
}

fn group_bounds(g: &GroupDto) -> Option<Rect> {
    if g.clipped
        && let Some(clip) = &g.clip
        && !clip.is_empty()
    {
        return subpath_bounds(clip);
    }
    g.items
        .iter()
        .fold(None, |r, i| union(r, item_bounds(i)))
        .or(g.bounds)
}

/// Document space → artboard space.
#[derive(Clone, Copy)]
struct Offset([f64; 2]);

impl Offset {
    fn point(self, p: [f64; 2]) -> [f64; 2] {
        [p[0] - self.0[0], p[1] - self.0[1]]
    }

    fn kpoint(self, p: [f64; 2]) -> Point {
        let q = self.point(p);
        Point::new(q[0], q[1])
    }

    fn bezier(self, s: &SubpathDto) -> BezierPath {
        let n = s.points.len();
        let mut path = BezierPath {
            vertices: Vec::with_capacity(n),
            in_tangents: Vec::with_capacity(n),
            out_tangents: Vec::with_capacity(n),
            closed: s.closed,
            feather: Vec::new(),
        };
        for p in &s.points {
            path.vertices.push(self.point([p[0], p[1]]));
            path.in_tangents.push([p[2] - p[0], p[3] - p[1]]);
            path.out_tangents.push([p[4] - p[0], p[5] - p[1]]);
        }
        path
    }
}

// ---------------------------------------------------------------- mapping

struct Reader<'o> {
    options: &'o BridgeOptions,
    warnings: Vec<String>,
}

impl<'o> Reader<'o> {
    fn new(options: &'o BridgeOptions) -> Self {
        Self {
            options,
            warnings: Vec::new(),
        }
    }

    fn warn(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }

    fn document(mut self, fallback_name: &str, file: FileDto) -> AiDocument {
        let name = file
            .document
            .name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| fallback_name.to_owned());
        let rects: Vec<Rect> = file.artboards.iter().map(|a| a.rect).collect();
        // Items outside every artboard are not imported anywhere.
        let mut outside = 0usize;
        for layer in &file.layers {
            if let ItemDto::Layer(l) = layer {
                outside += count_outside(&l.items, &rects);
            }
        }
        if outside > 0 {
            self.warn(format!(
                "{outside} item(s) outside every artboard were left out"
            ));
        }
        let mut artboards = Vec::with_capacity(file.artboards.len());
        for ab in &file.artboards {
            let [l, t, r, b] = ab.rect;
            let offset = Offset([l, t]);
            let mut layers = Vec::new();
            for item in &file.layers {
                match item {
                    ItemDto::Layer(layer) => {
                        if let Some(l) = self.top_layer(layer, &ab.rect, offset) {
                            layers.push(l);
                        }
                    }
                    _ => self.warn("a top-level entry that is not a layer was ignored"),
                }
            }
            artboards.push(Artboard {
                name: ab.name.clone(),
                origin: [l, t],
                size: [r - l, b - t],
                layers,
            });
        }
        let mut warnings: Vec<String> = file
            .report
            .warnings
            .into_iter()
            .map(|w| format!("Illustrator export: {w}"))
            .collect();
        warnings.append(&mut self.warnings);
        AiDocument {
            name,
            source: SourceFormat::Bridge,
            artboards,
            warnings,
        }
    }

    fn top_layer(&mut self, layer: &LayerDto, rect: &Rect, offset: Offset) -> Option<AiLayer> {
        let mut items = self.items_on(&layer.items, rect, offset);
        if items.is_empty() {
            return None;
        }
        let blend = self.blend(&layer.blend);
        if layer.opacity < 1.0 || blend != BlendMode::Normal {
            let mut g = AiGroup::new(Some(layer.name.clone()));
            g.opacity = layer.opacity;
            g.blend = blend;
            g.items = items;
            items = vec![AiItem::Group(g)];
        }
        if !layer.printable && layer.visible {
            self.warn(format!(
                "non-printing layer \"{}\" imported hidden",
                layer.name
            ));
        }
        Some(AiLayer {
            name: layer.name.clone(),
            visible: layer.visible && layer.printable,
            locked: layer.locked,
            items,
        })
    }

    /// The items of a layer that touch the artboard (sublayers are filtered
    /// item by item), in artboard space.
    fn items_on(&mut self, items: &[ItemDto], rect: &Rect, offset: Offset) -> Vec<AiItem> {
        let mut out = Vec::new();
        for item in items {
            if let ItemDto::Layer(sub) = item {
                let children = self.items_on(&sub.items, rect, offset);
                if children.is_empty() {
                    continue;
                }
                let mut g = AiGroup::new(Some(sub.name.clone()));
                g.opacity = sub.opacity;
                g.blend = self.blend(&sub.blend);
                g.hidden = !sub.visible || !sub.printable;
                g.items = children;
                out.push(AiItem::Group(g));
                continue;
            }
            match item_bounds(item) {
                Some(b) if overlaps(&b, rect) => {}
                _ => continue,
            }
            if let Some(i) = self.item(item, offset) {
                out.push(i);
            }
        }
        out
    }

    fn item(&mut self, item: &ItemDto, offset: Offset) -> Option<AiItem> {
        match item {
            ItemDto::Layer(l) => {
                // Only reachable inside groups, which Illustrator does not allow.
                let mut g = AiGroup::new(Some(l.name.clone()));
                g.hidden = !l.visible;
                g.items = l
                    .items
                    .iter()
                    .filter_map(|i| self.item(i, offset))
                    .collect();
                Some(AiItem::Group(g))
            }
            ItemDto::Group(g) => Some(AiItem::Group(self.group(g, offset))),
            ItemDto::Path(p) => Some(AiItem::Path(self.path(p, offset))),
            ItemDto::Text(t) => Some(self.text(t, offset)),
            ItemDto::Image(o) => {
                self.warn(format!(
                    "{} image{} not imported yet",
                    o.image_kind.as_deref().unwrap_or("placed"),
                    o.name
                        .as_deref()
                        .map(|n| format!(" \"{n}\""))
                        .unwrap_or_default()
                ));
                None
            }
            ItemDto::Other(o) => {
                self.warn(format!(
                    "{} not imported (expand it in Illustrator first)",
                    o.item_type.as_deref().unwrap_or("item")
                ));
                None
            }
            ItemDto::Unknown => {
                self.warn("unknown item type skipped");
                None
            }
        }
    }

    fn group(&mut self, g: &GroupDto, offset: Offset) -> AiGroup {
        let mut out = AiGroup::new(g.name.clone());
        out.opacity = g.opacity;
        out.blend = self.blend(&g.blend);
        out.hidden = g.hidden;
        if g.clipped {
            match &g.clip {
                Some(clip) if !clip.is_empty() => {
                    out.clip = Some(clip.iter().map(|s| offset.bezier(s)).collect());
                }
                _ => self.warn("clip group without a clipping path imported unclipped"),
            }
        }
        out.items = g
            .items
            .iter()
            .filter_map(|i| self.item(i, offset))
            .collect();
        out
    }

    fn path(&mut self, p: &PathDto, offset: Offset) -> AiPath {
        let mut out = AiPath::new(p.subpaths.iter().map(|s| offset.bezier(s)).collect());
        out.name = p.name.clone();
        out.opacity = p.opacity;
        out.blend = self.blend(&p.blend);
        out.hidden = p.hidden;
        let rule = match p.fill_rule.as_deref() {
            Some("even_odd") => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        };
        // Illustrator's basic appearance draws the stroke above the fill.
        if let Some(s) = &p.stroke
            && let Some(brush) = self.brush(&s.paint, offset)
        {
            out.paints.push(paint(
                PaintKind::Stroke(StrokeStyle {
                    width: s.width,
                    cap: LineCap::from_token(&s.cap),
                    join: LineJoin::from_token(&s.join),
                    miter_limit: s.miter_limit,
                    dashes: s.dashes.clone(),
                    dash_offset: s.dash_offset,
                }),
                brush,
            ));
        }
        if let Some(f) = &p.fill
            && let Some(brush) = self.brush(f, offset)
        {
            out.paints.push(paint(PaintKind::Fill(rule), brush));
        }
        out
    }

    fn blend(&mut self, token: &str) -> BlendMode {
        BlendMode::from_token(token).unwrap_or_else(|| {
            self.warn(format!("blend mode {token} imported as normal"));
            BlendMode::Normal
        })
    }

    fn brush(&mut self, p: &PaintDto, offset: Offset) -> Option<Brush> {
        match p {
            PaintDto::Color { color } => Some(Brush::Solid(paint_color(*color))),
            PaintDto::Gradient(g) => Some(Brush::Gradient(self.gradient(g, offset))),
            PaintDto::Pattern { name } => {
                self.warn(format!("pattern \"{name}\" not imported"));
                None
            }
            PaintDto::Unknown => {
                self.warn("unknown paint type not imported");
                None
            }
        }
    }

    fn gradient(&mut self, g: &GradientDto, offset: Offset) -> Gradient {
        let start = offset.kpoint(g.start);
        let end = offset.kpoint(g.end);
        let kind = match g.gradient.as_str() {
            "radial" => {
                if let Some(m) = g.matrix
                    && !is_similarity(&m)
                {
                    self.warn("elliptical radial gradient imported as circular");
                }
                let (highlight_length, highlight_angle) = match g.focal {
                    Some(f) => highlight(start, end, offset.kpoint(f)),
                    None => (0.0, 0.0),
                };
                GradientKind::Radial {
                    highlight_length,
                    highlight_angle,
                }
            }
            "linear" => GradientKind::Linear,
            other => {
                self.warn(format!("{other} gradient imported as linear"));
                GradientKind::Linear
            }
        };
        Gradient {
            kind,
            start,
            end,
            stops: self.stops(&g.stops),
        }
    }

    /// Illustrator's stops plus one extra stop per moved midpoint (a stop's
    /// midpoint sits between it and the next stop; bae-vector interpolates
    /// linearly, so the midpoint colour is pinned where Illustrator puts it).
    fn stops(&mut self, stops: &[StopDto]) -> Vec<GradientStop> {
        let mut sorted: Vec<&StopDto> = stops.iter().collect();
        sorted.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        let mut out = Vec::with_capacity(sorted.len());
        for (i, s) in sorted.iter().enumerate() {
            out.push(GradientStop {
                offset: s.offset.clamp(0.0, 1.0) as f32,
                color: paint_color(s.color),
            });
            if let Some(next) = sorted.get(i + 1)
                && (s.midpoint - 0.5).abs() > 1e-3
                && next.offset > s.offset
            {
                let at = s.offset + (next.offset - s.offset) * s.midpoint.clamp(0.0, 1.0);
                let mid: [f64; 4] = std::array::from_fn(|k| (s.color[k] + next.color[k]) / 2.0);
                out.push(GradientStop {
                    offset: at.clamp(0.0, 1.0) as f32,
                    color: paint_color(mid),
                });
                self.warn("gradient midpoints approximated with an extra colour stop");
            }
        }
        if out.is_empty() {
            self.warn("gradient without colour stops imported white to black");
            out = Gradient::stops_from_ae(&[], 0);
        }
        out
    }

    fn text(&mut self, t: &TextDto, offset: Offset) -> AiItem {
        if self.options.text_outlines
            && let Some(outline) = &t.outline
        {
            let mut g = self.group(outline, offset);
            g.name = t.name.clone().or_else(|| Some(short(&t.contents)));
            g.hidden = t.hidden;
            g.opacity = t.opacity;
            g.blend = self.blend(&t.blend);
            return AiItem::Group(g);
        }
        let first = t.runs.first();
        let size = first.map(|r| r.size).filter(|s| *s > 0.0).unwrap_or(12.0);
        let mut out = AiText::new(
            t.contents.clone(),
            first.map(|r| r.font.clone()).unwrap_or_default(),
            size,
        );
        out.name = t.name.clone();
        out.hidden = t.hidden;
        if t.runs.len() > 1 {
            self.warn("text with several styles imported with the style of its first character");
        }
        if t.orientation == "vertical" {
            self.warn("vertical text imported as horizontal");
        }
        if t.opacity < 1.0 || t.blend != "normal" {
            self.warn("text opacity and blend mode not imported");
        }
        out.position = match (t.text_kind.as_str(), t.anchor, &t.frame) {
            ("point", Some(a), _) | ("", Some(a), _) => offset.point(a),
            (kind, _, frame) => {
                self.warn(format!(
                    "{kind} text imported as point text (outlines keep its exact shape)"
                ));
                let r = frame
                    .as_ref()
                    .and_then(|f| subpath_bounds(std::slice::from_ref(f)))
                    .or(t.bounds)
                    .unwrap_or([0.0; 4]);
                offset.point([r[0], r[1] + size])
            }
        };
        let (h, v) = first
            .map(|r| (r.h_scale, r.v_scale))
            .unwrap_or((100.0, 100.0));
        let [a, b, c, d] = t.matrix.unwrap_or([1.0, 0.0, 0.0, 1.0]);
        // y down: a positive angle turns clockwise, as AiText expects.
        out.rotation = round6(b.atan2(a).to_degrees());
        out.scale = [round6(a.hypot(b) * h), round6(c.hypot(d) * v)];
        if let Some(r) = first {
            out.fill = match &r.fill {
                None => None,
                Some(p) => self.text_color(p),
            };
            out.stroke = match (&r.stroke, r.stroke_width) {
                (Some(p), Some(w)) if w > 0.0 => self.text_color(p).map(|c| (c, w)),
                _ => None,
            };
        }
        out.justification = match t
            .paragraphs
            .first()
            .map(|p| p.justification.as_str())
            .unwrap_or("left")
        {
            "center" | "justify_center" => "center",
            "right" | "justify_right" => "right",
            _ => "left",
        }
        .to_owned();
        AiItem::Text(out)
    }

    fn text_color(&mut self, p: &PaintDto) -> Option<[f64; 4]> {
        match p {
            PaintDto::Color { color } => Some(*color),
            PaintDto::Gradient(g) => {
                self.warn("gradient text imported with its first colour");
                g.stops.first().map(|s| s.color)
            }
            _ => None,
        }
    }
}

fn paint(kind: PaintKind, brush: Brush) -> Paint {
    Paint {
        kind,
        brush,
        opacity: 1.0,
        blend: BlendMode::Normal,
        composite: Composite::Below,
    }
}

fn count_outside(items: &[ItemDto], rects: &[Rect]) -> usize {
    items
        .iter()
        .map(|item| match item {
            ItemDto::Layer(l) => count_outside(&l.items, rects),
            _ => match item_bounds(item) {
                Some(b) if rects.iter().any(|r| overlaps(&b, r)) => 0,
                // Items without bounds (unknown types) are reported elsewhere.
                None => 0,
                Some(_) => 1,
            },
        })
        .sum()
}

/// Rotation + uniform scale (+ mirror): a circle stays a circle.
fn is_similarity(m: &[f64; 6]) -> bool {
    let [a, b, c, d, ..] = *m;
    let tol = 1e-3 * (a.abs() + b.abs() + c.abs() + d.abs()).max(1e-9);
    ((a - d).abs() < tol && (b + c).abs() < tol) || ((a + d).abs() < tol && (b - c).abs() < tol)
}

/// AE's highlight length (percent of the radius) and angle (degrees from the
/// start → end direction, clockwise in y-down space) for a focal point.
fn highlight(start: Point, end: Point, focal: Point) -> (f64, f64) {
    let d = end - start;
    let v = focal - start;
    let r = d.hypot();
    if r <= 0.0 || v.hypot() < 1e-6 {
        return (0.0, 0.0);
    }
    let length = (v.hypot() / r * 100.0).min(99.0);
    let mut angle = (v.y.atan2(v.x) - d.y.atan2(d.x)).to_degrees();
    if angle > 180.0 {
        angle -= 360.0;
    } else if angle <= -180.0 {
        angle += 360.0;
    }
    (round6(length), round6(angle))
}

fn round6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

fn short(s: &str) -> String {
    let line = s.split(['\r', '\n']).next().unwrap_or("");
    let mut out: String = line.chars().take(24).collect();
    if line.chars().count() > 24 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(json: &str) -> AiDocument {
        read_bridge("t", json.as_bytes()).expect("reads")
    }

    const HEAD: &str = r#""format": "bae-ai-bridge", "format_version": "0.1.0""#;

    #[test]
    fn rejects_other_formats_and_versions() {
        let e = read_bridge("t", br#"{"format": "x", "format_version": "0.1.0"}"#).unwrap_err();
        assert!(matches!(e, Error::Malformed(_)), "{e}");
        let e = read_bridge(
            "t",
            br#"{"format": "bae-ai-bridge", "format_version": "0.2.0"}"#,
        )
        .unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e}");
        assert!(read_bridge("t", b"not json").is_err());
    }

    #[test]
    fn points_move_into_artboard_space_with_relative_tangents() {
        let d = doc(&format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [100, -300, 500, 0]}}],
            "layers": [{{"type": "layer", "name": "L", "items": [
              {{"type": "path", "subpaths": [{{"closed": true, "points": [[110, -290, 105, -290, 115, -290], [200, -200, 200, -200, 200, -200]]}}],
                "fill": {{"type": "color", "color": [1, 0, 0, 1]}},
                "stroke": {{"paint": {{"type": "color", "color": [0, 0, 1, 1]}}, "width": 3, "cap": "round", "join": "bevel", "dashes": [4, 2]}}}}]}}]}}"#
        ));
        let ab = &doc_ab(&d, 0);
        assert_eq!(ab.origin, [100.0, -300.0]);
        assert_eq!(ab.size, [400.0, 300.0]);
        let AiItem::Path(p) = &ab.layers[0].items[0] else {
            panic!()
        };
        assert_eq!(p.subpaths[0].vertices, vec![[10.0, 10.0], [100.0, 100.0]]);
        assert_eq!(p.subpaths[0].in_tangents[0], [-5.0, 0.0]);
        assert_eq!(p.subpaths[0].out_tangents[0], [5.0, 0.0]);
        // Stroke above fill.
        assert!(
            matches!(p.paints[0].kind, PaintKind::Stroke(ref s) if s.width == 3.0 && s.cap == LineCap::Round && s.join == LineJoin::Bevel && s.dashes == vec![4.0, 2.0])
        );
        assert!(matches!(
            p.paints[1].kind,
            PaintKind::Fill(FillRule::NonZero)
        ));
    }

    fn doc_ab(d: &AiDocument, i: usize) -> Artboard {
        d.artboards[i].clone()
    }

    #[test]
    fn items_go_to_the_artboards_they_touch() {
        let d = doc(&format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [0, -100, 100, 0]}}, {{"name": "B", "rect": [200, -100, 300, 0]}}],
            "layers": [{{"type": "layer", "name": "L", "items": [
              {{"type": "path", "name": "a", "bounds": [10, -90, 20, -80], "subpaths": []}},
              {{"type": "path", "name": "both", "bounds": [50, -50, 250, -40], "subpaths": []}},
              {{"type": "path", "name": "far", "bounds": [-900, -900, -800, -800], "subpaths": []}},
              {{"type": "layer", "name": "Sub", "items": [{{"type": "path", "name": "b", "bounds": [210, -90, 220, -80], "subpaths": []}}]}}
            ]}}]}}"#
        ));
        let names = |i: usize| -> Vec<String> {
            d.artboards[i].layers[0]
                .items
                .iter()
                .map(|it| match it {
                    AiItem::Path(p) => p.name.clone().unwrap(),
                    AiItem::Group(g) => g.name.clone().unwrap(),
                    AiItem::Text(_) => "text".into(),
                })
                .collect()
        };
        assert_eq!(names(0), ["a", "both"]);
        assert_eq!(names(1), ["both", "Sub"]);
        assert!(d.warnings.iter().any(|w| w.contains("1 item(s) outside")));
    }

    #[test]
    fn clip_groups_are_placed_by_their_clip_path() {
        // Illustrator reports the clipped-away disc in the group's bounds.
        let d = doc(&format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [0, -100, 100, 0]}}, {{"name": "B", "rect": [200, -100, 300, 0]}}],
            "layers": [{{"type": "layer", "name": "L", "items": [
              {{"type": "group", "clipped": true, "bounds": [0, -100, 250, 0],
                "clip": [{{"closed": true, "points": [[10, -90, 10, -90, 10, -90], [40, -60, 40, -60, 40, -60]]}}],
                "items": [{{"type": "path", "bounds": [0, -100, 250, 0], "subpaths": []}}]}}
            ]}}]}}"#
        ));
        assert_eq!(d.artboards[0].layers.len(), 1);
        assert!(d.artboards[1].layers.is_empty());
        let AiItem::Group(g) = &d.artboards[0].layers[0].items[0] else {
            panic!()
        };
        assert_eq!(g.clip.as_ref().unwrap()[0].vertices[1], [40.0, 40.0]);
    }

    #[test]
    fn gradient_midpoint_and_highlight() {
        let d = doc(&format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [0, -100, 100, 0]}}],
            "layers": [{{"type": "layer", "name": "L", "items": [
              {{"type": "path", "bounds": [0, -100, 100, 0], "subpaths": [],
                "fill": {{"type": "gradient", "gradient": "radial", "start": [50, -50], "end": [90, -50], "focal": [50, -70],
                  "matrix": [1, 0, 0, 1, 0, 0],
                  "stops": [{{"offset": 0, "midpoint": 0.2, "color": [0, 0, 0, 1]}}, {{"offset": 1, "midpoint": 0.5, "color": [1, 1, 1, 0.5]}}]}}}}
            ]}}]}}"#
        ));
        let AiItem::Path(p) = &d.artboards[0].layers[0].items[0] else {
            panic!()
        };
        let Brush::Gradient(g) = &p.paints[0].brush else {
            panic!()
        };
        // Focal point 20 up (y down: -y), radius 40 to the right: 50 %, -90 degrees.
        assert_eq!(
            g.kind,
            GradientKind::Radial {
                highlight_length: 50.0,
                highlight_angle: -90.0
            }
        );
        assert_eq!(g.start, Point::new(50.0, 50.0));
        let offsets: Vec<f32> = g.stops.iter().map(|s| s.offset).collect();
        assert_eq!(offsets, [0.0, 0.2, 1.0]);
        assert_eq!(g.stops[1].color, [0.5, 0.5, 0.5, 0.75]);
    }

    #[test]
    fn layer_opacity_wraps_items_and_template_layers_are_hidden() {
        let d = doc(&format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [0, -100, 100, 0]}}],
            "layers": [
              {{"type": "layer", "name": "Faded", "opacity": 0.5, "blend": "multiply", "items": [{{"type": "path", "bounds": [0, -10, 10, 0], "subpaths": []}}]}},
              {{"type": "layer", "name": "Tmpl", "printable": false, "items": [{{"type": "path", "bounds": [0, -10, 10, 0], "subpaths": []}}]}}
            ]}}"#
        ));
        let layers = &d.artboards[0].layers;
        let AiItem::Group(g) = &layers[0].items[0] else {
            panic!()
        };
        assert_eq!((g.opacity, g.blend), (0.5, BlendMode::Multiply));
        assert!(!layers[1].visible);
    }

    #[test]
    fn text_rotation_scale_and_outlines() {
        let json = format!(
            r#"{{{HEAD}, "artboards": [{{"name": "A", "rect": [0, -100, 100, 0]}}],
            "layers": [{{"type": "layer", "name": "L", "items": [
              {{"type": "text", "name": "T", "text_kind": "point", "contents": "Hi", "anchor": [10, -20],
                "matrix": [0.866025, -0.5, 0.5, 0.866025], "bounds": [10, -40, 40, -20],
                "paragraphs": [{{"justification": "center"}}],
                "runs": [{{"text": "Hi", "font": "MyriadPro-Regular", "size": 24, "h_scale": 100, "v_scale": 100,
                  "fill": {{"type": "color", "color": [1, 0, 0, 1]}}, "stroke": {{"type": "color", "color": [0, 0, 1, 1]}}, "stroke_width": 2}}],
                "outline": {{"type": "group", "items": [{{"type": "path", "subpaths": [{{"closed": true, "points": [[10, -20, 10, -20, 10, -20]]}}],
                  "fill": {{"type": "color", "color": [1, 0, 0, 1]}}}}]}}}}
            ]}}]}}"#
        );
        let d = doc(&json);
        let AiItem::Text(t) = &d.artboards[0].layers[0].items[0] else {
            panic!()
        };
        assert_eq!(t.position, [10.0, 80.0]);
        assert!((t.rotation + 30.0).abs() < 1e-3, "{}", t.rotation);
        assert_eq!(t.font, "MyriadPro-Regular");
        assert_eq!(t.justification, "center");
        assert_eq!(t.fill, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(t.stroke, Some(([0.0, 0.0, 1.0, 1.0], 2.0)));
        assert!((t.scale[0] - 100.0).abs() < 1e-3);

        let d = read_bridge_with(
            "t",
            json.as_bytes(),
            &BridgeOptions {
                text_outlines: true,
            },
        )
        .unwrap();
        let AiItem::Group(g) = &d.artboards[0].layers[0].items[0] else {
            panic!()
        };
        assert_eq!(g.name.as_deref(), Some("T"));
        assert_eq!(g.items.len(), 1);
    }

    #[test]
    fn similarity_check() {
        assert!(is_similarity(&[0.866, 0.5, -0.5, 0.866, 3.0, 4.0]));
        assert!(is_similarity(&[1.0, 0.0, 0.0, -1.0, 0.0, 0.0]));
        assert!(!is_similarity(&[1.0, 0.0, 0.0, 0.4, 0.0, 0.0]));
    }
}

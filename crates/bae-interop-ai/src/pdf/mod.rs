//! Reading .ai files (their embedded PDF) and plain PDFs into an
//! [`AiDocument`].
//!
//! Illustrator's PDF-compatible content, as written by Illustrator 25–30:
//!
//! - one page per artboard; the crop box is the artboard;
//! - every top-level layer is an optional content group (OCG): its name, its
//!   place in `/OCProperties /D /Order` (topmost first) and `/OFF` for hidden
//!   layers; page content is wrapped in `/OC /MCn BDC … EMC`;
//! - a gradient fill is "clip to the path, then `sh`"; a group with opacity
//!   or a blend mode is a transparency-group form XObject;
//! - `q`/`Q` pairs do not nest with the layers' marked content, so clip groups
//!   are rebuilt from every item's own clip stack, not from the stream shape;
//! - a clip to the artboard rectangle wraps layer content and is dropped.
//!
//! Plain groups are not visible in a PDF (their items are written one by one)
//! and neither are artboard names; the Illustrator bridge export has both.

mod color;
mod content;
mod file;
mod font;
mod lexer;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bae_model::BezierPath;
use bae_vector::PaintKind;
use bae_vector::kurbo::{Affine, Rect};

use crate::doc::{AiDocument, AiGroup, AiItem, AiLayer, Artboard, SourceFormat};
use crate::error::Error;
use content::{Clip, Drawn, DrawnKind, Interp, Warnings};
use file::PdfFile;
use lexer::{Dict, Obj, text_string};

/// Gap between artboards in the reconstructed layout (PDF does not record
/// where the artboards sit in the Illustrator document).
const ARTBOARD_GAP: f64 = 20.0;
const MAX_PAGES: usize = 1000;

/// Reads a PDF or a PDF-compatible .ai file.
pub(crate) fn read_pdf(name: &str, bytes: &[u8]) -> Result<AiDocument, Error> {
    let pdf = PdfFile::open(bytes)?;
    let root = pdf
        .root()
        .ok_or_else(|| Error::Malformed("no document catalog".into()))?;
    let mut pages = Vec::new();
    collect_pages(
        &pdf,
        &pdf.dict_get(&root, b"Pages"),
        &Dict::default(),
        &mut pages,
        &mut HashSet::new(),
        0,
    );
    if pages.is_empty() {
        return Err(Error::Malformed("the PDF has no pages".into()));
    }
    let info = pdf.dict_get(&pdf.trailer, b"Info");
    let creator = info
        .as_dict()
        .map(|d| match pdf.dict_get(d, b"Creator") {
            Obj::Str(s) => text_string(&s),
            _ => String::new(),
        })
        .unwrap_or_default();
    let is_ai = creator.contains("Illustrator")
        || pages.iter().any(|p| {
            pdf.dict_get(p, b"PieceInfo")
                .as_dict()
                .is_some_and(|d| d.get(b"Illustrator").is_some())
        });
    let layers = OcLayers::read(&pdf, &root);

    let mut warnings = Warnings::default();
    let mut artboards = Vec::new();
    let mut x = 0.0;
    let mut images = 0;
    for (i, page) in pages.iter().enumerate() {
        let bbox = page_box(&pdf, page);
        let (w, h) = (bbox.width(), bbox.height());
        let rotate = pdf.dict_get(page, b"Rotate").as_i64().unwrap_or(0);
        if rotate.rem_euclid(360) != 0 {
            warnings.push("rotated PDF pages are imported unrotated");
        }
        let to_board = Affine::new([1.0, 0.0, 0.0, -1.0, -bbox.x0, bbox.y1]);
        let resources = match pdf.dict_get(page, b"Resources") {
            Obj::Dict(d) => d,
            _ => Dict::default(),
        };
        let content = page_content(&pdf, page, &mut warnings);
        let page_rect = Rect::new(0.0, 0.0, w, h);
        let mut interp = Interp::new(&pdf, to_board, page_rect, &mut warnings);
        let drawn = merge_fill_stroke(interp.run_page(&content, &resources));
        images += interp.images;
        let mut board = Artboard::new(format!("Artboard {}", i + 1), [w, h]);
        board.origin = [x, 0.0];
        x += w + ARTBOARD_GAP;
        board.layers = layers.build(drawn, page_rect, &mut warnings);
        artboards.push(board);
    }
    if images > 0 {
        warnings.push(format!("{images} placed image(s) are not imported yet"));
    }
    Ok(AiDocument {
        name: name.to_owned(),
        source: if is_ai {
            SourceFormat::AiPdf
        } else {
            SourceFormat::Pdf
        },
        artboards,
        warnings: warnings.0,
    })
}

/// Leaf pages with inherited attributes merged in.
fn collect_pages(
    pdf: &PdfFile<'_>,
    node: &Obj,
    inherited: &Dict,
    out: &mut Vec<Dict>,
    seen: &mut HashSet<u32>,
    depth: usize,
) {
    if depth > 64 || out.len() >= MAX_PAGES {
        return;
    }
    if let Obj::Ref(n, _) = node
        && !seen.insert(*n)
    {
        return;
    }
    let Some(d) = pdf.resolve(node).as_dict().cloned() else {
        return;
    };
    let mut merged = inherited.clone();
    for key in [b"Resources".as_slice(), b"MediaBox", b"CropBox", b"Rotate"] {
        if let Some(v) = d.get(key) {
            merged.0.retain(|(k, _)| k != key);
            merged.0.push((key.to_vec(), v.clone()));
        }
    }
    match pdf.dict_get(&d, b"Kids") {
        Obj::Array(kids) => {
            for k in &kids {
                collect_pages(pdf, k, &merged, out, seen, depth + 1);
            }
        }
        _ => {
            let mut page = d.clone();
            for (k, v) in merged.0 {
                if page.get(&k).is_none() {
                    page.0.push((k, v));
                }
            }
            out.push(page);
        }
    }
}

/// Crop box (default media box), normalized; Letter when missing.
fn page_box(pdf: &PdfFile<'_>, page: &Dict) -> Rect {
    let rect = |key: &[u8]| -> Option<Rect> {
        let a: Vec<f64> = pdf
            .dict_get(page, key)
            .as_array()?
            .iter()
            .filter_map(|o| pdf.resolve(o).as_f64())
            .collect();
        match a.as_slice() {
            [x0, y0, x1, y1] => {
                let r = Rect::new(*x0, *y0, *x1, *y1).abs();
                (r.width() > 0.0 && r.height() > 0.0).then_some(r)
            }
            _ => None,
        }
    };
    let media = rect(b"MediaBox").unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));
    rect(b"CropBox")
        .map(|c| c.intersect(media))
        .filter(|c| c.width() > 0.0 && c.height() > 0.0)
        .unwrap_or(media)
}

fn page_content(pdf: &PdfFile<'_>, page: &Dict, warnings: &mut Warnings) -> Vec<u8> {
    let mut out = Vec::new();
    let parts = match pdf.dict_get(page, b"Contents") {
        Obj::Array(a) => a,
        Obj::Null => Vec::new(),
        _ => vec![page.get(b"Contents").cloned().unwrap_or(Obj::Null)],
    };
    for part in &parts {
        match pdf.stream_data(part) {
            Ok(data) => {
                out.extend_from_slice(&data);
                out.push(b'\n');
            }
            Err(e) => warnings.push(format!("page content skipped: {e}")),
        }
    }
    out
}

/// Optional content groups: names, Layers panel order, visibility.
struct OcLayers {
    names: HashMap<u32, String>,
    /// Top-level layers, topmost first.
    order: Vec<u32>,
    hidden: HashSet<u32>,
    locked: HashSet<u32>,
}

impl OcLayers {
    fn read(pdf: &PdfFile<'_>, root: &Dict) -> OcLayers {
        let mut l = OcLayers {
            names: HashMap::new(),
            order: Vec::new(),
            hidden: HashSet::new(),
            locked: HashSet::new(),
        };
        let Some(props) = pdf.dict_get(root, b"OCProperties").as_dict().cloned() else {
            return l;
        };
        if let Obj::Array(ocgs) = pdf.dict_get(&props, b"OCGs") {
            for o in &ocgs {
                if let Obj::Ref(n, _) = o
                    && let Some(d) = pdf.get(*n).as_dict()
                {
                    let name = match pdf.dict_get(d, b"Name") {
                        Obj::Str(s) => text_string(&s),
                        _ => String::new(),
                    };
                    l.names.insert(*n, name);
                }
            }
        }
        let Some(d) = pdf.dict_get(&props, b"D").as_dict().cloned() else {
            return l;
        };
        let refs = |o: Obj| -> Vec<u32> {
            match o {
                Obj::Array(a) => a
                    .iter()
                    .filter_map(|x| match x {
                        Obj::Ref(n, _) => Some(*n),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        // /Order: [layer, [sublayers…], layer …]; only top-level refs count.
        l.order = refs(pdf.dict_get(&d, b"Order"));
        l.hidden = refs(pdf.dict_get(&d, b"OFF")).into_iter().collect();
        l.locked = refs(pdf.dict_get(&d, b"Locked")).into_iter().collect();
        if pdf.dict_get(&d, b"BaseState").as_name() == Some(b"OFF") {
            let on: HashSet<u32> = refs(pdf.dict_get(&d, b"ON")).into_iter().collect();
            l.hidden.extend(l.names.keys().filter(|k| !on.contains(k)));
        }
        l
    }

    fn name(&self, id: u32) -> String {
        self.names
            .get(&id)
            .filter(|n| !n.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "Layer".to_owned())
    }

    /// Drawn items of one page (paint order) → layers, topmost first.
    fn build(&self, drawn: Vec<Drawn>, page: Rect, warnings: &mut Warnings) -> Vec<AiLayer> {
        let page = page.inflate(1.0, 1.0);
        let mut by_layer: Vec<(Option<u32>, Vec<Drawn>)> = Vec::new();
        let mut outside = 0;
        for d in drawn {
            if d.bounds.intersect(page).area() <= 0.0
                && !(d.bounds.width() == 0.0 || d.bounds.height() == 0.0)
            {
                outside += 1;
                continue;
            }
            let key = d.layer.first().copied();
            match by_layer.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push(d),
                None => by_layer.push((key, vec![d])),
            }
        }
        if outside > 0 {
            warnings.push(format!(
                "{outside} item(s) entirely outside their artboard were left out"
            ));
        }
        // Layers panel order: /Order first (topmost first), then the rest
        // topmost (last painted) first.
        by_layer.sort_by_key(|(k, _)| {
            match k.and_then(|k| self.order.iter().position(|o| *o == k)) {
                Some(i) => (0, i as i64),
                None => (1, 0),
            }
        });
        let unordered_start = by_layer
            .iter()
            .position(|(k, _)| !k.is_some_and(|k| self.order.contains(&k)))
            .unwrap_or(by_layer.len());
        by_layer[unordered_start..].reverse();

        by_layer
            .into_iter()
            .map(|(key, items)| {
                let mut layer = AiLayer::new(match key {
                    Some(id) => self.name(id),
                    None => "Layer 1".to_owned(),
                });
                if let Some(id) = key {
                    layer.visible = !self.hidden.contains(&id);
                    layer.locked = self.locked.contains(&id);
                }
                layer.items = self.sublayers(items, 1);
                layer
            })
            .collect()
    }

    /// Groups nested optional content (sublayers) at `level`, then clips;
    /// returns topmost-first items.
    fn sublayers(&self, items: Vec<Drawn>, level: usize) -> Vec<AiItem> {
        let mut out: Vec<AiItem> = Vec::new();
        let mut run: Vec<Drawn> = Vec::new();
        let mut run_key: Option<u32> = None;
        let flush = |run: &mut Vec<Drawn>, key: Option<u32>, out: &mut Vec<AiItem>| {
            if run.is_empty() {
                return;
            }
            let drained = std::mem::take(run);
            match key {
                Some(id) => {
                    let mut g = AiGroup::new(Some(self.name(id)));
                    g.hidden = self.hidden.contains(&id);
                    g.items = self.sublayers(drained, level + 1);
                    // `g.items` is topmost first; keep paint order in `out`.
                    out.push(AiItem::Group(g));
                }
                None => out.extend(clip_groups(drained, 0)),
            }
        };
        for d in items {
            let key = d.layer.get(level).copied();
            if key != run_key {
                flush(&mut run, run_key, &mut out);
                run_key = key;
            }
            run.push(d);
        }
        flush(&mut run, run_key, &mut out);
        out.reverse();
        out
    }
}

/// Illustrator sometimes writes one path's fill and stroke as two objects
/// (fill, then stroke of the same geometry). Put them back together.
fn merge_fill_stroke(drawn: Vec<Drawn>) -> Vec<Drawn> {
    let mut out: Vec<Drawn> = Vec::with_capacity(drawn.len());
    for mut d in drawn {
        if let DrawnKind::Group { items, .. } = &mut d.kind {
            *items = merge_fill_stroke(std::mem::take(items));
        }
        if let (Some(prev), DrawnKind::Path(stroke)) = (out.last_mut(), &d.kind)
            && let DrawnKind::Path(fill) = &mut prev.kind
            && prev.layer == d.layer
            && prev.clips.len() == d.clips.len()
            && prev
                .clips
                .iter()
                .zip(&d.clips)
                .all(|(a, b)| Rc::ptr_eq(a, b))
            && fill.blend == stroke.blend
            && fill
                .paints
                .iter()
                .all(|p| matches!(p.kind, PaintKind::Fill(_)))
            && stroke
                .paints
                .iter()
                .all(|p| matches!(p.kind, PaintKind::Stroke(_)))
            && same_geometry(&fill.subpaths, &stroke.subpaths)
        {
            let mut paints = stroke.paints.clone();
            paints.append(&mut fill.paints);
            fill.paints = paints;
            // The stroke's geometry keeps open / closed exactly.
            fill.subpaths = stroke.subpaths.clone();
            prev.bounds = prev.bounds.union(d.bounds);
            continue;
        }
        out.push(d);
    }
    out
}

fn same_geometry(a: &[BezierPath], b: &[BezierPath]) -> bool {
    let close = |x: &[[f64; 2]], y: &[[f64; 2]]| {
        x.len() == y.len()
            && x.iter()
                .zip(y)
                .all(|(p, q)| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6)
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| {
            let mut q = q.clone();
            content::close_if_coincident(&mut q);
            close(&p.vertices, &q.vertices)
                && close(&p.in_tangents, &q.in_tangents)
                && close(&p.out_tangents, &q.out_tangents)
        })
}

/// Consecutive items sharing a clip at `level` → clip groups. Input and
/// output are in paint order (bottom first).
fn clip_groups(items: Vec<Drawn>, level: usize) -> Vec<AiItem> {
    let mut out = Vec::new();
    let mut run: Vec<Drawn> = Vec::new();
    let mut run_clip: Option<Rc<Clip>> = None;
    let flush = |run: &mut Vec<Drawn>, clip: &Option<Rc<Clip>>, out: &mut Vec<AiItem>| {
        if run.is_empty() {
            return;
        }
        let drained = std::mem::take(run);
        match clip {
            Some(c) => {
                let mut g = AiGroup::new(None);
                g.clip = Some(c.paths.clone());
                let mut inner = clip_groups(drained, level + 1);
                inner.reverse();
                g.items = inner;
                out.push(AiItem::Group(g));
            }
            None => out.extend(drained.into_iter().map(convert)),
        }
    };
    for d in items {
        let clip = d.clips.get(level).cloned();
        let same = match (&clip, &run_clip) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same || clip.is_none() {
            flush(&mut run, &run_clip, &mut out);
            run_clip = clip;
        }
        run.push(d);
    }
    flush(&mut run, &run_clip, &mut out);
    out
}

/// One drawn element as an item (a transparency group's content is clipped
/// and grouped on its own).
fn convert(d: Drawn) -> AiItem {
    match d.kind {
        DrawnKind::Path(p) => AiItem::Path(p),
        DrawnKind::Text(t) => AiItem::Text(t),
        DrawnKind::Group {
            opacity,
            blend,
            items,
        } => {
            let mut inner = clip_groups(items, 0);
            inner.reverse();
            // A transparency group around a single path is that path's
            // opacity and blend mode.
            if let [AiItem::Path(_)] = inner.as_slice()
                && let Some(AiItem::Path(mut p)) = inner.pop()
            {
                p.opacity *= opacity;
                if p.blend == bae_vector::BlendMode::Normal {
                    p.blend = blend;
                }
                return AiItem::Path(p);
            }
            let mut g = AiGroup::new(None);
            g.opacity = opacity;
            g.blend = blend;
            g.items = inner;
            AiItem::Group(g)
        }
    }
}

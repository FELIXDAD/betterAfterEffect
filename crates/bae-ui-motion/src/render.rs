//! Simplified preview of an [`EvaluatedComp`] with the egui painter.
//!
//! Stand-in until the GPU compositor (04 `bae-render`) publishes viewport textures:
//! solids, footage placeholders, shape layers (rectangles, ellipses, stars, Bezier
//! paths with fills and strokes, nested groups with their transforms), text and
//! pre-comps are drawn as vector meshes in comp space. No effects, masks, mattes,
//! blend modes or 3D perspective (3D layers are drawn orthographically).

use bae_anim::glam::{DMat4, DVec2, DVec3};
use bae_anim::{EvaluatedComp, EvaluatedLayer, EvaluatedPrim, LayerSource};
use bae_model::{BezierPath, LayerKind, PrimType, Value};
use egui::epaint::{Mesh, PathShape, PathStroke, TextShape};
use egui::{Color32, FontId, Pos2, Rect, Shape, Stroke, pos2};

/// Comp space → screen.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    /// Screen position of comp pixel (0, 0).
    pub origin: Pos2,
    /// Points per comp pixel.
    pub scale: f32,
    /// Horizontal stretch from the source pixel aspect ratio.
    pub aspect: f32,
    /// Path point and handle size in points (Preferences > Appearance).
    pub point: f32,
    /// The 3D view's camera and the comp size: 3D layers are seen through it
    /// (`None`: x and y only).
    pub camera: Option<(bae_render::Camera, DVec2)>,
}

impl View {
    /// Screen position of comp-space point `p` of a layer, through the camera
    /// for a 3D layer; `None` behind the camera.
    pub fn project(self, p: DVec3, three_d: bool) -> Option<Pos2> {
        match self.camera {
            Some((camera, size)) if three_d => camera.project(p, size).map(|q| self.to_screen(q)),
            _ => Some(self.to_screen(p.truncate())),
        }
    }

    pub fn to_screen(self, p: DVec2) -> Pos2 {
        pos2(
            self.origin.x + p.x as f32 * self.scale * self.aspect,
            self.origin.y + p.y as f32 * self.scale,
        )
    }

    pub fn to_comp(self, p: Pos2) -> DVec2 {
        DVec2::new(
            f64::from((p.x - self.origin.x) / (self.scale * self.aspect)),
            f64::from((p.y - self.origin.y) / self.scale),
        )
    }
}

/// 2D affine transform taken from a layer matrix (x, y only).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Affine {
    m: DMat4,
}

impl Affine {
    pub fn new(m: DMat4) -> Self {
        Affine { m }
    }

    pub fn apply(&self, p: DVec2) -> DVec2 {
        let q = self.m.transform_point3(DVec3::new(p.x, p.y, 0.0));
        DVec2::new(q.x, q.y)
    }

    /// Average scale factor (for stroke widths and text size).
    pub fn scale(&self) -> f64 {
        let a = self.m.x_axis;
        let b = self.m.y_axis;
        (a.x * b.y - a.y * b.x).abs().sqrt()
    }

    /// Rotation angle in radians.
    pub fn angle(&self) -> f64 {
        self.m.x_axis.y.atan2(self.m.x_axis.x)
    }

    pub fn then(&self, inner: &DMat4) -> Affine {
        Affine { m: self.m * *inner }
    }
}

fn color32(c: [f64; 4], opacity: f64) -> Color32 {
    let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(to(c[0]), to(c[1]), to(c[2]), to(c[3] * opacity))
}

fn vec2_attr(p: &EvaluatedPrim, attr: &str, default: [f64; 2]) -> DVec2 {
    match p.get(attr) {
        Some(Value::Vec2(v)) => DVec2::new(v[0], v[1]),
        Some(Value::Vec3(v)) => DVec2::new(v[0], v[1]),
        _ => DVec2::from_array(default),
    }
}

/// A polyline in layer space.
#[derive(Clone, Debug)]
pub(crate) struct Poly {
    pub points: Vec<DVec2>,
    pub closed: bool,
}

/// Paint instructions for one layer, in layer space, back to front.
#[derive(Clone, Debug)]
pub(crate) enum Draw {
    Fill {
        polys: Vec<Poly>,
        color: Color32,
    },
    Stroke {
        polys: Vec<Poly>,
        color: Color32,
        width: f64,
    },
}

fn ellipse(center: DVec2, size: DVec2) -> Poly {
    let n = 64;
    let points = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
            center + DVec2::new(a.cos() * size.x / 2.0, a.sin() * size.y / 2.0)
        })
        .collect();
    Poly {
        points,
        closed: true,
    }
}

fn rect(center: DVec2, size: DVec2, roundness: f64) -> Poly {
    let h = size / 2.0;
    let r = roundness.max(0.0).min(h.x.abs()).min(h.y.abs());
    if r <= 0.0 {
        let points = vec![
            center + DVec2::new(-h.x, -h.y),
            center + DVec2::new(h.x, -h.y),
            center + DVec2::new(h.x, h.y),
            center + DVec2::new(-h.x, h.y),
        ];
        return Poly {
            points,
            closed: true,
        };
    }
    let mut points = Vec::new();
    let corners = [
        (DVec2::new(h.x - r, -h.y + r), -90.0),
        (DVec2::new(h.x - r, h.y - r), 0.0),
        (DVec2::new(-h.x + r, h.y - r), 90.0),
        (DVec2::new(-h.x + r, -h.y + r), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=8 {
            let a = (start + 90.0 * f64::from(i) / 8.0_f64).to_radians();
            points.push(center + c + DVec2::new(a.cos(), a.sin()) * r);
        }
    }
    Poly {
        points,
        closed: true,
    }
}

fn star(p: &EvaluatedPrim) -> Poly {
    let polygon = matches!(p.get("star_type"), Some(Value::Enum(s)) if s == "polygon");
    let n = p.f64("points").unwrap_or(5.0).round().clamp(3.0, 200.0) as usize;
    let center = vec2_attr(p, "position", [0.0, 0.0]);
    let rot = p.f64("rotation").unwrap_or(0.0).to_radians();
    let outer = p.f64("outer_radius").unwrap_or(100.0);
    let inner = p.f64("inner_radius").unwrap_or(50.0);
    let count = if polygon { n } else { n * 2 };
    let points = (0..count)
        .map(|i| {
            let a =
                rot - std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / count as f64;
            let r = if polygon || i % 2 == 0 { outer } else { inner };
            center + DVec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    Poly {
        points,
        closed: true,
    }
}

fn bezier(path: &BezierPath) -> Poly {
    let v = &path.vertices;
    let mut points = Vec::new();
    let n = v.len();
    if n == 0 {
        return Poly {
            points,
            closed: false,
        };
    }
    let seg = |i: usize, j: usize, points: &mut Vec<DVec2>| {
        let p0 = DVec2::from_array(v[i]);
        let p3 = DVec2::from_array(v[j]);
        let p1 = p0 + DVec2::from_array(path.out_tangents.get(i).copied().unwrap_or_default());
        let p2 = p3 + DVec2::from_array(path.in_tangents.get(j).copied().unwrap_or_default());
        for k in 1..=16 {
            let t = f64::from(k) / 16.0;
            let u = 1.0 - t;
            points.push(
                p0 * (u * u * u)
                    + p1 * (3.0 * u * u * t)
                    + p2 * (3.0 * u * t * t)
                    + p3 * (t * t * t),
            );
        }
    };
    points.push(DVec2::from_array(v[0]));
    for i in 0..n - 1 {
        seg(i, i + 1, &mut points);
    }
    if path.closed && n > 1 {
        seg(n - 1, 0, &mut points);
        points.pop();
    }
    Poly {
        points,
        closed: path.closed,
    }
}

pub(crate) fn group_matrix(p: &EvaluatedPrim) -> DMat4 {
    let anchor = vec2_attr(p, "transform:anchor_point", [0.0, 0.0]);
    let pos = vec2_attr(p, "transform:position", [0.0, 0.0]);
    let scale = vec2_attr(p, "transform:scale", [100.0, 100.0]) / 100.0;
    let rot = p.f64("transform:rotation").unwrap_or(0.0).to_radians();
    DMat4::from_translation(DVec3::new(pos.x, pos.y, 0.0))
        * DMat4::from_rotation_z(rot)
        * DMat4::from_scale(DVec3::new(scale.x, scale.y, 1.0))
        * DMat4::from_translation(DVec3::new(-anchor.x, -anchor.y, 0.0))
}

fn transform_poly(poly: &Poly, m: &DMat4) -> Poly {
    let a = Affine::new(*m);
    Poly {
        points: poly.points.iter().map(|p| a.apply(*p)).collect(),
        closed: poly.closed,
    }
}

/// AE shape semantics (simplified): fills and strokes paint every path above them in
/// their group (including paths of nested groups above them); items higher in the
/// list are drawn in front. Returns the group's paths (for fills further down in the
/// parent) and its draws, front first.
fn shape_items(items: &[EvaluatedPrim], m: &DMat4, opacity: f64) -> (Vec<Poly>, Vec<Draw>) {
    let mut paths: Vec<Poly> = Vec::new();
    let mut draws: Vec<Draw> = Vec::new();
    for item in items {
        let ty = item.prim_type.as_ref();
        let op = |p: &EvaluatedPrim| p.f64("opacity").unwrap_or(100.0) / 100.0;
        match ty {
            Some(t) if *t == PrimType::RECT => {
                let p = rect(
                    vec2_attr(item, "position", [0.0, 0.0]),
                    vec2_attr(item, "size", [100.0, 100.0]),
                    item.f64("roundness").unwrap_or(0.0),
                );
                paths.push(transform_poly(&p, m));
            }
            Some(t) if *t == PrimType::ELLIPSE => {
                let p = ellipse(
                    vec2_attr(item, "position", [0.0, 0.0]),
                    vec2_attr(item, "size", [100.0, 100.0]),
                );
                paths.push(transform_poly(&p, m));
            }
            Some(t) if *t == PrimType::STAR => paths.push(transform_poly(&star(item), m)),
            Some(t) if *t == PrimType::PATH => {
                if let Some(Value::Bezier(b)) = item.get("path") {
                    paths.push(transform_poly(&bezier(b), m));
                }
            }
            Some(t) if *t == PrimType::SHAPE_GROUP => {
                let gm = *m * group_matrix(item);
                let go = opacity * item.f64("transform:opacity").unwrap_or(100.0) / 100.0;
                let (p, d) = shape_items(&item.children, &gm, go);
                paths.extend(p);
                draws.extend(d);
            }
            Some(t) if *t == PrimType::FILL || *t == PrimType::GRADIENT_FILL => {
                let c = match item.get("color") {
                    Some(Value::Color(c)) => *c,
                    _ => [0.6, 0.6, 0.6, 1.0],
                };
                draws.push(Draw::Fill {
                    polys: paths.clone(),
                    color: color32(c, opacity * op(item)),
                });
            }
            Some(t) if *t == PrimType::STROKE || *t == PrimType::GRADIENT_STROKE => {
                let c = match item.get("color") {
                    Some(Value::Color(c)) => *c,
                    _ => [1.0, 1.0, 1.0, 1.0],
                };
                draws.push(Draw::Stroke {
                    polys: paths.clone(),
                    color: color32(c, opacity * op(item)),
                    width: item.f64("width").unwrap_or(2.0) * Affine::new(*m).scale(),
                });
            }
            // Folders (a group's `contents`) are transparent.
            _ if item.children.iter().any(|c| c.prim_type.is_some()) => {
                let (p, d) = shape_items(&item.children, m, opacity);
                paths.extend(p);
                draws.extend(d);
            }
            _ => {}
        }
    }
    (paths, draws)
}

/// Draws of a shape layer's contents in layer space, front first.
pub(crate) fn shape_draws(layer: &EvaluatedLayer) -> Vec<Draw> {
    shape_items(&layer.contents, &DMat4::IDENTITY, 1.0).1
}

/// Bounds of a layer's content in layer space (for selection and hit testing).
pub(crate) fn layer_bounds(layer: &EvaluatedLayer) -> Option<(DVec2, DVec2)> {
    match &layer.source {
        LayerSource::Asset { info, .. } if info.width > 0 && info.height > 0 => {
            return Some((
                DVec2::ZERO,
                DVec2::new(f64::from(info.width), f64::from(info.height)),
            ));
        }
        LayerSource::Comp {
            nested: Some(n), ..
        } => {
            return Some((
                DVec2::ZERO,
                DVec2::new(f64::from(n.width), f64::from(n.height)),
            ));
        }
        _ => {}
    }
    match layer.kind {
        LayerKind::Shape => {
            let (paths, _) = shape_items(&layer.contents, &DMat4::IDENTITY, 1.0);
            let mut min = DVec2::splat(f64::INFINITY);
            let mut max = DVec2::splat(f64::NEG_INFINITY);
            for p in paths.iter().flat_map(|p| &p.points) {
                min = min.min(*p);
                max = max.max(*p);
            }
            (min.x <= max.x).then_some((min, max))
        }
        LayerKind::Text => layer.text.as_ref().map(|t| {
            if let Some(rect) = bae_text::layout::box_rect(t) {
                return (DVec2::new(rect.x0, rect.y0), DVec2::new(rect.x1, rect.y1));
            }
            let w = if t.runs.is_empty() {
                t.style.font_size * 0.55 * t.text.chars().count().max(1) as f64
            } else {
                t.text
                    .chars()
                    .enumerate()
                    .map(|(i, _)| {
                        let style = t.style_at(i);
                        style.font_size * (0.55 + style.tracking / 1000.)
                    })
                    .sum()
            };
            let x0 = match t.paragraph.justification.as_str() {
                "center" => -w / 2.0,
                "right" => -w,
                _ => 0.0,
            };
            (
                DVec2::new(x0, -t.style.font_size * 0.8),
                DVec2::new(x0 + w, t.style.font_size * 0.25),
            )
        }),
        LayerKind::Null => Some((DVec2::ZERO, DVec2::splat(100.0))),
        _ => None,
    }
}

/// Ear-clipping triangulation of a simple polygon (no holes).
pub(crate) fn triangulate(points: &[Pos2]) -> Vec<u32> {
    let n = points.len();
    if n < 3 {
        return Vec::new();
    }
    let area: f32 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    let ccw = area > 0.0;
    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity((n - 2) * 3);
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let inside = |p: Pos2, a: Pos2, b: Pos2, c: Pos2| {
        let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let (ia, ib, ic) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            let c2 = cross(a, b, c);
            let convex = if ccw { c2 > 0.0 } else { c2 < 0.0 };
            if !convex {
                continue;
            }
            let any_inside = idx
                .iter()
                .any(|&j| j != ia && j != ib && j != ic && inside(points[j], a, b, c));
            if any_inside {
                continue;
            }
            out.extend([ia as u32, ib as u32, ic as u32]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            // Degenerate or self-intersecting: fall back to a fan.
            for k in 1..idx.len() - 1 {
                out.extend([idx[0] as u32, idx[k] as u32, idx[k + 1] as u32]);
            }
            return out;
        }
    }
    if idx.len() == 3 {
        out.extend(idx.iter().map(|&i| i as u32));
    }
    out
}

fn fill_shape(points: Vec<Pos2>, color: Color32) -> Shape {
    let indices = triangulate(&points);
    let mut mesh = Mesh::default();
    for p in &points {
        mesh.colored_vertex(*p, color);
    }
    mesh.indices = indices;
    Shape::mesh(mesh)
}

/// Paints one comp (recursively for pre-comps) into `shapes`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_comp(
    comp: &EvaluatedComp,
    painter: &egui::Painter,
    to_screen: &dyn Fn(DVec2) -> Pos2,
    outer: Affine,
    opacity: f64,
    view_scale: f64,
    font: &dyn Fn(f32) -> FontId,
    shapes: &mut Vec<Shape>,
) {
    let solo = comp.has_solo();
    for layer in comp.layers.iter().rev() {
        if !layer.visible || (solo && !layer.switches.solo) || layer.switches.adjustment {
            continue;
        }
        let xf = outer.then(&layer.world_matrix);
        let alpha = opacity * layer.opacity;
        let screen = |p: DVec2| to_screen(xf.apply(p));
        match &layer.source {
            LayerSource::Asset { info, .. } => {
                let (w, h) = (f64::from(info.width), f64::from(info.height));
                let quad: Vec<Pos2> = [
                    DVec2::ZERO,
                    DVec2::new(w, 0.0),
                    DVec2::new(w, h),
                    DVec2::new(0.0, h),
                ]
                .into_iter()
                .map(screen)
                .collect();
                if let Some(c) = info.solid_color {
                    shapes.push(fill_shape(quad, color32(c, alpha)));
                } else {
                    // Footage: a placeholder until decoded frames reach the viewer.
                    let c = color32([0.25, 0.25, 0.28, 1.0], alpha);
                    shapes.push(fill_shape(quad.clone(), c));
                    shapes.push(Shape::line_segment(
                        [quad[0], quad[2]],
                        Stroke::new(1.0, Color32::from_gray(90)),
                    ));
                    shapes.push(Shape::line_segment(
                        [quad[1], quad[3]],
                        Stroke::new(1.0, Color32::from_gray(90)),
                    ));
                }
            }
            LayerSource::Comp {
                nested: Some(nested),
                ..
            } => paint_comp(
                nested, painter, to_screen, xf, alpha, view_scale, font, shapes,
            ),
            _ => {}
        }
        match layer.kind {
            LayerKind::Shape => {
                for d in shape_draws(layer).iter().rev() {
                    match d {
                        Draw::Fill { polys, color } => {
                            for p in polys.iter().filter(|p| p.closed || p.points.len() > 2) {
                                let pts: Vec<Pos2> = p.points.iter().map(|q| screen(*q)).collect();
                                shapes.push(fill_shape(pts, color.gamma_multiply(alpha as f32)));
                            }
                        }
                        Draw::Stroke {
                            polys,
                            color,
                            width,
                        } => {
                            let w = (*width * xf.scale() * view_scale) as f32;
                            for p in polys {
                                let pts: Vec<Pos2> = p.points.iter().map(|q| screen(*q)).collect();
                                let stroke =
                                    PathStroke::new(w.max(0.5), color.gamma_multiply(alpha as f32));
                                shapes.push(if p.closed {
                                    Shape::Path(PathShape::closed_line(pts, stroke))
                                } else {
                                    Shape::Path(PathShape::line(pts, stroke))
                                });
                            }
                        }
                    }
                }
            }
            LayerKind::Text => {
                if let Some(t) = &layer.text
                    && alpha > 0.
                {
                    shapes.push(text_shape(
                        painter,
                        t,
                        (xf.scale() * view_scale) as f32,
                        alpha,
                        xf,
                        &screen,
                        font,
                    ));
                }
            }
            _ => {}
        }
    }
}

/// TextEdit and the lightweight viewer use the same character advances for hit testing.
pub(crate) fn text_job(
    doc: &bae_model::TextDocument,
    scale: f32,
    alpha: f64,
    font: &dyn Fn(f32) -> FontId,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut chars = doc.text.char_indices().peekable();
    let mut start = 0;
    for (style, end) in std::iter::once(&doc.style)
        .chain(doc.runs.iter().map(|r| &r.style))
        .zip(
            doc.runs
                .iter()
                .map(|r| r.start)
                .chain(std::iter::once(usize::MAX)),
        )
    {
        let from = chars.peek().map_or(doc.text.len(), |(i, _)| *i);
        let n = chars.by_ref().take(end.saturating_sub(start)).count();
        let to = chars.peek().map_or(doc.text.len(), |(i, _)| *i);
        let size = style.font_size as f32 * scale;
        let (factor, valign) = match style.baseline_option {
            bae_model::BaselineOption::Normal => (1., egui::Align::BOTTOM),
            bae_model::BaselineOption::Superscript => (0.6, egui::Align::TOP),
            bae_model::BaselineOption::Subscript => (0.6, egui::Align::BOTTOM),
        };
        let mut format = egui::TextFormat::simple(
            font((size * factor).max(1.)),
            color32(style.fill_color, alpha),
        );
        format.italics = style.faux_italic;
        format.valign = valign;
        format.extra_letter_spacing = style.tracking as f32 / 1000. * size;
        format.line_height = style.leading.map(|v| v as f32 * scale);
        job.append(&doc.text[from..to], 0., format);
        start += n;
    }
    job
}

fn text_shape(
    painter: &egui::Painter,
    t: &bae_model::TextDocument,
    scale: f32,
    alpha: f64,
    xf: Affine,
    screen: &dyn Fn(DVec2) -> Pos2,
    font: &dyn Fn(f32) -> FontId,
) -> Shape {
    let size = t.style.font_size as f32 * scale;
    let galley = painter.layout_job(text_job(t, scale, alpha, font));
    let ascent = galley
        .rows
        .first()
        .map_or(size * 0.8, |r| r.row.height() * 0.78);
    let w = galley.size().x;
    let dx = match t.paragraph.justification.as_str() {
        "center" => -w / 2.0,
        "right" => -w,
        _ => 0.0,
    };
    // Baseline at the layer origin, rotated with the layer.
    let angle = xf.angle() as f32;
    let (sin, cos) = angle.sin_cos();
    let off = egui::vec2(dx * cos + ascent * sin, dx * sin - ascent * cos);
    let pos = screen(DVec2::ZERO) + off;
    Shape::Text(TextShape::new(pos, galley, Color32::WHITE).with_angle(angle))
}

/// Screen-space quad of a layer's bounds (for selection outlines).
pub(crate) fn bounds_quad(
    layer: &EvaluatedLayer,
    view: &View,
    outer: &Affine,
) -> Option<[Pos2; 4]> {
    let (min, max) = layer_bounds(layer)?;
    let c = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
    if is_3d(layer) {
        let [a, b, c, d] =
            c.map(|p| view.project(layer.world_matrix.transform_point3(p.extend(0.0)), true));
        return Some([a?, b?, c?, d?]);
    }
    let xf = outer.then(&layer.world_matrix);
    Some(c.map(|p| view.to_screen(xf.apply(p))))
}

/// A 3D layer the camera sees (cameras and lights have no picture).
pub(crate) fn is_3d(layer: &EvaluatedLayer) -> bool {
    layer.switches.three_d && !matches!(layer.kind, LayerKind::Camera | LayerKind::Light)
}

/// Whether `p` lies inside the convex quad.
pub(crate) fn quad_contains(q: &[Pos2; 4], p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// The comp rectangle on screen.
pub(crate) fn comp_rect(view: &View, width: u32, height: u32) -> Rect {
    Rect::from_min_max(
        view.to_screen(DVec2::ZERO),
        view.to_screen(DVec2::new(f64::from(width), f64::from(height))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_concave_star() {
        let p = star(&EvaluatedPrim {
            path: bae_model::PrimPath::new("/s").unwrap(),
            prim_type: Some(PrimType::STAR),
            attrs: Default::default(),
            children: Vec::new().into(),
        });
        assert_eq!(p.points.len(), 10);
        let pts: Vec<Pos2> = p
            .points
            .iter()
            .map(|v| pos2(v.x as f32, v.y as f32))
            .collect();
        let tris = triangulate(&pts);
        assert_eq!(tris.len(), (10 - 2) * 3);
    }

    #[test]
    fn quad_hit() {
        let q = [
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
        ];
        assert!(quad_contains(&q, pos2(5.0, 5.0)));
        assert!(!quad_contains(&q, pos2(15.0, 5.0)));
    }
}

//! 3D layers (2.5D, AE's Classic 3D renderer): grouping the layer stack into 3D
//! groups, projective layer geometry and per-pixel-exact depth order.
//!
//! * [`segments`] splits a comp's stack into what is composited by stacking order
//!   and contiguous runs of 3D layers ("3D bins", AE-CMP-406): 2D layers and
//!   adjustment layers break a run; cameras, lights and layers without pixels do
//!   not. A 3D pre-comp is one plane of the run (AE 26.2.1 sorts it by depth
//!   with the other 3D layers) unless effects, masks or a track matte flatten it.
//! * [`Homography`] is a planar projective map (a 3D layer seen through a camera,
//!   or a 2D affine), with the clipping and bounds the render graph needs for ROI
//!   negotiation.
//! * [`depth_order`] orders the layers of one 3D group back to front for the
//!   camera (AE-CMP-404) and splits layers that pass through each other along the
//!   intersection line (AE-CMP-405), BSP style: every layer is a planar quad, so
//!   the order is exact per pixel. Each [`Fragment`] is a layer drawn with up to
//!   [`MAX_CLIPS`] half-planes in its own layer space. Layers less than
//!   [`TIE_DEPTH`] apart keep the stacking order (the higher layer in front),
//!   which also moves intersection seams like AE does.

use glam::{DAffine2, DMat3, DMat4, DVec2, DVec3};

use crate::blend::MaskMode;
use crate::geom::Rect;
use crate::input::{LayerContent, RenderLayer};

/// Part of a comp's layer stack, bottom first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// A layer composited by stacking order (2D layers, adjustment layers).
    Layer(usize),
    /// A 3D group: layer indices, bottom of the stack first, rendered together
    /// in depth order and composited onto the layers below.
    Group(Vec<usize>),
}

/// The drawn layers of `layers` (top first, as in [`crate::RenderComp`]) from
/// the bottom of the stack up, with contiguous 3D layers grouped (AE-CMP-406).
/// Hidden and inactive layers are left out; layers without pixels (nulls,
/// cameras, lights) neither draw nor break a group.
pub fn segments(layers: &[RenderLayer]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut group = Vec::new();
    for index in (0..layers.len()).rev() {
        let layer = &layers[index];
        if !layer.visible || !layer.active {
            continue;
        }
        let draws = layer.adjustment || !matches!(layer.content, LayerContent::None);
        if !draws {
            continue;
        }
        if layer.matrix_3d.is_some() && !layer.adjustment {
            // A 3D pre-comp with effects, masks or a track matte is flattened
            // and composited on its own (AE-CMP-406); a plain one sorts with
            // the group like any 3D layer (measured in AE 26.2.1).
            let flattened = matches!(layer.content, LayerContent::Comp(_))
                && (layer.effects.iter().any(|e| e.enabled)
                    || layer.masks.iter().any(|m| m.mode != MaskMode::None)
                    || layer.track_matte.is_some());
            if !flattened {
                group.push(index);
                continue;
            }
            if !group.is_empty() {
                out.push(Segment::Group(std::mem::take(&mut group)));
            }
            out.push(Segment::Group(vec![index]));
            continue;
        }
        if !group.is_empty() {
            out.push(Segment::Group(std::mem::take(&mut group)));
        }
        out.push(Segment::Layer(index));
    }
    if !group.is_empty() {
        out.push(Segment::Group(group));
    }
    out
}

/// A planar projective map `p ↦ (X/w, Y/w)` with `(X, Y, w) = M · (p, 1)`.
///
/// For a 3D layer seen through a camera `w` is the camera depth of the point
/// ([`crate::camera::Camera::plane_homography`]); affine maps have `w ≡ 1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography(pub DMat3);

impl Homography {
    /// The identity map.
    pub const IDENTITY: Homography = Homography(DMat3::IDENTITY);

    /// An affine map as a homography.
    pub fn from_affine(a: &DAffine2) -> Self {
        Homography(DMat3::from_cols(
            a.matrix2.x_axis.extend(0.0),
            a.matrix2.y_axis.extend(0.0),
            a.translation.extend(1.0),
        ))
    }

    /// The affine map, when this is one (last row `(0, 0, 1)` up to rounding).
    pub fn as_affine(&self) -> Option<DAffine2> {
        let r = self.0.row(2);
        let scale = r.z.abs().max(1e-300);
        if r.x.abs() / scale > 1e-12 || r.y.abs() / scale > 1e-12 || r.z.abs() < 1e-300 {
            return None;
        }
        let m = self.0 * (1.0 / r.z);
        Some(DAffine2::from_cols(
            m.x_axis.truncate(),
            m.y_axis.truncate(),
            m.z_axis.truncate(),
        ))
    }

    /// `other` applied first, then `self`.
    pub fn then_after(&self, other: &Homography) -> Homography {
        Homography(self.0 * other.0)
    }

    /// `self` applied first, then the affine `a`.
    pub fn followed_by(&self, a: &DAffine2) -> Homography {
        Homography::from_affine(a).then_after(self)
    }

    /// `a` applied first, then `self`.
    pub fn preceded_by(&self, a: &DAffine2) -> Homography {
        self.then_after(&Homography::from_affine(a))
    }

    /// The homogeneous weight of `p` (camera depth for 3D layers).
    pub fn w(&self, p: DVec2) -> f64 {
        self.0.row(2).dot(p.extend(1.0))
    }

    /// The image of `p`; `None` where `w ≤ 0` (behind the camera).
    pub fn map(&self, p: DVec2) -> Option<DVec2> {
        let v = self.0 * p.extend(1.0);
        (v.z > 0.0).then(|| v.truncate() / v.z)
    }

    /// The inverse map (`None` when degenerate).
    pub fn inverse(&self) -> Option<Homography> {
        let det = self.0.determinant();
        (det.is_finite() && det.abs() > 1e-300).then(|| Homography(self.0.inverse()))
    }

    /// Whether the map is non-degenerate (the layer is not seen edge-on).
    pub fn is_invertible(&self) -> bool {
        let m = self.0;
        // Relative to the scale of the matrix, so tiny layers still count.
        let norm = m.x_axis.length() * m.y_axis.length() * m.z_axis.length();
        let det = m.determinant();
        det.is_finite() && norm > 0.0 && det.abs() > 1e-12 * norm
    }

    /// The part of `rect` (input space) that lies at `w ≥ near` and maps into
    /// `canvas` (output space), as a convex polygon in input space (empty when
    /// nothing is visible).
    pub fn visible(&self, rect: &Rect, canvas: &Rect, near: f64) -> Vec<DVec2> {
        if rect.is_empty() || canvas.is_empty() {
            return Vec::new();
        }
        let mut poly = rect.corners().to_vec();
        let [r0, r1, r2] = [self.0.row(0), self.0.row(1), self.0.row(2)];
        // w ≥ near, then X ≥ x0·w, X ≤ x1·w, Y ≥ y0·w, Y ≤ y1·w (w > 0 there).
        let planes = [
            r2 - DVec3::new(0.0, 0.0, near),
            r0 - canvas.x0 * r2,
            canvas.x1 * r2 - r0,
            r1 - canvas.y0 * r2,
            canvas.y1 * r2 - r1,
        ];
        for plane in planes {
            poly = clip_polygon(&poly, plane);
            if poly.len() < 3 {
                return Vec::new();
            }
        }
        poly
    }

    /// Bounding box of the images of `points` (which must have `w > 0`).
    pub fn image_bounds(&self, points: &[DVec2]) -> Rect {
        let mut r = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in points {
            if let Some(q) = self.map(*p) {
                r.x0 = r.x0.min(q.x);
                r.y0 = r.y0.min(q.y);
                r.x1 = r.x1.max(q.x);
                r.y1 = r.y1.max(q.y);
            }
        }
        if r.x0 > r.x1 { Rect::default() } else { r }
    }

    /// Lengths of the images of the input's unit x and y vectors at `p` (the
    /// local axis scales; [`crate::geom::axis_scales`] for affine maps).
    pub fn axis_scales_at(&self, p: DVec2) -> DVec2 {
        let v = self.0 * p.extend(1.0);
        if v.z.abs() < 1e-300 {
            return DVec2::splat(f64::INFINITY);
        }
        let q = v.truncate() / v.z;
        let g = self.0.row(2).truncate();
        let jx = (self.0.x_axis.truncate() - q * g.x) / v.z;
        let jy = (self.0.y_axis.truncate() - q * g.y) / v.z;
        DVec2::new(jx.length(), jy.length())
    }

    /// The largest local axis scale over `points`.
    pub fn max_scale(&self, points: &[DVec2]) -> f64 {
        points
            .iter()
            .map(|p| self.axis_scales_at(*p).max_element())
            .fold(0.0, f64::max)
    }
}

/// Bounding box of `points` (empty rect for none).
pub fn points_bounds(points: &[DVec2]) -> Rect {
    if points.is_empty() {
        return Rect::default();
    }
    let mut r = Rect::new(f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in points {
        r.x0 = r.x0.min(p.x);
        r.y0 = r.y0.min(p.y);
        r.x1 = r.x1.max(p.x);
        r.y1 = r.y1.max(p.y);
    }
    r
}

/// `poly` clipped to the half-plane `plane · (p, 1) ≥ 0` (Sutherland–Hodgman).
fn clip_polygon(poly: &[DVec2], plane: DVec3) -> Vec<DVec2> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    let d = |p: DVec2| plane.dot(p.extend(1.0));
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (d(a), d(b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}

/// Most half-planes a [`Fragment`] carries. Layers that would be cut more often
/// keep their last piece whole (sorted by its centre); this only matters for
/// many mutually intersecting layers.
pub const MAX_CLIPS: usize = 4;

/// A planar layer of a 3D group for [`depth_order`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plate {
    /// Layer space → comp space.
    pub matrix: DMat4,
    /// The layer's drawn rectangle in layer space (z = 0).
    pub rect: Rect,
}

/// One piece of a layer in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    /// Index into the plates given to [`depth_order`].
    pub item: usize,
    /// Half-planes `a·x + b·y + c ≥ 0` in the layer's space (`(a, b)` unit
    /// length, so the value is a distance in layer pixels) the piece is cut to.
    pub clips: Vec<[f64; 3]>,
}

/// Distance below which a point counts as on a plane (comp pixels).
const PLANE_EPS: f64 = 1e-4;

/// Depth along the view ray (comp pixels) within which the layer higher in the
/// stack wins over another layer's plane. Measured in AE 26.2.1: the seam of a
/// layer crossing a camera-facing one at 45° sits one pixel into the lower
/// layer's half.
pub const TIE_DEPTH: f64 = 1.0;

struct Poly {
    item: usize,
    points: Vec<DVec3>,
    clips: Vec<[f64; 3]>,
}

struct Node {
    normal: DVec3,
    offset: f64,
    on: Vec<Poly>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
}

/// Back-to-front drawing order of `plates` (bottom of the stack first) seen
/// from `eye` (comp space), with layers that pass through each other split
/// along their intersection. Degenerate plates (zero area) are left out.
pub fn depth_order(plates: &[Plate], eye: DVec3) -> Vec<Fragment> {
    let polys: Vec<Poly> = plates
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.rect.is_empty() && plane_of(p).is_some())
        .map(|(item, p)| Poly {
            item,
            points: p
                .rect
                .corners()
                .iter()
                .map(|c| p.matrix.transform_point3(c.extend(0.0)))
                .collect(),
            clips: Vec::new(),
        })
        .collect();
    if polys.len() <= 1 {
        return polys
            .into_iter()
            .map(|p| Fragment {
                item: p.item,
                clips: p.clips,
            })
            .collect();
    }
    let mut budget = plates.len() * 4 + 16;
    let tree = build(polys, plates, eye, &mut budget);
    let mut out = Vec::with_capacity(plates.len());
    emit(tree, eye, &mut out);
    out
}

/// Unit normal and offset (`n · x + d = 0`) of a plate's plane.
fn plane_of(p: &Plate) -> Option<(DVec3, f64)> {
    let x = p.matrix.x_axis.truncate();
    let y = p.matrix.y_axis.truncate();
    let n = x.cross(y);
    let len = n.length();
    if !len.is_finite() || len < 1e-12 {
        return None;
    }
    let n = n / len;
    Some((n, -n.dot(p.matrix.w_axis.truncate())))
}

fn build(
    mut polys: Vec<Poly>,
    plates: &[Plate],
    eye: DVec3,
    budget: &mut usize,
) -> Option<Box<Node>> {
    if polys.is_empty() {
        return None;
    }
    let root = polys.remove(0);
    let (normal, offset) = plane_of(&plates[root.item]).expect("degenerate plates filtered");
    let mut node = Node {
        normal,
        offset,
        on: vec![root],
        front: None,
        back: None,
    };
    let (mut front, mut back) = (Vec::new(), Vec::new());
    // Towards the eye: +1 or −1 (0 when the eye is on the plane).
    let towards_eye = (normal.dot(eye) + offset).signum();
    let root_item = node.on[0].item;
    for poly in polys {
        // Ties go to the higher layer (the larger item): another layer's points
        // within TIE_DEPTH (along the view ray) of this plane count as behind it
        // when this layer is higher, as in front of it when that layer is higher.
        let centre = poly.points.iter().sum::<DVec3>() / poly.points.len() as f64;
        let along_ray = normal.dot((centre - eye).normalize_or_zero()).abs();
        let tie = TIE_DEPTH * along_ray * towards_eye;
        let bias = match poly.item.cmp(&root_item) {
            std::cmp::Ordering::Less => -tie,
            std::cmp::Ordering::Greater => tie,
            std::cmp::Ordering::Equal => 0.0,
        };
        let offset = offset + bias;
        let d: Vec<f64> = poly
            .points
            .iter()
            .map(|p| normal.dot(*p) + offset)
            .collect();
        let pos = d.iter().any(|&v| v > PLANE_EPS);
        let neg = d.iter().any(|&v| v < -PLANE_EPS);
        match (pos, neg) {
            (false, false) => node.on.push(poly),
            (true, false) => front.push(poly),
            (false, true) => back.push(poly),
            (true, true) => {
                if poly.clips.len() >= MAX_CLIPS || *budget == 0 {
                    // Too many cuts: keep the piece whole, on its centre's side.
                    if d.iter().sum::<f64>() >= 0.0 {
                        front.push(poly);
                    } else {
                        back.push(poly);
                    }
                    continue;
                }
                *budget -= 1;
                let (f, b) = split(poly, &d, normal, offset, plates);
                front.extend(f);
                back.extend(b);
            }
        }
    }
    node.front = build(front, plates, eye, budget);
    node.back = build(back, plates, eye, budget);
    Some(Box::new(node))
}

/// Splits `poly` (signed distances `d` to the plane) into its front and back
/// pieces, each with the plane's trace as a new half-plane in layer space.
fn split(
    poly: Poly,
    d: &[f64],
    normal: DVec3,
    offset: f64,
    plates: &[Plate],
) -> (Option<Poly>, Option<Poly>) {
    let m = plates[poly.item].matrix;
    // The plane's trace on the layer: n · (M · (x, y, 0, 1)) + offset.
    let line = DVec3::new(
        normal.dot(m.x_axis.truncate()),
        normal.dot(m.y_axis.truncate()),
        normal.dot(m.w_axis.truncate()) + offset,
    );
    let len = line.truncate().length();
    if len < 1e-12 {
        // Parallel planes cannot straddle; keep it on its centre's side.
        let side = d.iter().sum::<f64>() >= 0.0;
        return if side {
            (Some(poly), None)
        } else {
            (None, Some(poly))
        };
    }
    let line = line / len;
    let n = poly.points.len();
    let (mut fp, mut bp) = (Vec::with_capacity(n + 1), Vec::with_capacity(n + 1));
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (poly.points[i], poly.points[j]);
        let (da, db) = (d[i], d[j]);
        if da >= 0.0 {
            fp.push(a);
        }
        if da <= 0.0 {
            bp.push(a);
        }
        if (da > 0.0 && db < 0.0) || (da < 0.0 && db > 0.0) {
            let x = a + (b - a) * (da / (da - db));
            fp.push(x);
            bp.push(x);
        }
    }
    let piece = |points: Vec<DVec3>, sign: f64| {
        (points.len() >= 3).then(|| {
            let mut clips = poly.clips.clone();
            clips.push((line * sign).to_array());
            Poly {
                item: poly.item,
                points,
                clips,
            }
        })
    };
    (piece(fp, 1.0), piece(bp, -1.0))
}

fn emit(node: Option<Box<Node>>, eye: DVec3, out: &mut Vec<Fragment>) {
    let Some(node) = node else { return };
    let node = *node;
    // The half-space without the eye is farther away: draw it first.
    let eye_in_front = node.normal.dot(eye) + node.offset >= 0.0;
    let (first, second) = if eye_in_front {
        (node.back, node.front)
    } else {
        (node.front, node.back)
    };
    emit(first, eye, out);
    out.extend(node.on.into_iter().map(|p| Fragment {
        item: p.item,
        clips: p.clips,
    }));
    emit(second, eye, out);
}

#[cfg(test)]
mod tests {
    use bae_model::PrimPath;

    use super::*;
    use crate::camera::Camera;

    fn solid(name: &str) -> RenderLayer {
        RenderLayer::new(
            PrimPath::new(&format!("/comps/c/layers/{name}")).unwrap(),
            LayerContent::Solid {
                color: [1.0; 4],
                width: 10,
                height: 10,
            },
        )
    }

    fn three_d(name: &str) -> RenderLayer {
        let mut l = solid(name);
        l.matrix_3d = Some(DMat4::IDENTITY);
        l
    }

    #[test]
    fn segments_group_contiguous_3d_layers() {
        // Top first: a(3D) b(3D) null(2D, no pixels) c(2D) d(3D) e(3D, hidden) f(3D)
        let mut null = solid("null");
        null.content = LayerContent::None;
        let mut hidden = three_d("e");
        hidden.visible = false;
        let layers = vec![
            three_d("a"),
            three_d("b"),
            null,
            solid("c"),
            three_d("d"),
            hidden,
            three_d("f"),
        ];
        assert_eq!(
            segments(&layers),
            vec![
                Segment::Group(vec![6, 4]),
                Segment::Layer(3),
                Segment::Group(vec![1, 0]),
            ]
        );
        // Adjustment layers break groups even when 3D.
        let mut adj = three_d("adj");
        adj.adjustment = true;
        adj.content = LayerContent::None;
        let layers = vec![three_d("a"), adj, three_d("b")];
        assert_eq!(
            segments(&layers),
            vec![
                Segment::Group(vec![2]),
                Segment::Layer(1),
                Segment::Group(vec![0]),
            ]
        );
    }

    #[test]
    fn segments_sort_3d_precomps_with_other_layers() {
        let mut pre = three_d("pre");
        pre.content = LayerContent::Comp(std::sync::Arc::new(crate::RenderComp::new(
            PrimPath::new("/comps/inner").unwrap(),
            10,
            10,
        )));
        let layers = vec![three_d("a"), pre.clone(), three_d("b")];
        assert_eq!(segments(&layers), vec![Segment::Group(vec![2, 1, 0])]);
        // With a track matte it is flattened and composited on its own.
        let mut matted = pre;
        matted.track_matte = Some(crate::TrackMatte {
            layer: 0,
            mode: crate::TrackMatteMode::Alpha,
        });
        let layers = vec![three_d("a"), matted, three_d("b")];
        assert_eq!(
            segments(&layers),
            vec![
                Segment::Group(vec![2]),
                Segment::Group(vec![1]),
                Segment::Group(vec![0]),
            ]
        );
    }

    #[test]
    fn visible_part_clips_near_plane_and_canvas() {
        let cam = Camera::default_for(100, 100);
        // A layer receding from z = 0 at its left edge through the camera plane.
        let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 0.0))
            * DMat4::from_rotation_y(-80f64.to_radians());
        let h = Homography(cam.plane_homography(&m, DVec2::splat(100.0)));
        let rect = Rect::from_size(1000.0, 100.0);
        let canvas = Rect::from_size(100.0, 100.0);
        let poly = h.visible(&rect, &canvas, crate::camera::NEAR);
        assert!(poly.len() >= 3);
        for p in &poly {
            assert!(h.w(*p) >= crate::camera::NEAR - 1e-9);
            let q = h.map(*p).unwrap();
            assert!(q.x > -1e-6 && q.x < 100.0 + 1e-6, "{q}");
        }
        // Affine maps: just the canvas.
        let a = Homography::from_affine(&DAffine2::from_translation(DVec2::new(-50.0, 0.0)));
        let poly = a.visible(&Rect::from_size(100.0, 10.0), &canvas, 0.0);
        let b = points_bounds(&poly);
        assert!((b.x0 - 50.0).abs() < 1e-9 && (b.x1 - 100.0).abs() < 1e-9);
        assert!(a.as_affine().is_some());
        assert!(h.as_affine().is_none());
    }

    #[test]
    fn axis_scales_halve_at_double_distance() {
        let cam = Camera::default_for(100, 100);
        let m = DMat4::from_translation(DVec3::new(0.0, 0.0, cam.zoom));
        let h = Homography(cam.plane_homography(&m, DVec2::splat(100.0)));
        let s = h.axis_scales_at(DVec2::new(30.0, 70.0));
        assert!((s - DVec2::splat(0.5)).length() < 1e-12, "{s}");
    }

    fn plate(z: f64, rot_y: f64) -> Plate {
        Plate {
            matrix: DMat4::from_translation(DVec3::new(0.0, 0.0, z))
                * DMat4::from_rotation_y(rot_y.to_radians())
                * DMat4::from_translation(DVec3::new(-50.0, -50.0, 0.0)),
            rect: Rect::from_size(100.0, 100.0),
        }
    }

    const EYE: DVec3 = DVec3::new(0.0, 0.0, -1000.0);

    #[test]
    fn parallel_layers_sort_by_depth_not_stack() {
        // Bottom-of-stack plate is nearest the camera: it must be drawn last.
        let order = depth_order(
            &[plate(-100.0, 0.0), plate(0.0, 0.0), plate(100.0, 0.0)],
            EYE,
        );
        let items: Vec<usize> = order.iter().map(|f| f.item).collect();
        assert_eq!(items, vec![2, 1, 0]);
        assert!(order.iter().all(|f| f.clips.is_empty()));
        // Seen from behind the order flips.
        let order = depth_order(
            &[plate(-100.0, 0.0), plate(0.0, 0.0)],
            DVec3::new(0.0, 0.0, 1000.0),
        );
        assert_eq!(order.iter().map(|f| f.item).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn coplanar_layers_keep_stacking_order() {
        let order = depth_order(&[plate(0.0, 0.0), plate(0.0, 0.0), plate(0.0, 0.0)], EYE);
        assert_eq!(
            order.iter().map(|f| f.item).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn intersecting_layers_are_split() {
        // A flat plate and one turned 45° about y through the same centre.
        let plates = [plate(0.0, 0.0), plate(0.0, 45.0)];
        let order = depth_order(&plates, EYE);
        assert_eq!(order.len(), 3, "{order:?}");
        // The turned plate is cut in two near its centre line x = 50: where it
        // is less than TIE_DEPTH (in depth) behind the flat one it still wins
        // (it is the higher layer), so the cut sits √2 · TIE_DEPTH to the left.
        let cut: Vec<&Fragment> = order.iter().filter(|f| f.item == 1).collect();
        assert_eq!(cut.len(), 2);
        for f in &cut {
            let [a, b, c] = f.clips[0];
            assert!((b.abs()) < 1e-9 && (a.abs() - 1.0).abs() < 1e-9);
            let x = -c / a;
            assert!(
                x < 50.0 - 0.9 * TIE_DEPTH && x > 50.0 - 1.5 * TIE_DEPTH,
                "{x}"
            );
        }
        // Back half (left side, x < 50, which is pushed away from the eye)
        // first, then the flat plate, then the front half.
        let first = &order[0];
        let last = &order[2];
        assert_eq!((first.item, order[1].item, last.item), (1, 0, 1));
        let side = |f: &Fragment, x: f64| f.clips[0][0] * x + f.clips[0][2];
        assert!(side(first, 0.0) > 0.0, "{first:?}");
        assert!(side(last, 100.0) > 0.0, "{last:?}");
    }

    #[test]
    fn clip_budget_limits_fragments() {
        // Many layers fanned around the same axis all intersect each other.
        let plates: Vec<Plate> = (0..12).map(|i| plate(0.0, f64::from(i) * 15.0)).collect();
        let order = depth_order(&plates, EYE);
        assert!(order.len() <= plates.len() * 5 + 16, "{}", order.len());
        assert!(order.iter().all(|f| f.clips.len() <= MAX_CLIPS));
        // Every layer is still drawn.
        for i in 0..plates.len() {
            assert!(order.iter().any(|f| f.item == i));
        }
    }
}

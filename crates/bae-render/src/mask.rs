//! Layer masks (docs/spec/ae/compositing-3d-color.md AE-CMP-036..045): which
//! masks count, how they combine, where the result can be non-zero, and the
//! geometry the GPU rasterises.
//!
//! Masks act on the layer's source in layer space, before effects and the layer
//! transform (AE-CMP-007). The render graph plans one [`MaskNode`] per masked
//! layer; the executor runs, per mask (`gpu/mask.wgsl`):
//!
//! 1. **Coverage.** The flattened path is accumulated as exact signed area per
//!    texel: every edge adds, to each texel of its rows, the part of the texel to
//!    its right (additive blending into an RG16F buffer). R holds the path's
//!    winding number, G the winding of the Mask Expansion outline: a stroke of
//!    width `2·|expansion|` with round joins (kurbo), so growing is the union with
//!    the stroke (rounded outer corners, like a morphological dilation) and
//!    shrinking removes the stroke from the inside (sharp inner corners). The
//!    resolve applies the fill rule and clips to the layer frame (feather sees
//!    transparent pixels outside the layer, AE-CMP-041).
//! 2. **Feather.** A separable Gaussian with `sigma = FEATHER_TO_SIGMA × feather`,
//!    separately in x and y. Large feathers are rasterised and blurred at a
//!    reduced resolution (the result is smooth, so a bilinear upsample is
//!    indistinguishable) – the cost does not grow with the feather.
//! 3. **Combine.** `m = inverted ? 1 − cov : cov`, times the mask opacity, merged
//!    top to bottom into an accumulator with fixed-function blending: Add
//!    `acc + m − acc·m`, Subtract `acc·(1 − m)`, Intersect `acc·m`, Lighten
//!    `max`, Darken `min`, Difference `acc + m − 2·acc·m` (AE-CMP-038). The
//!    accumulator starts at 1 when the first mask subtracts, intersects or
//!    darkens, else 0. A single mask skips the accumulator: the layer draw
//!    applies `a + b·cov` itself.
//!
//! The layer draw then multiplies its alpha by the result (`composite.wgsl`); for
//! layers with effects, a layer-space composite applies the mask before the first
//! effect.

use std::hash::{Hash, Hasher};

use bae_model::{BezierPath, PrimPath};
use kurbo::{BezPath, PathEl, Point, Stroke, StrokeOpts};

use crate::blend::MaskMode;
use crate::geom::{IRect, Rect};
use crate::input::RenderMask;

/// Mask Feather (pixels) → Gaussian sigma (pixels). Skottie's empirical match to
/// AE (AE-CMP-041): the 10–90 % ramp is about as wide as the feather value.
/// To be confirmed against AE (`tests/ae/render_cases.jsx`, mask cases).
pub const FEATHER_TO_SIGMA: f64 = 0.38;

/// Fill rule for self-intersecting mask paths. AE-CMP-045 assumes nonzero
/// (Skottie and lottie-web use it); to be confirmed against AE.
pub const MASK_EVEN_ODD: bool = false;

/// Largest raster side in texels (the coverage buffers are scaled down beyond
/// it); leaves room for the atlas gutters within an 8192-texel page.
const MAX_TEXELS: f64 = 8000.0;

/// A path edge in raster texels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaskEdge {
    /// Start.
    pub p0: [f32; 2],
    /// End.
    pub p1: [f32; 2],
    /// Part of the expansion outline (G channel) rather than the path (R).
    pub stroke: bool,
}

/// How a mask's expansion outline modifies its coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Expansion {
    /// No expansion.
    None,
    /// Union with the outline (expansion > 0).
    Grow,
    /// Minus the outline (expansion < 0).
    Shrink,
}

/// One mask, rasterised and feathered on its own buffer.
#[derive(Clone, Debug)]
pub struct MaskRaster {
    /// Mode (how it merges into the masks above).
    pub mode: MaskMode,
    /// Mask value `m = value[0] + value[1] · coverage` (inversion and opacity).
    pub value: [f32; 2],
    /// Expansion outline usage.
    pub expansion: Expansion,
    /// Edges in raster texels.
    pub edges: Vec<MaskEdge>,
    /// Right bound of all edges (texels, at most the raster width): the edge
    /// quads reach the texel containing it, beyond which windings cancel.
    pub xmax: f32,
    /// Raster region in layer pixels.
    pub rect: IRect,
    /// Layer pixels per raster texel along x and y (reduced separately for
    /// large feathers).
    pub pixel_scale: [f64; 2],
    /// Gaussian sigma in raster texels (x, y); 0 = no feather on that axis.
    pub sigma: [f32; 2],
    /// The layer frame in raster texels: coverage outside it is 0 (decal border).
    pub clip: Option<[f32; 4]>,
}

impl MaskRaster {
    /// Raster size in texels.
    pub fn texels(&self) -> [u32; 2] {
        texels_xy(self.rect, self.pixel_scale)
    }
}

/// The mask stage of a layer.
#[derive(Clone, Debug)]
pub struct MaskNode {
    /// The layer.
    pub layer: PrimPath,
    /// Active masks, top first.
    pub rasters: Vec<MaskRaster>,
    /// Initial accumulator value (1 when the first mask subtracts, intersects or
    /// darkens).
    pub init: f32,
    /// Accumulator region in layer pixels (with one mask, the raster itself is
    /// the output and this is unused).
    pub rect: IRect,
    /// Layer pixels per accumulator texel.
    pub pixel_scale: f64,
}

impl MaskNode {
    /// The output is the single raster's coverage (the draw applies its value)
    /// rather than a combined accumulator.
    pub fn is_single(&self) -> bool {
        self.rasters.len() == 1
    }
}

/// The masks of a layer that affect its alpha (closed, mode not None), and how
/// they start.
pub(crate) struct MaskStack<'a> {
    masks: Vec<&'a RenderMask>,
    init: f32,
}

impl<'a> MaskStack<'a> {
    /// `None` when no mask affects alpha (AE-CMP-036: open paths and None masks
    /// never do).
    pub fn new(masks: &'a [RenderMask]) -> Option<Self> {
        let masks: Vec<&RenderMask> = masks
            .iter()
            .filter(|m| m.mode != MaskMode::None && m.shape.closed)
            .collect();
        let first = masks.first()?;
        let init = match first.mode {
            MaskMode::Subtract | MaskMode::Intersect | MaskMode::Darken => 1.0,
            _ => 0.0,
        };
        Some(Self { masks, init })
    }

    /// What the layer draw applies to the node's output: `M = a + b · v`.
    pub fn output_value(&self) -> [f32; 2] {
        match self.masks.as_slice() {
            [only] => {
                let [a, b] = mask_value(only);
                // With one mask the accumulator is `init` merged with `m`:
                // Subtract starts at 1 → 1 − m; every other mode yields m.
                if only.mode == MaskMode::Subtract {
                    [1.0 - a, -b]
                } else {
                    [a, b]
                }
            }
            _ => [0.0, 1.0],
        }
    }

    /// Layer-space bounds outside which the combined mask is 0, `None` when it
    /// can be non-zero anywhere (inverted masks, a subtracting first mask).
    pub fn support(&self) -> Option<Rect> {
        let mut acc: Option<Rect> = if self.init > 0.0 {
            None
        } else {
            Some(Rect::default())
        };
        for m in &self.masks {
            let own = mask_support(m);
            acc = match m.mode {
                MaskMode::Add | MaskMode::Lighten | MaskMode::Difference => match (acc, own) {
                    (Some(a), Some(b)) => Some(union(&a, &b)),
                    _ => None,
                },
                MaskMode::Intersect | MaskMode::Darken => match (acc, own) {
                    (Some(a), Some(b)) => Some(intersect(&a, &b)),
                    (None, b) => b,
                    (a, None) => a,
                },
                MaskMode::Subtract | MaskMode::None => acc,
            };
        }
        acc
    }

    /// Hash of everything that affects the result.
    pub fn hash_into(&self, h: &mut impl Hasher) {
        self.masks.len().hash(h);
        for m in &self.masks {
            m.hash_into(h);
        }
    }

    /// Plans the GPU work for an output covering `region` (layer pixels) at
    /// `pixel_scale` layer pixels per texel. `frame` is the layer frame for
    /// raster layers (coverage outside it is 0 before feathering).
    pub fn plan(
        &self,
        layer: &PrimPath,
        region: IRect,
        pixel_scale: f64,
        frame: Option<Rect>,
    ) -> MaskNode {
        let largest = f64::from(region.width().max(region.height()).max(1));
        let pixel_scale = pixel_scale.max(largest / MAX_TEXELS);
        let rasters = self
            .masks
            .iter()
            .map(|m| plan_raster(m, region, pixel_scale, frame))
            .collect();
        MaskNode {
            layer: layer.clone(),
            rasters,
            init: self.init,
            rect: region,
            pixel_scale,
        }
    }
}

/// `m = a + b · coverage` of one mask (AE-CMP-039/040).
fn mask_value(m: &RenderMask) -> [f32; 2] {
    let opacity = m.opacity.clamp(0.0, 1.0);
    if m.inverted {
        [opacity, -opacity]
    } else {
        [0.0, opacity]
    }
}

/// Feather sigmas in layer pixels.
fn sigma_px(m: &RenderMask) -> [f64; 2] {
    m.feather.map(|f| f64::from(f).max(0.0) * FEATHER_TO_SIGMA)
}

/// Bounds where one mask's value can be non-zero (`None`: anywhere).
fn mask_support(m: &RenderMask) -> Option<Rect> {
    if m.opacity <= 0.0 {
        return Some(Rect::default());
    }
    if m.inverted {
        return None;
    }
    Some(geometry_support(m).unwrap_or_default())
}

/// Bounds of a mask's (expanded, feathered) coverage, ignoring inversion and
/// opacity; `None` for an empty path.
fn geometry_support(m: &RenderMask) -> Option<Rect> {
    let b = path_bounds(&m.shape)?;
    let s = sigma_px(m);
    let grow = f64::from(m.expansion).max(0.0) + 1.0;
    Some(Rect::new(
        b.x0 - grow - 3.0 * s[0],
        b.y0 - grow - 3.0 * s[1],
        b.x1 + grow + 3.0 * s[0],
        b.y1 + grow + 3.0 * s[1],
    ))
}

/// Bounds of a path's vertices and control points (contains the curve).
fn path_bounds(path: &BezierPath) -> Option<Rect> {
    let mut r = Rect::new(
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut add = |p: [f64; 2]| {
        if p[0].is_finite() && p[1].is_finite() {
            r.x0 = r.x0.min(p[0]);
            r.y0 = r.y0.min(p[1]);
            r.x1 = r.x1.max(p[0]);
            r.y1 = r.y1.max(p[1]);
        }
    };
    for (i, v) in path.vertices.iter().enumerate() {
        add(*v);
        for list in [&path.in_tangents, &path.out_tangents] {
            if let Some(t) = list.get(i) {
                add([v[0] + t[0], v[1] + t[1]]);
            }
        }
    }
    (r.x0 <= r.x1 && r.y0 <= r.y1).then_some(r)
}

fn union(a: &Rect, b: &Rect) -> Rect {
    if a.is_empty() {
        return *b;
    }
    if b.is_empty() {
        return *a;
    }
    Rect::new(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    )
}

/// Intersection (empty rectangles stay empty).
pub(crate) fn intersect(a: &Rect, b: &Rect) -> Rect {
    let r = Rect::new(
        a.x0.max(b.x0),
        a.y0.max(b.y0),
        a.x1.min(b.x1),
        a.y1.min(b.y1),
    );
    if r.is_empty() { Rect::default() } else { r }
}

/// AE's vertex + relative-tangent form as a kurbo path (one closed sub-path).
pub fn to_bez_path(path: &BezierPath) -> BezPath {
    let mut out = BezPath::new();
    let n = path.vertices.len();
    if n == 0 {
        return out;
    }
    let point = |p: [f64; 2]| Point::new(p[0], p[1]);
    let tangent = |list: &[[f64; 2]], k: usize| list.get(k).copied().unwrap_or([0.0, 0.0]);
    out.move_to(point(path.vertices[0]));
    let segments = if path.closed { n } else { n - 1 };
    for i in 0..segments {
        let j = (i + 1) % n;
        let (v0, v1) = (path.vertices[i], path.vertices[j]);
        let o = tangent(&path.out_tangents, i);
        let t = tangent(&path.in_tangents, j);
        if o == [0.0, 0.0] && t == [0.0, 0.0] {
            out.line_to(point(v1));
        } else {
            out.curve_to(
                point([v0[0] + o[0], v0[1] + o[1]]),
                point([v1[0] + t[0], v1[1] + t[1]]),
                point(v1),
            );
        }
    }
    if path.closed {
        out.close_path();
    }
    out
}

/// Plans one mask's raster for an output `region` (layer px) at `pixel_scale`.
fn plan_raster(m: &RenderMask, region: IRect, pixel_scale: f64, frame: Option<Rect>) -> MaskRaster {
    let sigma = sigma_px(m);
    // Reduce each axis so its sigma keeps 3–6 texels: the blur costs at most
    // 37 taps per pass whatever the feather, on a buffer a fraction of the size.
    let mut reduce = sigma.map(|s| (s / pixel_scale / 3.0).floor().max(1.0));
    // Coverage needed: the output region plus the blur's reach, clipped to the
    // layer frame (decal) and to where this mask can be non-zero.
    let support = geometry_support(m);
    let rect_for = |reduce: [f64; 2]| {
        let mx = (3.0 * sigma[0] + 2.0 * pixel_scale * reduce[0]).ceil() as i32;
        let my = (3.0 * sigma[1] + 2.0 * pixel_scale * reduce[1]).ceil() as i32;
        let mut rect = IRect::new(
            region.x0 - mx,
            region.y0 - my,
            region.x1 + mx,
            region.y1 + my,
        );
        if let Some(f) = frame {
            rect = rect.intersect(&f.round_out().inflate(1));
        }
        if let Some(s) = support {
            rect = rect.intersect(&s.round_out());
        }
        rect
    };
    let mut rect = rect_for(reduce);
    let side = [
        f64::from(rect.width().max(1)),
        f64::from(rect.height().max(1)),
    ];
    if (0..2).any(|i| side[i] / (pixel_scale * reduce[i]) > MAX_TEXELS) {
        for i in 0..2 {
            reduce[i] = reduce[i].max((side[i] / (MAX_TEXELS * pixel_scale)).ceil());
        }
        rect = rect_for(reduce);
    }
    let empty = rect.is_empty();
    if empty {
        // Nothing to rasterise: a 1-texel buffer of zero coverage.
        rect = IRect::new(region.x0, region.y0, region.x0 + 1, region.y0 + 1);
    }
    let ps = [pixel_scale * reduce[0], pixel_scale * reduce[1]];
    let [tw, th] = texels_xy(rect, ps);
    let (ox, oy) = (f64::from(rect.x0), f64::from(rect.y0));
    let to_texel = |p: Point| [((p.x - ox) / ps[0]) as f32, ((p.y - oy) / ps[1]) as f32];

    let mut edges = Vec::new();
    let mut xmax = f32::NEG_INFINITY;
    if !empty {
        let path = to_bez_path(&m.shape);
        // Flatten to within 1/20 texel of the raster.
        let tolerance = (0.05 * ps[0].min(ps[1])).max(1e-4);
        let mut push = |el_path: &BezPath, stroke: bool| {
            let mut start = Point::ZERO;
            let mut last = Point::ZERO;
            let mut edge = |a: Point, b: Point| {
                let (p0, p1) = (to_texel(a), to_texel(b));
                if p0[1] == p1[1] {
                    return;
                }
                // Every edge counts for the right bound (edges right of the raster
                // still close the winding of the texels left of them).
                xmax = xmax.max(p0[0].max(p1[0]));
                let (ylo, yhi) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
                if yhi <= 0.0 || ylo >= th as f32 || p0[0].min(p1[0]) >= tw as f32 {
                    return;
                }
                edges.push(MaskEdge { p0, p1, stroke });
            };
            kurbo::flatten(el_path.iter(), tolerance, |el| match el {
                PathEl::MoveTo(p) => {
                    if last != start {
                        edge(last, start);
                    }
                    start = p;
                    last = p;
                }
                PathEl::LineTo(p) => {
                    edge(last, p);
                    last = p;
                }
                PathEl::ClosePath => {
                    if last != start {
                        edge(last, start);
                    }
                    last = start;
                }
                // flatten only emits lines.
                PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
            });
            // Unclosed sub-paths are closed implicitly (fill semantics).
            if last != start {
                edge(last, start);
            }
        };
        push(&path, false);
        let e = f64::from(m.expansion);
        if e.abs() > 1e-6 {
            let outline = kurbo::stroke(
                path.iter(),
                &Stroke::new(2.0 * e.abs()),
                &StrokeOpts::default(),
                tolerance,
            );
            push(&outline, true);
        }
    }
    let expansion = if empty || m.expansion.abs() <= 1e-6 {
        Expansion::None
    } else if m.expansion > 0.0 {
        Expansion::Grow
    } else {
        Expansion::Shrink
    };
    let clip = frame.map(|f| {
        [
            ((f.x0 - ox) / ps[0]) as f32,
            ((f.y0 - oy) / ps[1]) as f32,
            ((f.x1 - ox) / ps[0]) as f32,
            ((f.y1 - oy) / ps[1]) as f32,
        ]
    });
    MaskRaster {
        mode: m.mode,
        value: mask_value(m),
        expansion,
        edges,
        xmax: xmax.clamp(-1.0, tw as f32),
        rect,
        pixel_scale: ps,
        sigma: [(sigma[0] / ps[0]) as f32, (sigma[1] / ps[1]) as f32],
        clip,
    }
}

/// Texels covering `rect` (pixels) at `pixel_scale` pixels per texel per axis.
pub fn texels_xy(rect: IRect, pixel_scale: [f64; 2]) -> [u32; 2] {
    let n = |v: u32, s: f64| ((f64::from(v) / s.max(1e-9)) - 1e-9).ceil().max(0.0) as u32;
    [
        n(rect.width(), pixel_scale[0]),
        n(rect.height(), pixel_scale[1]),
    ]
}

/// Normalised Gaussian kernel radius (taps each side) for `sigma` texels.
pub fn kernel_radius(sigma: f32) -> u32 {
    if sigma < 0.05 {
        0
    } else {
        (3.0 * sigma).ceil() as u32
    }
}

/// `1 / Σ w(i)` for the shader's weights `w(i) = exp(−i² / 2σ²)`, `|i| ≤ radius`.
pub fn kernel_norm(sigma: f32) -> f32 {
    let r = kernel_radius(sigma) as i32;
    if r == 0 {
        return 1.0;
    }
    let k = -0.5 / (sigma * sigma);
    let sum: f32 = (-r..=r).map(|i| ((i * i) as f32 * k).exp()).sum();
    1.0 / sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_path(x0: f64, y0: f64, x1: f64, y1: f64) -> BezierPath {
        BezierPath {
            vertices: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            in_tangents: vec![[0.0, 0.0]; 4],
            out_tangents: vec![[0.0, 0.0]; 4],
            closed: true,
            feather: Vec::new(),
        }
    }

    fn mask(mode: MaskMode, shape: BezierPath) -> RenderMask {
        RenderMask {
            path: PrimPath::new("/comps/c/layers/l/masks/m").unwrap(),
            shape,
            mode,
            inverted: false,
            opacity: 1.0,
            feather: [0.0, 0.0],
            expansion: 0.0,
        }
    }

    #[test]
    fn only_closed_masks_with_a_mode_count() {
        let mut open = rect_path(0.0, 0.0, 10.0, 10.0);
        open.closed = false;
        let masks = [
            mask(MaskMode::None, rect_path(0.0, 0.0, 10.0, 10.0)),
            mask(MaskMode::Add, open),
        ];
        assert!(MaskStack::new(&masks).is_none());
    }

    #[test]
    fn start_value_and_single_mask_value() {
        let r = rect_path(0.0, 0.0, 10.0, 10.0);
        for (mode, init, value) in [
            (MaskMode::Add, 0.0, [0.0, 1.0]),
            (MaskMode::Subtract, 1.0, [1.0, -1.0]),
            (MaskMode::Intersect, 1.0, [0.0, 1.0]),
            (MaskMode::Lighten, 0.0, [0.0, 1.0]),
            (MaskMode::Darken, 1.0, [0.0, 1.0]),
            (MaskMode::Difference, 0.0, [0.0, 1.0]),
        ] {
            let masks = [mask(mode, r.clone())];
            let s = MaskStack::new(&masks).unwrap();
            assert_eq!(s.init, init, "{mode:?}");
            assert_eq!(s.output_value(), value, "{mode:?}");
        }
        let mut inv = mask(MaskMode::Add, r);
        inv.inverted = true;
        inv.opacity = 0.5;
        let masks = [inv];
        assert_eq!(MaskStack::new(&masks).unwrap().output_value(), [0.5, -0.5]);
    }

    #[test]
    fn support_bounds_follow_the_modes() {
        let a = mask(MaskMode::Add, rect_path(0.0, 0.0, 10.0, 10.0));
        let b = mask(MaskMode::Add, rect_path(20.0, 0.0, 30.0, 10.0));
        let masks = [a.clone(), b.clone()];
        let s = MaskStack::new(&masks).unwrap().support().unwrap();
        assert_eq!((s.x0, s.x1), (-1.0, 31.0));

        let mut i = b.clone();
        i.mode = MaskMode::Intersect;
        let masks = [a.clone(), i];
        assert!(
            MaskStack::new(&masks)
                .unwrap()
                .support()
                .unwrap()
                .is_empty()
        );

        let mut sub = a.clone();
        sub.mode = MaskMode::Subtract;
        let masks = [sub];
        assert!(MaskStack::new(&masks).unwrap().support().is_none());

        let mut inv = a.clone();
        inv.inverted = true;
        let masks = [inv];
        assert!(MaskStack::new(&masks).unwrap().support().is_none());

        let mut f = a;
        f.feather = [10.0, 0.0];
        f.expansion = 5.0;
        let masks = [f];
        let s = MaskStack::new(&masks).unwrap().support().unwrap();
        let grow = 5.0 + 1.0 + 3.0 * 10.0 * FEATHER_TO_SIGMA;
        assert!((s.x0 + grow).abs() < 1e-6 && (s.y0 + 6.0).abs() < 1e-6);
    }

    #[test]
    fn raster_edges_are_in_texels_and_reduced_for_large_feathers() {
        let masks = [mask(MaskMode::Add, rect_path(10.0, 10.0, 90.0, 90.0))];
        let stack = MaskStack::new(&masks).unwrap();
        let node = stack.plan(
            &PrimPath::new("/c/l").unwrap(),
            IRect::new(0, 0, 100, 100),
            1.0,
            Some(Rect::from_size(100.0, 100.0)),
        );
        let r = &node.rasters[0];
        assert_eq!(r.pixel_scale, [1.0, 1.0]);
        assert_eq!(r.rect.x0, 9, "clipped to the mask's support");
        // Two vertical edges (the horizontal ones add nothing).
        assert_eq!(r.edges.len(), 2);
        assert_eq!(r.edges[0].p0, [81.0, 1.0]);

        let mut big = mask(MaskMode::Add, rect_path(10.0, 10.0, 90.0, 90.0));
        big.feather = [100.0, 100.0];
        let masks = [big];
        let node = MaskStack::new(&masks).unwrap().plan(
            &PrimPath::new("/c/l").unwrap(),
            IRect::new(0, 0, 100, 100),
            1.0,
            None,
        );
        let r = &node.rasters[0];
        assert_eq!(r.pixel_scale, [12.0, 12.0]);
        assert!(r.sigma[0] >= 3.0 && r.sigma[0] < 6.0, "{:?}", r.sigma);

        // Different feathers reduce each axis on its own.
        let mut wide = mask(MaskMode::Add, rect_path(10.0, 10.0, 90.0, 90.0));
        wide.feather = [100.0, 4.0];
        let masks = [wide];
        let node = MaskStack::new(&masks).unwrap().plan(
            &PrimPath::new("/c/l").unwrap(),
            IRect::new(0, 0, 100, 100),
            1.0,
            None,
        );
        assert_eq!(node.rasters[0].pixel_scale, [12.0, 1.0]);
    }

    #[test]
    fn expansion_adds_a_round_joined_outline() {
        let mut m = mask(MaskMode::Add, rect_path(10.0, 10.0, 30.0, 30.0));
        m.expansion = 4.0;
        let masks = [m];
        let node = MaskStack::new(&masks).unwrap().plan(
            &PrimPath::new("/c/l").unwrap(),
            IRect::new(0, 0, 40, 40),
            1.0,
            None,
        );
        let r = &node.rasters[0];
        assert_eq!(r.expansion, Expansion::Grow);
        assert!(r.edges.iter().filter(|e| e.stroke).count() > 8);
    }

    #[test]
    fn kernel_weights_sum_to_one() {
        for sigma in [0.5f32, 1.0, 3.3, 7.9] {
            let r = kernel_radius(sigma) as i32;
            let k = -0.5 / (sigma * sigma);
            let n = kernel_norm(sigma);
            let sum: f32 = (-r..=r).map(|i| ((i * i) as f32 * k).exp() * n).sum();
            assert!((sum - 1.0).abs() < 1e-5);
        }
        assert_eq!(kernel_radius(0.0), 0);
    }
}

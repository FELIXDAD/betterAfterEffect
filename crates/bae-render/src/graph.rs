//! The render graph: what to draw, in which order, and which pixels are needed.
//!
//! [`build`] turns a [`RenderComp`] plus a [`RenderRequest`] into a DAG of
//! [`Node`]s, children before parents (so `nodes` is already a valid execution
//! order). Node kinds follow AE's per-layer pipeline:
//!
//! ```text
//! layer content (solid / Source / nested Composite)
//!   → masks (Mask) → effects (Effect …) ───────────┐
//!                                                  ├→ LayerOp in the parent Composite:
//! track matte layer → matte Composite ─────────────┘   transform · opacity · matte · blend
//! ```
//!
//! Adjustment layers: the layers below are composited into a backdrop, the
//! backdrop runs through the adjustment layer's effects, and a [`MixNode`]
//! blends the result back by the adjustment layer's own coverage (its pixels'
//! alpha with masks, track matte and opacity). Layers above draw on top.
//!
//! Masks: a [`MaskNode`] computes the layer's combined mask in layer space. Without
//! effects the layer op samples it while drawing (no extra pass over the layer);
//! with effects a one-op layer-space Composite applies it before the first effect.
//! The masks' support (where the result can be non-zero) shrinks the layer's
//! bounds, so a small mask on a large layer draws only its own area.
//!
//! **ROI negotiation.** Every node has a `rect`: the part of its output that is
//! actually needed. The root composite needs the request ROI; a layer op needs the
//! part of the layer that lands inside its composite's rect (its destination
//! bounds); the inverse transform of that region is the layer content's ROI; each
//! effect maps its output ROI to the input ROI it needs ([`Effect::input_roi`]);
//! nested comps render only the requested part. Layers whose bounds miss the ROI
//! are culled, and so are layers whose (non-inverted) matte is empty.
//!
//! **Spaces and units.** Every node lives in a *space* (a comp's pixel space or a
//! layer's pixel space) measured in full-resolution pixels. Its output texels are
//! `pixel_scale` space pixels wide (2 at half resolution), so `rect` is in units of
//! texels of that space. Layer-op transforms stay in full-resolution pixels; the
//! executor combines them with the actual images' scale and placement.
//!
//! **3D layers and motion blur.** The stack is split into 3D groups
//! ([`crate::three_d::segments`], AE-CMP-406); the layers of a group are drawn
//! in the camera's depth order, cut at intersections
//! ([`crate::three_d::depth_order`]). A 3D layer's op carries its homography
//! through the camera ([`LayerOp::projection`]); bounds and ROI come from the
//! part of the layer in front of the near plane that lands in the canvas. A
//! motion-blurred layer's op carries its placement at every sample
//! ([`LayerOp::motion`]; 2D layers get AE's adaptive count): its bounds are the
//! union over the shutter, its content is rendered once.
//!
//! **Cache keys.** Each node carries `key`, a hash of (prim, time, every parameter
//! that affects its pixels, rect, scale) – the `(Prim, time, parameter hash)` key
//! of ARCHITECTURE §5. Identical subgraphs within a frame are shared.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bae_model::{PrimPath, Time};
use glam::{DAffine2, DVec2};
use rustc_hash::{FxHashMap, FxHasher};

use crate::blend::{BlendMode, TrackMatteMode};
use crate::camera::NEAR;
use crate::effect::{Effect, EffectLibrary, EffectParams, LayerDependency, LayerStage, MaskSelect};
use crate::error::RenderError;
use crate::frame_blend;
use crate::geom::{IRect, Rect, is_invertible, scale_affine};
use crate::input::{
    CustomContent, FootageRef, LayerContent, RenderComp, RenderEffect, RenderLayer, RenderMask,
};
use crate::light::Lighting;
use crate::mask::MaskStack;
pub use crate::mask::{Expansion, MaskEdge, MaskNode, MaskRaster};
use crate::request::RenderRequest;
use crate::source::{LayerSource, SourceRequest};
pub use crate::three_d::Homography;
use crate::three_d::{Plate, Segment, depth_order, points_bounds};

/// Maximum nesting of comps (and of mattes of mattes).
pub const MAX_DEPTH: usize = 64;

/// Index of a node in [`RenderGraph::nodes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    /// As an index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A built render graph.
#[derive(Debug)]
pub struct RenderGraph {
    /// Project depth for blend functions with depth-specific arithmetic.
    pub depth: crate::request::BitDepth,
    /// Nodes in execution order (inputs before their consumers).
    pub nodes: Vec<Node>,
    /// The node producing the requested frame.
    pub output: NodeId,
    /// Missing footage, effects and other rendering warnings for logs and the UI.
    pub warnings: Vec<String>,
    /// An effect reads another time of the comp: the frame depends on more than
    /// its own evaluated comp (not frame-cached).
    pub time_dependent: bool,
    /// The top-level layer each node was planned for, by node index (`None`:
    /// the comp itself). Per-layer render times add up their nodes.
    pub owners: Vec<Option<PrimPath>>,
}

impl RenderGraph {
    /// The node with id `id`.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// The output node.
    pub fn output_node(&self) -> &Node {
        self.node(self.output)
    }

    /// How many times each node's output is read (inputs, mattes, effect inputs).
    pub fn use_counts(&self) -> Vec<u32> {
        let mut counts = vec![0u32; self.nodes.len()];
        for node in &self.nodes {
            node.kind.for_each_input(|id| counts[id.index()] += 1);
        }
        counts
    }

    /// Number of layer draws over all composites.
    pub fn draw_count(&self) -> usize {
        self.nodes
            .iter()
            .map(|n| match &n.kind {
                NodeKind::Composite(c) => c.ops.len(),
                _ => 0,
            })
            .sum()
    }
}

/// One node of the graph.
#[derive(Debug)]
pub struct Node {
    /// What the node does.
    pub kind: NodeKind,
    /// Needed output region, in texels of the node's space (see module docs).
    pub rect: IRect,
    /// Space pixels per output texel.
    pub pixel_scale: f64,
    /// Cache key.
    pub key: u64,
    /// Whether the result is worth keeping across frames (nested comps, effects).
    pub cacheable: bool,
}

/// Node kinds.
#[derive(Debug)]
pub enum NodeKind {
    /// Layers composited onto transparent black: a comp, a nested comp or a matte.
    Composite(CompositeNode),
    /// A solid colour rectangle rendered into an image (only when an effect or
    /// mask needs the solid as an image; plain solids are drawn directly).
    Fill(FillNode),
    /// Pixels from a [`LayerSource`] (footage, shapes, text, generators).
    Source(SourceNode),
    /// An effect applied to its input.
    Effect(EffectNode),
    /// A layer's masks combined into one coverage image (layer space).
    Mask(MaskNode),
    /// An adjustment layer's result mixed into its backdrop.
    Mix(MixNode),
}

/// `base + (over − base) · coverage.alpha`, per pixel (all three in the
/// node's space; pixels outside an input's rect read as transparent).
#[derive(Debug)]
pub struct MixNode {
    /// The layer (diagnostics).
    pub layer: PrimPath,
    /// The backdrop: the layers below the adjustment layer.
    pub base: NodeId,
    /// The backdrop after the adjustment layer's effects.
    pub over: NodeId,
    /// The adjustment layer's own pixels (only alpha is read).
    pub coverage: NodeId,
}

impl NodeKind {
    /// Calls `f` for every node this one reads.
    pub fn for_each_input(&self, mut f: impl FnMut(NodeId)) {
        match self {
            NodeKind::Composite(c) => {
                for op in &c.ops {
                    if let OpInput::Image(id) = op.input {
                        f(id);
                    }
                    if let Some(m) = &op.matte {
                        f(m.node);
                    }
                    if let Some(m) = &op.mask {
                        f(m.node);
                    }
                }
            }
            NodeKind::Effect(e) => {
                f(e.input);
                for (_, node) in &e.layers {
                    f(*node);
                }
                for (_, node) in &e.masks {
                    f(*node);
                }
            }
            NodeKind::Mix(m) => {
                f(m.base);
                f(m.over);
                f(m.coverage);
            }
            NodeKind::Fill(_) | NodeKind::Source(_) | NodeKind::Mask(_) => {}
        }
    }

    /// Short name for logs.
    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::Composite(_) => "composite",
            NodeKind::Fill(_) => "fill",
            NodeKind::Source(_) => "source",
            NodeKind::Effect(_) => "effect",
            NodeKind::Mask(_) => "mask",
            NodeKind::Mix(_) => "mix",
        }
    }
}

/// Composites `ops` (bottom layer first) onto transparent black.
#[derive(Debug)]
pub struct CompositeNode {
    /// The comp (or, for a matte composite, the matte layer).
    pub prim: PrimPath,
    /// Comp time.
    pub time: Time,
    /// Layer draws in order (bottom first).
    pub ops: Vec<LayerOp>,
    /// Clamp to `[0, 1]` after each blend (8/16-bpc semantics).
    pub clamp: bool,
}

/// One layer drawn into a composite: transform, opacity, matte and blend fused
/// into a single GPU draw.
#[derive(Clone, Debug)]
pub struct LayerOp {
    /// The layer.
    pub layer: PrimPath,
    /// What is drawn.
    pub input: OpInput,
    /// Input space pixels → composite space pixels (full resolution).
    pub transform: DAffine2,
    /// Opacity `[0, 1]`.
    pub opacity: f32,
    /// Layer blend mode.
    pub blend: BlendMode,
    /// Stable per-layer dissolve seed, mixed with the comp frame for Dancing Dissolve.
    pub dissolve_seed: u32,
    /// Only draw where the backdrop has alpha.
    pub preserve_transparency: bool,
    /// Track matte image (composite space, same units as the composite).
    pub matte: Option<MatteRef>,
    /// Mask image (input's layer space).
    pub mask: Option<MaskRef>,
    /// Destination region in composite texels (clipped to the composite rect).
    pub bounds: IRect,
    /// The layer's content bounds in input-space pixels (anti-aliased edges).
    pub content: Rect,
    /// 3D layers: input space pixels → composite space pixels as a homography
    /// through the camera (`transform` then only approximates it).
    pub projection: Option<Homography>,
    /// Motion blur: input space pixels → composite space pixels at every
    /// sample; the draw averages the layer over them.
    pub motion: Option<Arc<[Homography]>>,
    /// Pieces of 3D layers cut at intersections: half-planes `a·x + b·y + c ≥ 0`
    /// in input space pixels, and how far coverage reaches past each
    /// ([`crate::three_d::Fragment::clips`]).
    pub clips: Vec<[f64; 4]>,
    /// 3D layers lit by the comp's lights (AE-CMP-436).
    pub lighting: Option<Arc<Lighting>>,
}

/// The pixels of a layer op.
#[derive(Clone, Debug)]
pub enum OpInput {
    /// A solid rectangle drawn analytically (no texture).
    Solid {
        /// Straight RGBA.
        color: [f32; 4],
        /// Rectangle in layer pixels.
        rect: Rect,
    },
    /// The output image of another node.
    Image(NodeId),
}

/// A track matte input.
#[derive(Clone, Copy, Debug)]
pub struct MatteRef {
    /// The matte composite.
    pub node: NodeId,
    /// How the matte is read.
    pub mode: TrackMatteMode,
}

/// A layer's mask input.
#[derive(Clone, Copy, Debug)]
pub struct MaskRef {
    /// The [`MaskNode`].
    pub node: NodeId,
    /// The draw multiplies alpha by `value[0] + value[1] · sample`.
    pub value: [f32; 2],
}

/// A solid rendered into an image.
#[derive(Debug)]
pub struct FillNode {
    /// Straight RGBA.
    pub color: [f32; 4],
    /// Rectangle in layer pixels.
    pub rect: Rect,
}

/// A layer source invocation.
pub struct SourceNode {
    /// The layer.
    pub layer: PrimPath,
    /// The source.
    pub source: Arc<dyn LayerSource>,
    /// What to produce.
    pub request: SourceRequest,
}

impl std::fmt::Debug for SourceNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceNode")
            .field("layer", &self.layer)
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

/// An effect invocation.
pub struct EffectNode {
    /// The layer.
    pub layer: PrimPath,
    /// The implementation.
    pub effect: Arc<dyn Effect>,
    /// Evaluated parameters.
    pub instance: RenderEffect,
    /// Frame context for [`EffectParams`] (time, scale, bounds, rate, shutter …).
    pub frame: EffectFrame,
    /// Input node.
    pub input: NodeId,
    /// Layer images the effect reads ([`Effect::layer_dependencies`]).
    pub layers: Vec<(LayerDependency, NodeId)>,
    /// Mask coverage images it reads ([`Effect::mask_dependencies`]).
    pub masks: Vec<(MaskSelect, NodeId)>,
}

/// The frame context of an effect call, stored per effect node (everything in
/// [`EffectParams`] except the instance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectFrame {
    /// Layer time.
    pub time: Time,
    /// Comp time.
    pub comp_time: Time,
    /// Comp-time seconds per layer-time second.
    pub layer_time_scale: f64,
    /// Resolution scale.
    pub scale: f64,
    /// Layer content bounds (layer pixels, full resolution, before effects).
    pub layer_bounds: IRect,
    /// Comp frame rate.
    pub frame_rate: bae_model::FrameRate,
    /// One comp frame (time-stretched layers are not rescaled yet).
    pub time_step: Time,
    /// Linearized blending.
    pub linear: bool,
    /// Preview or final.
    pub purpose: crate::request::RenderPurpose,
    /// 32-bpc float semantics (no clamping between effects).
    pub float: bool,
    /// Shutter angle and phase (degrees).
    pub shutter: [f64; 2],
}

impl EffectFrame {
    /// The parameters of `instance` in this frame.
    pub fn params<'a>(&self, instance: &'a RenderEffect) -> EffectParams<'a> {
        EffectParams {
            time: self.time,
            comp_time: self.comp_time,
            layer_time_scale: self.layer_time_scale,
            scale: self.scale,
            layer_bounds: self.layer_bounds,
            frame_rate: self.frame_rate,
            time_step: self.time_step,
            linear: self.linear,
            purpose: self.purpose,
            float: self.float,
            shutter_angle: self.shutter[0],
            shutter_phase: self.shutter[1],
            ..EffectParams::new(instance)
        }
    }
}

impl std::fmt::Debug for EffectNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectNode")
            .field("layer", &self.layer)
            .field("effect", &self.instance.effect)
            .field("input", &self.input)
            .finish_non_exhaustive()
    }
}

/// Resolves footage and custom content into layer sources while building.
pub trait SourceResolver {
    /// The source of a footage frame; `Ok(None)` when the file is missing.
    fn footage(
        &mut self,
        footage: &FootageRef,
    ) -> Result<Option<Arc<dyn LayerSource>>, RenderError>;

    /// The source of custom content.
    fn custom(&mut self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError>;
}

/// A resolver without footage or providers (solids and nested comps only).
#[derive(Debug, Default)]
pub struct NoSources;

impl SourceResolver for NoSources {
    fn footage(&mut self, _: &FootageRef) -> Result<Option<Arc<dyn LayerSource>>, RenderError> {
        Ok(None)
    }

    fn custom(&mut self, content: &CustomContent) -> Result<Arc<dyn LayerSource>, RenderError> {
        Err(RenderError::MissingProvider(content.provider.clone()))
    }
}

/// The comp evaluated at other times, for effects that read other frames
/// ([`LayerDependency::time`]); `crate::eval::EvaluatorFrames` evaluates the stage.
pub trait CompFrames {
    /// `comp` evaluated at comp time `time` (`None`: not available).
    fn comp_at(&mut self, comp: &PrimPath, time: Time) -> Result<Option<RenderComp>, RenderError>;
}

/// Builds the graph for `comp` under `request`.
pub fn build(
    comp: &RenderComp,
    request: &RenderRequest,
    sources: &mut dyn SourceResolver,
    effects: &EffectLibrary,
) -> Result<RenderGraph, RenderError> {
    build_with(comp, request, sources, effects, None)
}

/// [`build`] with other frames of the comp available to effects.
pub fn build_with(
    comp: &RenderComp,
    request: &RenderRequest,
    sources: &mut dyn SourceResolver,
    effects: &EffectLibrary,
    frames: Option<&mut dyn CompFrames>,
) -> Result<RenderGraph, RenderError> {
    if comp.width == 0 || comp.height == 0 {
        return Err(RenderError::InvalidSize {
            path: comp.path.clone(),
            width: comp.width,
            height: comp.height,
        });
    }
    let scale = f64::from(request.resolution.factor());
    let [w, h] = request.resolution.scaled_size(comp.width, comp.height);
    let domain = IRect::from_size(w, h);
    let rect = match request.roi {
        Some(roi) => units(&roi.to_rect(), scale).intersect(&domain),
        None => domain,
    };
    let mut builder = Builder {
        sources,
        effects,
        linear: request.color.blend == crate::request::BlendSpace::Linear,
        purpose: request.purpose,
        clamp: request.depth.clamps(),
        nodes: Vec::new(),
        dedup: FxHashMap::default(),
        warnings: Vec::new(),
        skip_effects_of: None,
        frames,
        resolving: Vec::new(),
        time_dependent: false,
        owner: None,
        owners: Vec::new(),
    };
    let output = builder.composite(comp, rect, scale, 0, false)?;
    Ok(RenderGraph {
        depth: request.depth,
        nodes: builder.nodes,
        output,
        warnings: builder.warnings,
        time_dependent: builder.time_dependent,
        owners: builder.owners,
    })
}

/// Converts a rectangle in space pixels to texel units at `pixel_scale`.
pub fn units(rect: &Rect, pixel_scale: f64) -> IRect {
    Rect::new(
        rect.x0 / pixel_scale,
        rect.y0 / pixel_scale,
        rect.x1 / pixel_scale,
        rect.y1 / pixel_scale,
    )
    .round_out()
}

struct Builder<'a, 'f> {
    sources: &'a mut dyn SourceResolver,
    effects: &'a EffectLibrary,
    linear: bool,
    purpose: crate::request::RenderPurpose,
    clamp: bool,
    nodes: Vec<Node>,
    dedup: FxHashMap<u64, NodeId>,
    warnings: Vec<String>,
    /// While planning an adjustment layer's coverage: that layer's effects
    /// apply to the backdrop, not to its own pixels.
    skip_effects_of: Option<PrimPath>,
    /// Other frames of the comp (effects reading other times).
    frames: Option<&'f mut dyn CompFrames>,
    /// Layers whose effects are being planned (cycle detection of layer inputs).
    resolving: Vec<PrimPath>,
    time_dependent: bool,
    /// The top-level layer being planned (owner of the nodes pushed now).
    owner: Option<PrimPath>,
    owners: Vec<Option<PrimPath>>,
}

/// The content of a layer, planned: its bounds (in layer texel units) and how
/// to produce it.
enum Content {
    Solid {
        color: [f32; 4],
        rect: Rect,
    },
    Fill {
        color: [f32; 4],
        rect: Rect,
    },
    Source(Arc<dyn LayerSource>, Time),
    /// Frame blending: source frames (start times) and their weights.
    Mix(Arc<dyn LayerSource>, Vec<(Time, f32)>),
    Comp(Arc<RenderComp>),
}

impl Builder<'_, '_> {
    fn push(&mut self, node: Node) -> NodeId {
        if let Some(&id) = self.dedup.get(&node.key) {
            return id;
        }
        let id = NodeId(self.nodes.len() as u32);
        self.dedup.insert(node.key, id);
        self.nodes.push(node);
        self.owners.push(self.owner.clone());
        id
    }

    /// Runs `plan` with the top-level layer `index` of `comp` as the owner of
    /// the nodes it pushes (nested layers keep the outer owner).
    fn owned<T>(
        &mut self,
        comp: &RenderComp,
        index: usize,
        depth: usize,
        plan: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let top = depth == 0 && self.owner.is_none();
        if top {
            self.owner = Some(comp.layers[index].path.clone());
        }
        let out = plan(self);
        if top {
            self.owner = None;
        }
        out
    }

    fn warn(&mut self, message: String) {
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    fn composite(
        &mut self,
        comp: &RenderComp,
        rect: IRect,
        scale: f64,
        depth: usize,
        cacheable: bool,
    ) -> Result<NodeId, RenderError> {
        if depth > MAX_DEPTH {
            return Err(RenderError::Cycle {
                path: comp.path.clone(),
                max: MAX_DEPTH,
            });
        }
        let mut ops = Vec::new();
        if !rect.is_empty() {
            // AE draws from the bottom of the stack (highest index) to the top;
            // contiguous 3D layers form groups drawn in depth order.
            for segment in crate::three_d::segments(&comp.layers) {
                let index = match segment {
                    Segment::Group(indices) => {
                        self.group_3d(comp, &indices, rect, scale, depth, &mut ops)?;
                        continue;
                    }
                    Segment::Layer(index) => index,
                };
                let layer = &comp.layers[index];
                if layer.adjustment {
                    ops = self.adjustment(comp, index, rect, scale, depth, ops)?;
                    continue;
                }
                let mut visiting = vec![index];
                if let Some(op) = self.layer_op(comp, index, rect, scale, depth, &mut visiting)? {
                    ops.push(op);
                } else if layer.blend_mode.is_stencil()
                    && !matches!(layer.content, LayerContent::None)
                {
                    // A stencil with no coverage clears the accumulated layers.
                    ops.clear();
                }
            }
        }
        let mut h = FxHasher::default();
        "composite".hash(&mut h);
        comp.hash_content(&mut h);
        rect.hash(&mut h);
        scale.to_bits().hash(&mut h);
        self.clamp.hash(&mut h);
        Ok(self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: comp.path.clone(),
                time: comp.time,
                ops,
                clamp: self.clamp,
            }),
            rect,
            pixel_scale: scale,
            key: h.finish(),
            cacheable,
        }))
    }

    /// Plans the 3D group `indices` (bottom first) of `comp` into `ops`: every
    /// layer seen through the camera, drawn back to front in depth order and cut
    /// where layers intersect (AE-CMP-404/405). Coplanar layers keep the stacking
    /// order.
    fn group_3d(
        &mut self,
        comp: &RenderComp,
        indices: &[usize],
        canvas: IRect,
        scale: f64,
        depth: usize,
        ops: &mut Vec<LayerOp>,
    ) -> Result<(), RenderError> {
        let mut planned: Vec<(usize, LayerOp)> = Vec::with_capacity(indices.len());
        for &index in indices {
            let layer = &comp.layers[index];
            let mut visiting = vec![index];
            match self.layer_op(comp, index, canvas, scale, depth, &mut visiting)? {
                Some(op) => planned.push((index, op)),
                None if layer.blend_mode.is_stencil()
                    && !matches!(layer.content, LayerContent::None) =>
                {
                    ops.clear();
                    planned.clear();
                }
                None => {}
            }
        }
        attach_shadows(comp, &mut planned);
        if planned.len() <= 1 {
            ops.extend(planned.into_iter().map(|(_, op)| op));
            return Ok(());
        }
        // Layers whose drawn areas do not overlap cannot hide each other: sort
        // and cut only within clusters of overlapping layers (fewer cuts, and
        // no cut where nothing is in front).
        let n = planned.len();
        let mut cluster: Vec<usize> = (0..n).collect();
        fn root(c: &mut [usize], mut i: usize) -> usize {
            while c[i] != i {
                c[i] = c[c[i]];
                i = c[i];
            }
            i
        }
        for i in 0..n {
            for j in i + 1..n {
                if !planned[i]
                    .1
                    .bounds
                    .intersect(&planned[j].1.bounds)
                    .is_empty()
                {
                    let (a, b) = (root(&mut cluster, i), root(&mut cluster, j));
                    cluster[a.max(b)] = a.min(b);
                }
            }
        }
        let eye = comp.view_camera().eye();
        for first in 0..n {
            if root(&mut cluster, first) != first {
                continue;
            }
            let members: Vec<usize> = (first..n)
                .filter(|&i| root(&mut cluster, i) == first)
                .collect();
            if members.len() == 1 {
                ops.push(planned[first].1.clone());
                continue;
            }
            let plates: Vec<Plate> = members
                .iter()
                .map(|&i| Plate {
                    matrix: comp.layers[planned[i].0].matrix_3d.unwrap_or_default(),
                    rect: planned[i].1.content,
                })
                .collect();
            for fragment in depth_order(&plates, eye) {
                let mut op = planned[members[fragment.item]].1.clone();
                op.clips = fragment.clips;
                ops.push(op);
            }
        }
        Ok(())
    }

    /// Plans layer `index` of `comp` drawn into a composite covering `canvas`.
    /// `None` when nothing of it is visible there.
    fn layer_op(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        visiting: &mut Vec<usize>,
    ) -> Result<Option<LayerOp>, RenderError> {
        self.owned(comp, index, depth, |b| {
            b.plan_layer(comp, index, canvas, scale, depth, visiting)
        })
    }

    fn plan_layer(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        visiting: &mut Vec<usize>,
    ) -> Result<Option<LayerOp>, RenderError> {
        let layer = &comp.layers[index];
        let flat = layer.matrix_3d.is_none() && layer.motion_samples.is_empty();
        if layer.opacity <= 0.0 || (flat && !is_invertible(&layer.transform)) {
            return Ok(None);
        }
        let Some(content) = self.plan_content(layer, layer.time)? else {
            return Ok(None);
        };
        let content_rect = match &content {
            Content::Solid { rect, .. } | Content::Fill { rect, .. } => *rect,
            Content::Source(src, _) | Content::Mix(src, _) => src.bounds().to_rect(),
            Content::Comp(c) => c.rect(),
        };
        let placement = placement(comp, layer, &content_rect, scale);
        if let Placement::General {
            frame,
            motion: None,
            ..
        } = &placement
            && !frame.is_invertible()
        {
            // Seen exactly edge-on.
            return Ok(None);
        }
        // Effects: bounds forward (in layer texel units).
        let effects = self.plan_effects(layer);
        let frame = EffectFrame {
            time: layer.time,
            comp_time: comp.time,
            layer_time_scale: layer.time_scale,
            scale: 1.0 / scale,
            layer_bounds: content_rect.round_out(),
            frame_rate: comp.frame_rate,
            time_step: comp.frame_rate.frame_duration(),
            linear: self.linear,
            purpose: self.purpose,
            float: !self.clamp,
            shutter: comp.shutter,
        };
        // Masks only remove pixels: where they can leave any bounds the layer.
        let masks = MaskStack::new(&layer.masks);
        let visible = match masks.as_ref().and_then(MaskStack::support) {
            Some(support) => crate::mask::intersect(&content_rect, &support),
            None => content_rect,
        };
        if visible.is_empty() {
            return Ok(None);
        }
        let mut bounds_chain = vec![units(&visible, scale)];
        for (effect, instance) in &effects {
            let params = frame.params(instance);
            let prev = *bounds_chain.last().unwrap_or(&IRect::EMPTY);
            bounds_chain.push(effect.output_bounds(&params, prev));
        }
        let layer_bounds = *bounds_chain.last().unwrap_or(&IRect::EMPTY);
        let content_px = if effects.is_empty() {
            content_rect
        } else {
            let r = layer_bounds.to_rect();
            Rect::new(r.x0 * scale, r.y0 * scale, r.x1 * scale, r.y1 * scale)
        };
        if layer_bounds.is_empty() {
            return Ok(None);
        }
        // Destination bounds in composite units, grown by the edge ramp: one
        // target texel, or one layer texel when the layer is magnified.
        let (bounds, general_roi, magnification) = match &placement {
            Placement::Affine(t) => {
                let to_units = scale_affine(1.0 / scale) * *t * scale_affine(scale);
                let magnification = crate::geom::axis_scales(&to_units).max_element();
                let margin = 1 + (magnification.min(1e4) / 2.0).ceil() as i32;
                let bounds = layer_bounds
                    .to_rect()
                    .transformed_bounds(&to_units)
                    .round_out()
                    .inflate(margin)
                    .intersect(&canvas);
                (bounds, None, magnification)
            }
            Placement::General {
                frame,
                motion,
                three_d,
            } => {
                // The part of the layer in front of the camera that lands in
                // the canvas, over every placement.
                let near = if *three_d { NEAR } else { 0.0 };
                let maps = motion.as_deref().unwrap_or(std::slice::from_ref(frame));
                let (lb, cr) = (layer_bounds.to_rect(), canvas.to_rect());
                let (mut dst, mut src, mut mag) = (None::<Rect>, None::<Rect>, 0.0f64);
                for h in maps {
                    let hu = h
                        .followed_by(&scale_affine(1.0 / scale))
                        .preceded_by(&scale_affine(scale));
                    let poly = hu.visible(&lb, &cr, near);
                    if poly.len() < 3 {
                        continue;
                    }
                    dst = Some(rect_union(dst, hu.image_bounds(&poly)));
                    src = Some(rect_union(src, points_bounds(&poly)));
                    mag = mag.max(hu.max_scale(&poly));
                }
                let (Some(dst), Some(src)) = (dst, src) else {
                    return Ok(None);
                };
                let margin = 1 + (mag.min(1e4) / 2.0).ceil() as i32;
                (
                    dst.round_out().inflate(margin).intersect(&canvas),
                    Some(src.round_out().inflate(1).intersect(&layer_bounds)),
                    mag,
                )
            }
        };
        if bounds.is_empty() {
            return Ok(None);
        }

        let matte = match layer.track_matte {
            None => None,
            Some(tm) => match self.matte(comp, tm.layer, bounds, scale, depth, visiting)? {
                Some(node) => Some(MatteRef {
                    node,
                    mode: tm.mode,
                }),
                // An empty matte hides the layer, an empty inverted matte keeps all of it.
                None if tm.mode.is_inverted() => None,
                None => return Ok(None),
            },
        };

        // Content ROI: the destination bounds mapped back into layer units, +1
        // texel for the bilinear footprint, then backwards through the effects.
        let mut roi = match (general_roi, &placement) {
            (Some(roi), _) => roi,
            (None, Placement::Affine(t)) => {
                let to_units = scale_affine(1.0 / scale) * *t * scale_affine(scale);
                bounds
                    .to_rect()
                    .transformed_bounds(&to_units.inverse())
                    .round_out()
                    .inflate(1)
                    .intersect(&layer_bounds)
            }
            (None, Placement::General { .. }) => unreachable!("general placements plan their ROI"),
        };
        if roi.is_empty() {
            return Ok(None);
        }
        let mut rois = vec![roi; effects.len() + 1];
        for (i, (effect, instance)) in effects.iter().enumerate().rev() {
            let params = frame.params(instance);
            roi = effect.required_input(&params, roi, bounds_chain[i]);
            rois[i] = roi;
        }

        let has_effects = !effects.is_empty();
        let roi_px = Rect::new(
            f64::from(roi.x0) * scale,
            f64::from(roi.y0) * scale,
            f64::from(roi.x1) * scale,
            f64::from(roi.y1) * scale,
        )
        .round_out();
        // Texels per layer pixel the content should be rendered at.
        let density = match &placement {
            Placement::Affine(t) => source_density(t, scale, roi_px),
            Placement::General { .. } => clamp_density(magnification / scale, scale, roi_px),
        };
        let mask = masks
            .as_ref()
            .map(|stack| self.mask(layer, stack, roi, scale, has_effects, content_rect, density));

        let mut input = match content {
            Content::Solid { color, rect } => OpInput::Solid { color, rect },
            // The masked layer-space composite draws the solid itself.
            Content::Fill { color, rect } if mask.is_some() => OpInput::Solid { color, rect },
            Content::Fill { color, rect } => {
                let key = hash_of(&(
                    "fill",
                    color.map(f32::to_bits),
                    rect_bits(&rect),
                    roi,
                    scale.to_bits(),
                ));
                OpInput::Image(self.push(Node {
                    kind: NodeKind::Fill(FillNode { color, rect }),
                    rect: roi,
                    pixel_scale: scale,
                    key,
                    cacheable: false,
                }))
            }
            Content::Source(source, time) => {
                OpInput::Image(self.source_node(layer, &source, time, roi, roi_px, density, scale))
            }
            Content::Mix(source, frames) => {
                OpInput::Image(
                    self.mix(layer, &frames, roi, scale, content_rect, |b, time| {
                        b.source_node(layer, &source, time, roi, roi_px, density, scale)
                    }),
                )
            }
            Content::Comp(nested) => {
                OpInput::Image(self.composite(&nested, roi, scale, depth + 1, true)?)
            }
        };
        let mut op_mask = mask;
        if let Some(m) = mask
            && has_effects
        {
            input = self.masked(layer, input, m, roi, scale, content_rect);
            op_mask = None;
        }
        self.resolving.push(layer.path.clone());
        for (i, (effect, instance)) in effects.into_iter().enumerate() {
            let OpInput::Image(input_id) = input else {
                unreachable!("solids with effects are planned as fills")
            };
            let node = self.effect_node(
                comp,
                layer,
                effect,
                instance,
                frame,
                input_id,
                rois[i + 1],
                scale,
                depth,
            );
            input = OpInput::Image(match node {
                Ok(node) => node,
                Err(e) => {
                    self.resolving.pop();
                    return Err(e);
                }
            });
        }
        self.resolving.pop();

        // 3D layers that accept lights, reached by a light (AE-CMP-436): a
        // light with its Adjustment Layer switch on lights only the layers
        // below it (AE-CMP-435), and a layer no light reaches stays unlit
        // (measured in AE 26.2.1).
        let lighting = layer
            .matrix_3d
            .filter(|_| layer.material.accepts_lights)
            .and_then(|matrix| {
                let lights: Arc<[_]> = comp
                    .lights
                    .iter()
                    .filter(|l| l.scope_below.is_none_or(|light| index > light))
                    .copied()
                    .collect();
                (!lights.is_empty()).then(|| {
                    let camera = comp.view_camera();
                    Arc::new(Lighting {
                        matrix,
                        material: layer.material,
                        lights,
                        eye: camera.eye(),
                        world_per_pixel: camera.world_per_pixel(),
                        casters: Arc::new([]),
                    })
                })
            });
        let (transform, projection, motion) = match placement {
            Placement::Affine(t) => (t, None, None),
            Placement::General {
                frame,
                motion,
                three_d,
            } => (
                layer.transform,
                three_d.then_some(frame),
                motion.map(Arc::from),
            ),
        };
        Ok(Some(LayerOp {
            layer: layer.path.clone(),
            input,
            transform,
            opacity: layer.opacity.clamp(0.0, 1.0),
            blend: layer.blend_mode,
            dissolve_seed: layer.blend_mode.dissolve_seed(layer.path.as_str(), comp),
            preserve_transparency: layer.preserve_transparency,
            matte,
            mask: op_mask,
            bounds,
            content: content_px,
            projection,
            motion,
            clips: Vec::new(),
            lighting,
        }))
    }

    /// Plans the mask node of `layer` for its content ROI `roi` (layer texels at
    /// `scale`). With effects the mask is computed at the effects' resolution;
    /// otherwise at the layer's density on screen, so mask edges stay sharp when
    /// the layer is scaled up (and cheap when it is scaled down).
    #[allow(clippy::too_many_arguments)]
    fn mask(
        &mut self,
        layer: &RenderLayer,
        stack: &MaskStack<'_>,
        roi: IRect,
        scale: f64,
        has_effects: bool,
        content: Rect,
        density: f64,
    ) -> MaskRef {
        let roi_px = Rect::new(
            f64::from(roi.x0) * scale,
            f64::from(roi.y0) * scale,
            f64::from(roi.x1) * scale,
            f64::from(roi.y1) * scale,
        )
        .round_out();
        let ps = if has_effects {
            scale
        } else {
            // Whole texels per layer pixel (or layer pixels per texel), so mask
            // texel edges fall on the layer's pixel grid.
            if density >= 1.0 {
                1.0 / (density - 1e-6).ceil()
            } else {
                (1.0 / density + 1e-6).floor().max(1.0)
            }
        };
        let region = roi_px.inflate((2.0 * ps).ceil() as i32);
        // Raster layers feather inside their frame only (AE-CMP-041); vector
        // content (shapes, text) is continuously rasterised and has no frame.
        let frame = (!matches!(layer.content, LayerContent::Custom(_))).then_some(content);
        let mut h = FxHasher::default();
        "mask".hash(&mut h);
        layer.path.hash(&mut h);
        stack.hash_into(&mut h);
        region.hash(&mut h);
        ps.to_bits().hash(&mut h);
        frame.map(|f| rect_bits(&f)).hash(&mut h);
        let key = h.finish();
        let value = stack.output_value();
        if let Some(&node) = self.dedup.get(&key) {
            return MaskRef { node, value };
        }
        let node = stack.plan(&layer.path, region, ps, frame);
        let pixel_scale = node.pixel_scale;
        let node = self.push(Node {
            kind: NodeKind::Mask(node),
            rect: units(&region.to_rect(), pixel_scale),
            pixel_scale,
            key,
            cacheable: true,
        });
        MaskRef { node, value }
    }

    /// `input` with `mask` applied, as a layer-space image covering `roi` (the
    /// input of the layer's first effect).
    /// The source's pixels at `time` over `roi_px` at `density` texels per
    /// layer pixel.
    #[allow(clippy::too_many_arguments)]
    fn source_node(
        &mut self,
        layer: &RenderLayer,
        source: &Arc<dyn LayerSource>,
        time: Time,
        roi: IRect,
        roi_px: IRect,
        density: f64,
        scale: f64,
    ) -> NodeId {
        let mut h = FxHasher::default();
        "source".hash(&mut h);
        layer.path.hash(&mut h);
        source.content_hash(time).hash(&mut h);
        roi.hash(&mut h);
        density.to_bits().hash(&mut h);
        // Small sources are asked for whole: the request then stays the same
        // while the layer turns or slides off screen, so a vector source can
        // reuse the image it rasterized (the compositor uses only what it needs).
        let bounds = source.bounds();
        let texels = f64::from(bounds.width()) * f64::from(bounds.height()) * density * density;
        let request = if !bounds.is_empty() && texels <= WHOLE_SOURCE_TEXELS {
            bounds
        } else {
            roi_px
        };
        self.push(Node {
            kind: NodeKind::Source(SourceNode {
                layer: layer.path.clone(),
                source: source.clone(),
                request: SourceRequest {
                    roi: request,
                    scale: density,
                    time,
                },
            }),
            rect: roi,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: false,
        })
    }

    /// The whole source at `time` as an effect input (`roi` in units of `scale`).
    fn effect_source(
        &mut self,
        layer: &RenderLayer,
        source: &Arc<dyn LayerSource>,
        time: Time,
        roi: IRect,
        scale: f64,
    ) -> NodeId {
        let bounds = source.bounds();
        self.push(Node {
            rect: roi,
            kind: NodeKind::Source(SourceNode {
                layer: layer.path.clone(),
                request: SourceRequest {
                    roi: bounds,
                    scale: 1.0 / scale,
                    time,
                },
                source: source.clone(),
            }),
            pixel_scale: scale,
            key: hash_of(&(
                "effect source",
                &layer.path,
                source.content_hash(time),
                bounds,
                scale.to_bits(),
            )),
            cacheable: false,
        })
    }

    /// Frame blending: the frames `node` makes for each time in `frames`, summed
    /// with their weights (Alpha Add of premultiplied pixels, exact while the
    /// weights sum to 1).
    fn mix(
        &mut self,
        layer: &RenderLayer,
        frames: &[(Time, f32)],
        roi: IRect,
        scale: f64,
        content: Rect,
        mut node: impl FnMut(&mut Self, Time) -> NodeId,
    ) -> NodeId {
        let mut h = FxHasher::default();
        "mix".hash(&mut h);
        let mut ops = Vec::with_capacity(frames.len());
        for &(time, weight) in frames {
            let id = node(self, time);
            self.nodes[id.index()].key.hash(&mut h);
            weight.to_bits().hash(&mut h);
            ops.push(LayerOp {
                layer: layer.path.clone(),
                input: OpInput::Image(id),
                transform: DAffine2::IDENTITY,
                opacity: weight,
                blend: BlendMode::AlphaAdd,
                dissolve_seed: 0,
                preserve_transparency: false,
                matte: None,
                mask: None,
                bounds: roi,
                content,
                projection: None,
                motion: None,
                clips: Vec::new(),
                lighting: None,
            });
        }
        roi.hash(&mut h);
        scale.to_bits().hash(&mut h);
        self.clamp.hash(&mut h);
        self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: layer.path.clone(),
                time: layer.time,
                ops,
                clamp: self.clamp,
            }),
            rect: roi,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: false,
        })
    }

    fn masked(
        &mut self,
        layer: &RenderLayer,
        input: OpInput,
        mask: MaskRef,
        roi: IRect,
        scale: f64,
        content: Rect,
    ) -> OpInput {
        let mut h = FxHasher::default();
        "masked".hash(&mut h);
        match &input {
            OpInput::Image(id) => self.nodes[id.index()].key.hash(&mut h),
            OpInput::Solid { color, rect } => {
                color.map(f32::to_bits).hash(&mut h);
                rect_bits(rect).hash(&mut h);
            }
        }
        self.nodes[mask.node.index()].key.hash(&mut h);
        mask.value.map(f32::to_bits).hash(&mut h);
        roi.hash(&mut h);
        scale.to_bits().hash(&mut h);
        self.clamp.hash(&mut h);
        let op = LayerOp {
            layer: layer.path.clone(),
            input,
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend: BlendMode::Normal,
            dissolve_seed: 0,
            preserve_transparency: false,
            matte: None,
            mask: Some(mask),
            bounds: roi,
            content,
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
        };
        OpInput::Image(self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: layer.path.clone(),
                time: layer.time,
                ops: vec![op],
                clamp: self.clamp,
            }),
            rect: roi,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: false,
        }))
    }

    /// The matte composite for matte layer `index`, covering at most `needed`.
    fn matte(
        &mut self,
        comp: &RenderComp,
        index: usize,
        needed: IRect,
        scale: f64,
        depth: usize,
        visiting: &mut Vec<usize>,
    ) -> Result<Option<NodeId>, RenderError> {
        let Some(matte_layer) = comp.layers.get(index) else {
            self.warn(format!(
                "{}: track matte layer {index} does not exist",
                comp.path
            ));
            return Ok(None);
        };
        if visiting.contains(&index) || visiting.len() > MAX_DEPTH {
            self.warn(format!("{}: track matte cycle", matte_layer.path));
            return Ok(None);
        }
        // An inactive matte layer (outside its in/out points) contributes nothing;
        // its video switch does not matter.
        if !matte_layer.active {
            return Ok(None);
        }
        visiting.push(index);
        let op = self.layer_op(comp, index, needed, scale, depth, visiting)?;
        visiting.pop();
        let Some(mut op) = op else {
            return Ok(None);
        };
        // The matte is the matte layer's own pixels (with its opacity, effects and
        // matte); its blend mode does not apply.
        op.blend = BlendMode::Normal;
        op.preserve_transparency = false;
        let rect = op.bounds;
        let mut h = FxHasher::default();
        "matte".hash(&mut h);
        matte_layer.path.hash(&mut h);
        matte_layer.content_hash().hash(&mut h);
        for v in matte_layer.transform.to_cols_array() {
            v.to_bits().hash(&mut h);
        }
        matte_layer.opacity.to_bits().hash(&mut h);
        matte_layer.track_matte.hash(&mut h);
        if let Some(m) = &matte_layer.matrix_3d {
            m.to_cols_array().map(f64::to_bits).hash(&mut h);
        }
        matte_layer.motion_samples.len().hash(&mut h);
        rect.hash(&mut h);
        scale.to_bits().hash(&mut h);
        Ok(Some(self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: matte_layer.path.clone(),
                time: comp.time,
                ops: vec![op],
                clamp: self.clamp,
            }),
            rect,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: false,
        })))
    }

    // ---- Adjustment layers ---------------------------------------------------

    /// Adjustment layer `index` of `comp` applied to the layers below it
    /// (`below`, drawn into a composite covering `canvas`). Returns the ops
    /// that replace `below`: one draw of the adjusted backdrop.
    ///
    /// The effects see the backdrop in comp space (AE renders adjustment
    /// effects on the comp-sized composite below; outside the comp it is
    /// transparent). Without enabled effects the layer changes nothing.
    fn adjustment(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        below: Vec<LayerOp>,
    ) -> Result<Vec<LayerOp>, RenderError> {
        self.owned(comp, index, depth, |b| {
            b.plan_adjustment(comp, index, canvas, scale, depth, below)
        })
    }

    fn plan_adjustment(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        below: Vec<LayerOp>,
    ) -> Result<Vec<LayerOp>, RenderError> {
        let layer = &comp.layers[index];
        let effects = self.plan_effects(layer);
        if effects.is_empty() || canvas.is_empty() || layer.opacity <= 0.0 {
            return Ok(below);
        }
        if layer.blend_mode.effective() != BlendMode::Normal {
            self.warn(format!(
                "{}: adjustment layers blend in Normal mode for now (`{}`)",
                layer.path, layer.blend_mode
            ));
        }
        // Coverage: the layer's own pixels through its transform, masks,
        // matte and opacity, without its effects.
        self.skip_effects_of = Some(layer.path.clone());
        let mut visiting = vec![index];
        let coverage = self.layer_op(comp, index, canvas, scale, depth, &mut visiting);
        self.skip_effects_of = None;
        let coverage = match coverage? {
            // No pixels of its own (no source): the whole comp.
            None if matches!(layer.content, LayerContent::None) => None,
            // Its pixels miss the canvas: nothing to adjust here.
            None => return Ok(below),
            Some(op) if covers(&op, canvas, scale) => None,
            Some(mut op) => {
                op.blend = BlendMode::Normal;
                op.preserve_transparency = false;
                Some(op)
            }
        };

        let comp_px = comp.rect();
        let domain = units(&comp_px, scale);
        let frame = EffectFrame {
            time: layer.time,
            comp_time: comp.time,
            layer_time_scale: layer.time_scale,
            scale: 1.0 / scale,
            layer_bounds: comp_px.round_out(),
            frame_rate: comp.frame_rate,
            time_step: comp.frame_rate.frame_duration(),
            linear: self.linear,
            purpose: self.purpose,
            float: !self.clamp,
            shutter: comp.shutter,
        };
        // Needed regions, backwards from the canvas through the effects.
        let mut rois = vec![canvas; effects.len() + 1];
        for i in (0..effects.len()).rev() {
            let params = frame.params(&effects[i].1);
            rois[i] = effects[i].0.required_input(&params, rois[i + 1], domain);
        }
        let base_rect = rois[0].union(&canvas).intersect(&domain);

        let mut h = FxHasher::default();
        "adjustment base".hash(&mut h);
        comp.hash_content(&mut h);
        index.hash(&mut h);
        base_rect.hash(&mut h);
        scale.to_bits().hash(&mut h);
        self.clamp.hash(&mut h);
        let base = self.push(Node {
            kind: NodeKind::Composite(CompositeNode {
                prim: comp.path.clone(),
                time: comp.time,
                ops: below,
                clamp: self.clamp,
            }),
            rect: base_rect,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: false,
        });
        let mut over = base;
        for (i, (effect, instance)) in effects.into_iter().enumerate() {
            over = self.effect_node(
                comp,
                layer,
                effect,
                instance,
                frame,
                over,
                rois[i + 1],
                scale,
                depth,
            )?;
        }
        let out = match coverage {
            None => over,
            Some(op) => {
                let mut h = FxHasher::default();
                "adjustment coverage".hash(&mut h);
                comp.hash_content(&mut h);
                index.hash(&mut h);
                canvas.hash(&mut h);
                scale.to_bits().hash(&mut h);
                let cover = self.push(Node {
                    kind: NodeKind::Composite(CompositeNode {
                        prim: layer.path.clone(),
                        time: comp.time,
                        ops: vec![op],
                        clamp: self.clamp,
                    }),
                    rect: canvas,
                    pixel_scale: scale,
                    key: h.finish(),
                    cacheable: false,
                });
                let key = hash_of(&(
                    "adjustment mix",
                    self.nodes[base.index()].key,
                    self.nodes[over.index()].key,
                    self.nodes[cover.index()].key,
                    canvas,
                ));
                self.push(Node {
                    kind: NodeKind::Mix(MixNode {
                        layer: layer.path.clone(),
                        base,
                        over,
                        coverage: cover,
                    }),
                    rect: canvas,
                    pixel_scale: scale,
                    key,
                    cacheable: false,
                })
            }
        };
        let canvas_px = Rect::new(
            f64::from(canvas.x0) * scale,
            f64::from(canvas.y0) * scale,
            f64::from(canvas.x1) * scale,
            f64::from(canvas.y1) * scale,
        );
        Ok(vec![LayerOp {
            layer: layer.path.clone(),
            input: OpInput::Image(out),
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend: BlendMode::Normal,
            dissolve_seed: 0,
            preserve_transparency: false,
            matte: None,
            mask: None,
            bounds: canvas,
            content: canvas_px,
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
        }])
    }

    /// One effect of `layer` applied to `input`, covering `rect`, with the
    /// layer and mask images it reads planned before it.
    #[allow(clippy::too_many_arguments)]
    fn effect_node(
        &mut self,
        comp: &RenderComp,
        layer: &RenderLayer,
        effect: Arc<dyn Effect>,
        instance: RenderEffect,
        frame: EffectFrame,
        input: NodeId,
        rect: IRect,
        scale: f64,
        depth: usize,
    ) -> Result<NodeId, RenderError> {
        let mut h = FxHasher::default();
        "effect".hash(&mut h);
        self.nodes[input.index()].key.hash(&mut h);
        // Effects may vary with time on their own (noise evolution, wiggle).
        layer.time.hash(&mut h);
        crate::input::hash_value(&bae_model::Value::String(instance.effect.clone()), &mut h);
        for (id, value) in &instance.params {
            id.hash(&mut h);
            crate::input::hash_value(value, &mut h);
        }
        rect.hash(&mut h);
        let params = frame.params(&instance);
        let mut layers = Vec::new();
        for dep in effect.layer_dependencies(&params) {
            if let Some(node) = self.layer_input(comp, &dep, scale, depth)? {
                dep.hash(&mut h);
                self.nodes[node.index()].key.hash(&mut h);
                layers.push((dep, node));
            }
        }
        let mut masks = Vec::new();
        for select in effect.mask_dependencies(&params) {
            if let Some(node) = self.mask_input(layer, &select, frame.layer_bounds, scale) {
                select.hash(&mut h);
                self.nodes[node.index()].key.hash(&mut h);
                masks.push((select, node));
            }
        }
        Ok(self.push(Node {
            kind: NodeKind::Effect(EffectNode {
                layer: layer.path.clone(),
                effect,
                instance,
                frame,
                input,
                layers,
                masks,
            }),
            rect,
            pixel_scale: scale,
            key: h.finish(),
            cacheable: true,
        }))
    }

    /// The image `dep` asks for: the layer at that comp time (another frame
    /// through [`CompFrames`]), at the requested stage, in its layer space.
    fn layer_input(
        &mut self,
        comp: &RenderComp,
        dep: &LayerDependency,
        scale: f64,
        depth: usize,
    ) -> Result<Option<NodeId>, RenderError> {
        let other;
        let at = if dep.time == comp.time {
            comp
        } else {
            let found = match self.frames.as_mut() {
                Some(frames) => frames.comp_at(&comp.path, dep.time)?,
                None => None,
            };
            let Some(found) = found else {
                self.warn(format!(
                    "{}: other frames are not available here",
                    dep.layer
                ));
                return Ok(None);
            };
            self.time_dependent = true;
            other = found;
            &other
        };
        let Some(layer) = at.layers.iter().find(|l| l.path == dep.layer) else {
            return Ok(None);
        };
        if matches!(dep.stage, LayerStage::BeforeEffect(_)) && !layer.active {
            return Ok(None);
        }
        let mut stage = dep.stage.clone();
        if stage == LayerStage::EffectsAndMasks && self.resolving.contains(&dep.layer) {
            self.warn(format!(
                "{}: circular layer reference, using the layer with its masks",
                dep.layer
            ));
            stage = LayerStage::Masks;
        }
        self.layer_image(at, layer, stage, scale, depth)
    }

    /// `layer`'s pixels in its layer space at `stage` (its source, with masks,
    /// with masks and effects); `None` when it has none.
    fn layer_image(
        &mut self,
        comp: &RenderComp,
        layer: &RenderLayer,
        stage: LayerStage,
        scale: f64,
        depth: usize,
    ) -> Result<Option<NodeId>, RenderError> {
        let Some(content) = self.plan_content(layer, layer.time)? else {
            return Ok(None);
        };
        let content_rect = match &content {
            Content::Solid { rect, .. } | Content::Fill { rect, .. } => *rect,
            Content::Source(src, _) | Content::Mix(src, _) => src.bounds().to_rect(),
            Content::Comp(c) => c.rect(),
        };
        let roi = units(&content_rect, scale);
        let mut node = match content {
            Content::Solid { color, rect } | Content::Fill { color, rect } => self.push(Node {
                kind: NodeKind::Fill(FillNode { color, rect }),
                rect: roi,
                pixel_scale: scale,
                key: hash_of(&(
                    "fill",
                    color.map(f32::to_bits),
                    rect_bits(&rect),
                    roi,
                    scale.to_bits(),
                )),
                cacheable: false,
            }),
            Content::Source(source, time) => self.effect_source(layer, &source, time, roi, scale),
            Content::Mix(source, frames) => {
                self.mix(layer, &frames, roi, scale, content_rect, |b, time| {
                    b.effect_source(layer, &source, time, roi, scale)
                })
            }
            Content::Comp(nested) => self.composite(&nested, roi, scale, depth + 1, true)?,
        };
        if stage != LayerStage::Source
            && let Some(stack) = MaskStack::new(&layer.masks)
        {
            let mask = self.mask(layer, &stack, roi, scale, true, content_rect, 1.0 / scale);
            if let OpInput::Image(id) =
                self.masked(layer, OpInput::Image(node), mask, roi, scale, content_rect)
            {
                node = id;
            }
        }
        let effect_count = match stage {
            LayerStage::EffectsAndMasks => layer.effects.len(),
            LayerStage::BeforeEffect(path) => layer
                .effects
                .iter()
                .position(|e| e.path == path)
                .unwrap_or(0),
            _ => 0,
        };
        if effect_count != 0 {
            let effects = self.plan_effect_prefix(layer, effect_count);
            let frame = EffectFrame {
                time: layer.time,
                comp_time: comp.time,
                layer_time_scale: layer.time_scale,
                scale: 1.0 / scale,
                layer_bounds: content_rect.round_out(),
                frame_rate: comp.frame_rate,
                time_step: comp.frame_rate.frame_duration(),
                linear: self.linear,
                purpose: self.purpose,
                float: !self.clamp,
                shutter: comp.shutter,
            };
            let mut rect = roi;
            self.resolving.push(layer.path.clone());
            for (effect, instance) in effects {
                rect = effect.output_bounds(&frame.params(&instance), rect);
                let next = self.effect_node(
                    comp, layer, effect, instance, frame, node, rect, scale, depth,
                );
                match next {
                    Ok(next) => node = next,
                    Err(e) => {
                        self.resolving.pop();
                        return Err(e);
                    }
                }
            }
            self.resolving.pop();
        }
        Ok(Some(node))
    }

    /// The coverage of `select` masks of `layer` as a layer-space image
    /// (alpha), over its content bounds `bounds` (full-resolution pixels).
    fn mask_input(
        &mut self,
        layer: &RenderLayer,
        select: &MaskSelect,
        bounds: IRect,
        scale: f64,
    ) -> Option<NodeId> {
        let picked: Vec<RenderMask> = match select {
            MaskSelect::All => layer.masks.clone(),
            MaskSelect::One(path) => layer
                .masks
                .iter()
                .filter(|m| m.path == *path)
                .map(|m| RenderMask {
                    mode: crate::blend::MaskMode::Add,
                    ..m.clone()
                })
                .collect(),
        };
        let stack = MaskStack::new(&picked)?;
        let content = bounds.to_rect();
        let roi = units(&content, scale);
        let mask = self.mask(layer, &stack, roi, scale, true, content, 1.0 / scale);
        let white = OpInput::Solid {
            color: [1.0; 4],
            rect: content,
        };
        match self.masked(layer, white, mask, roi, scale, content) {
            OpInput::Image(id) => Some(id),
            OpInput::Solid { .. } => None,
        }
    }

    fn plan_content(
        &mut self,
        layer: &RenderLayer,
        time: Time,
    ) -> Result<Option<Content>, RenderError> {
        let has_effects = self.skip_effects_of.as_ref() != Some(&layer.path)
            && layer.effects.iter().any(|e| {
                e.enabled
                    && self
                        .effects
                        .get(&e.effect)
                        .is_some_and(|fx| fx.renders_pixels())
            });
        Ok(match &layer.content {
            LayerContent::None => None,
            LayerContent::Solid {
                color,
                width,
                height,
            } => {
                if *width == 0 || *height == 0 || (color[3] <= 0.0 && !has_effects) {
                    return Ok(None);
                }
                let rect = Rect::from_size(f64::from(*width), f64::from(*height));
                Some(if has_effects {
                    Content::Fill {
                        color: *color,
                        rect,
                    }
                } else {
                    Content::Solid {
                        color: *color,
                        rect,
                    }
                })
            }
            LayerContent::Footage(footage) => match self.sources.footage(footage)? {
                Some(source) => Some(match (footage.blend, source.frame_rate()) {
                    (Some(blend), Some(rate)) => {
                        if blend.pixel_motion {
                            self.warn(format!(
                                "{}: Pixel Motion frame blending is not available yet; Frame Mix is used",
                                layer.path
                            ));
                        }
                        let frames = frame_blend::weights(footage.source_time, blend.span, rate);
                        match frames.as_slice() {
                            [(time, _)] => Content::Source(source, *time),
                            _ => Content::Mix(source, frames),
                        }
                    }
                    _ => Content::Source(source, footage.source_time),
                }),
                None => {
                    self.warn(format!(
                        "{}: footage {} is missing",
                        layer.path,
                        footage
                            .file
                            .as_deref()
                            .map_or_else(|| footage.asset.to_string(), |p| p.display().to_string())
                    ));
                    None
                }
            },
            LayerContent::Comp(nested) => {
                if nested.width == 0 || nested.height == 0 {
                    return Ok(None);
                }
                Some(Content::Comp(nested.clone()))
            }
            LayerContent::Custom(custom) => match self.sources.custom(custom) {
                Ok(source) => Some(Content::Source(source, time)),
                Err(error) => {
                    self.warn(format!("{}: {error}", layer.path));
                    None
                }
            },
        })
    }

    fn plan_effects(&mut self, layer: &RenderLayer) -> Vec<(Arc<dyn Effect>, RenderEffect)> {
        self.plan_effect_prefix(layer, layer.effects.len())
    }

    fn plan_effect_prefix(
        &mut self,
        layer: &RenderLayer,
        count: usize,
    ) -> Vec<(Arc<dyn Effect>, RenderEffect)> {
        let mut out = Vec::new();
        if self.skip_effects_of.as_ref() == Some(&layer.path) {
            return out;
        }
        for instance in layer.effects.iter().take(count).filter(|e| e.enabled) {
            match self.effects.get(&instance.effect) {
                Some(effect) if effect.renders_pixels() => {
                    out.push((effect.clone(), instance.clone()))
                }
                Some(_) => {}
                None => self.warn(format!(
                    "{}: effect `{}` is not available, skipped",
                    layer.path, instance.effect
                )),
            }
        }
        out
    }
}

/// Whether `op` (an adjustment layer's coverage draw) is fully opaque over all
/// of `canvas` (composite texels at `scale`): an opaque solid, no mask or
/// matte, full opacity, and an axis-aligned transform whose image of the solid
/// contains the canvas. The mix is skipped then.
fn covers(op: &LayerOp, canvas: IRect, scale: f64) -> bool {
    let OpInput::Solid { color, rect } = &op.input else {
        return false;
    };
    if color[3] < 1.0
        || op.opacity < 1.0
        || op.mask.is_some()
        || op.matte.is_some()
        || op.projection.is_some()
        || op.motion.is_some()
        || op.lighting.is_some()
        || !op.clips.is_empty()
    {
        return false;
    }
    let t = op.transform;
    if t.matrix2.x_axis.y.abs() > 1e-9 || t.matrix2.y_axis.x.abs() > 1e-9 {
        return false;
    }
    let image = rect.transformed_bounds(&t);
    let need = canvas.to_rect();
    image.x0 <= need.x0 * scale
        && image.y0 <= need.y0 * scale
        && image.x1 >= need.x1 * scale
        && image.y1 >= need.y1 * scale
}

/// Texels per layer pixel a source should deliver: the layer's magnification on
/// screen (resolution × scale through the transform and parents), so vector and
/// text layers rasterize at their final size like AE's continuous rasterization.
/// Clamped to [1/64, 16] and to the GPU texture limit for `roi`.
pub fn source_density(transform: &DAffine2, resolution_scale: f64, roi: IRect) -> f64 {
    let to_texels = scale_affine(1.0 / resolution_scale) * *transform;
    clamp_density(
        crate::geom::axis_scales(&to_texels).max_element(),
        resolution_scale,
        roi,
    )
}

/// Sources up to this many texels are rendered whole (see the source node).
const WHOLE_SOURCE_TEXELS: f64 = 2048.0 * 2048.0;

/// `density` (texels per layer pixel) made usable: finite, within [1/64, 16]
/// and within the GPU texture limit for `roi` (layer pixels). Snapped to
/// 1/4096 so a layer that only turns asks for the same density every frame
/// (and its source can reuse the image it rendered).
fn clamp_density(mut density: f64, resolution_scale: f64, roi: IRect) -> f64 {
    if !density.is_finite() || density <= 0.0 {
        density = 1.0 / resolution_scale;
    }
    density = ((density * 4096.0).round() / 4096.0).clamp(1.0 / 64.0, 16.0);
    let largest = f64::from(roi.width().max(roi.height()).max(1));
    density.min(8192.0 / largest)
}

/// How a layer lands in its composite (comp pixels).
enum Placement {
    /// One affine map: 2D layers, and 3D layers facing the camera squarely.
    Affine(DAffine2),
    /// Through the camera (`three_d`) and/or at several motion-blur samples.
    General {
        /// The placement at the frame time.
        frame: Homography,
        /// The placements at the shutter samples (the draw averages them).
        motion: Option<Vec<Homography>>,
        /// Seen through the camera: clipped at the near plane.
        three_d: bool,
    },
}

/// The placement of `layer` (content `content`, layer pixels) in `comp` at
/// resolution `scale`: through the camera for 3D layers, and at every
/// motion-blur sample unless the layer does not move during the shutter.
fn placement(comp: &RenderComp, layer: &RenderLayer, content: &Rect, scale: f64) -> Placement {
    let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
    if let Some(m) = layer.matrix_3d {
        let frame = Homography(comp.view_camera().plane_homography(&m, size));
        let motion = (!layer.motion_samples.is_empty())
            .then(|| {
                layer
                    .motion_samples
                    .iter()
                    .enumerate()
                    .map(|(k, s)| {
                        let m = s.matrix_3d.unwrap_or(m);
                        Homography(comp.sample_camera(k).plane_homography(&m, size))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|hs| !all_equal(hs));
        // A plane facing the camera squarely projects affinely: draw it like a
        // 2D layer (pixel-identical to 2D at z = 0 through the default camera).
        if motion.is_none()
            && frame.0.row(2).z >= NEAR
            && let Some(a) = frame.as_affine()
        {
            return Placement::Affine(a);
        }
        return Placement::General {
            frame,
            motion,
            three_d: true,
        };
    }
    if layer.motion_samples.len() > 1 && !crate::motion::is_static(&layer.motion_samples) {
        let transforms: Vec<DAffine2> = layer.motion_samples.iter().map(|s| s.transform).collect();
        let limit = comp.motion_blur.as_ref().map_or(128, |m| m.adaptive_limit);
        let dense = crate::motion::adaptive_samples(&transforms, content, limit, scale);
        return Placement::General {
            frame: Homography::from_affine(&layer.transform),
            motion: Some(dense.iter().map(Homography::from_affine).collect()),
            three_d: false,
        };
    }
    Placement::Affine(layer.transform)
}

/// Shadows inside one 3D group (AE-CMP-437): every lit layer that accepts
/// shadows gets the other layers of the group that cast shadows, when one of
/// its lights casts shadows.
fn attach_shadows(comp: &RenderComp, planned: &mut [(usize, LayerOp)]) {
    use crate::light::{CasterPixels, ShadowCaster, ShadowMode};
    let casters: Vec<(usize, ShadowCaster)> = planned
        .iter()
        .filter(|(index, op)| {
            comp.layers[*index].material.casts_shadows != ShadowMode::Off && op.opacity > 0.0
        })
        .filter_map(|(index, op)| {
            let pixels = match &op.input {
                OpInput::Solid { color, .. } => CasterPixels::Solid(*color),
                OpInput::Image(id) => CasterPixels::Image(*id),
            };
            Some((
                *index,
                ShadowCaster {
                    matrix: comp.layers[*index].matrix_3d?,
                    rect: op.content,
                    opacity: op.opacity,
                    transmission: comp.layers[*index].material.light_transmission,
                    pixels,
                },
            ))
        })
        .collect();
    if casters.is_empty() {
        return;
    }
    for (index, op) in planned.iter_mut() {
        let Some(lighting) = &op.lighting else {
            continue;
        };
        if comp.layers[*index].material.accepts_shadows == ShadowMode::Off
            || !lighting.lights.iter().any(|l| l.casts_shadows)
        {
            continue;
        }
        let receiver = lighting.matrix;
        let rect = op.content;
        let mine: Arc<[ShadowCaster]> = casters
            .iter()
            .filter(|(i, c)| {
                i != index
                    && lighting
                        .lights
                        .iter()
                        .filter(|l| l.casts_shadows)
                        .any(|l| can_shadow(&receiver, &rect, c, l))
            })
            .map(|(_, c)| c.clone())
            .collect();
        if mine.is_empty() {
            continue;
        }
        op.lighting = Some(Arc::new(Lighting {
            casters: mine,
            ..(**lighting).clone()
        }));
    }
}

/// Whether `caster` can throw a shadow of `light` onto the receiver plane
/// `receiver` within `rect` (layer pixels). Conservative: only rules out casters
/// behind the receiver as seen from the light, or whose shadow lands outside it.
fn can_shadow(
    receiver: &glam::DMat4,
    rect: &Rect,
    caster: &crate::light::ShadowCaster,
    light: &crate::light::Light,
) -> bool {
    use crate::light::LightKind;
    let Some(n) = receiver
        .x_axis
        .truncate()
        .cross(receiver.y_axis.truncate())
        .try_normalize()
    else {
        return true;
    };
    let d = -n.dot(receiver.w_axis.truncate());
    let side = |p: glam::DVec3| n.dot(p) + d;
    let corners = caster
        .rect
        .corners()
        .map(|c| caster.matrix.transform_point3(c.extend(0.0)));
    // Where each corner's shadow falls on the receiver plane.
    let shadow_of = |c: glam::DVec3| -> Option<glam::DVec3> {
        match light.kind {
            LightKind::Parallel => {
                let dir = light.direction();
                let toward = -n.dot(dir) * side(c);
                (toward > 1e-9).then(|| c - dir * (side(c) / n.dot(dir)))
            }
            _ => {
                let lp = light.position();
                let (sl, sc) = (side(lp), side(c));
                // Strictly between the light and the receiver plane.
                (sl * sc > 1e-9 && sc.abs() < sl.abs()).then(|| lp + (c - lp) * (sl / (sl - sc)))
            }
        }
    };
    let light_side = match light.kind {
        LightKind::Parallel => -n.dot(light.direction()),
        _ => side(light.position()),
    };
    if corners.iter().all(|c| side(*c) * light_side <= 0.0) {
        // Behind the receiver (or in its plane) as seen from the light.
        return false;
    }
    let Some(shadows) = corners
        .iter()
        .map(|c| shadow_of(*c))
        .collect::<Option<Vec<_>>>()
    else {
        return true;
    };
    let to_layer = receiver.inverse();
    let pts: Vec<glam::DVec2> = shadows
        .iter()
        .map(|p| to_layer.transform_point3(*p).truncate())
        .collect();
    let b = crate::three_d::points_bounds(&pts);
    let px = 1.0 / receiver.x_axis.truncate().length().max(1e-12);
    let pad = (2.0 * f64::from(light.shadow_diffusion) + 4.0) * px;
    b.x1 + pad > rect.x0 && b.x0 - pad < rect.x1 && b.y1 + pad > rect.y0 && b.y0 - pad < rect.y1
}

/// Whether every homography of `hs` is the same map.
fn all_equal(hs: &[Homography]) -> bool {
    let Some(first) = hs.first() else {
        return true;
    };
    let norm = first
        .0
        .to_cols_array()
        .iter()
        .fold(0.0f64, |a, v| a.max(v.abs()));
    hs.iter()
        .all(|h| h.0.abs_diff_eq(first.0, 1e-12 * norm.max(1.0)))
}

fn rect_union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        None => b,
        Some(a) => Rect::new(
            a.x0.min(b.x0),
            a.y0.min(b.y0),
            a.x1.max(b.x1),
            a.y1.max(b.y1),
        ),
    }
}

fn rect_bits(r: &Rect) -> [u64; 4] {
    [
        r.x0.to_bits(),
        r.y0.to_bits(),
        r.x1.to_bits(),
        r.y1.to_bits(),
    ]
}

fn hash_of(value: &impl Hash) -> u64 {
    let mut h = FxHasher::default();
    value.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;
    use crate::input::{RenderLayer, TrackMatte};
    use crate::request::Resolution;

    fn p(s: &str) -> PrimPath {
        PrimPath::new(s).unwrap()
    }

    fn solid(name: &str, w: u32, h: u32, color: [f32; 4], pos: (f64, f64)) -> RenderLayer {
        let mut l = RenderLayer::new(
            p(&format!("/comps/main/layers/{name}")),
            LayerContent::Solid {
                color,
                width: w,
                height: h,
            },
        );
        l.transform = DAffine2::from_translation(DVec2::new(pos.0, pos.1));
        l
    }

    fn build_default(comp: &RenderComp, request: &RenderRequest) -> RenderGraph {
        build(comp, request, &mut NoSources, &EffectLibrary::new()).unwrap()
    }

    #[test]
    fn culls_layers_outside_the_roi_and_orders_bottom_first() {
        let mut comp = RenderComp::new(p("/comps/main"), 1000, 1000);
        comp.layers
            .push(solid("top", 100, 100, [1.0, 0.0, 0.0, 1.0], (0.0, 0.0)));
        comp.layers
            .push(solid("far", 100, 100, [0.0, 1.0, 0.0, 1.0], (800.0, 800.0)));
        comp.layers.push(solid(
            "bottom",
            1000,
            1000,
            [0.0, 0.0, 1.0, 1.0],
            (0.0, 0.0),
        ));
        let g = build_default(&comp, &RenderRequest::full());
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        let names: Vec<_> = c.ops.iter().map(|o| o.layer.name().to_owned()).collect();
        assert_eq!(names, ["bottom", "far", "top"]);
        assert_eq!(c.ops[2].bounds, IRect::new(0, 0, 102, 102));

        let roi = RenderRequest {
            roi: Some(IRect::new(0, 0, 200, 200)),
            ..RenderRequest::full()
        };
        let g = build_default(&comp, &roi);
        assert_eq!(g.output_node().rect, IRect::new(0, 0, 200, 200));
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert_eq!(c.ops.len(), 2, "the far layer is culled");
    }

    #[test]
    fn half_resolution_scales_rects() {
        let mut comp = RenderComp::new(p("/comps/main"), 1920, 1080);
        comp.layers
            .push(solid("a", 200, 100, [1.0; 4], (100.0, 100.0)));
        let req = RenderRequest {
            resolution: Resolution::Half,
            ..RenderRequest::full()
        };
        let g = build_default(&comp, &req);
        assert_eq!(g.output_node().rect, IRect::new(0, 0, 960, 540));
        assert_eq!(g.output_node().pixel_scale, 2.0);
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert_eq!(c.ops[0].bounds, IRect::new(48, 48, 152, 102));
    }

    #[test]
    fn nested_comp_gets_inverse_mapped_roi() {
        let mut inner = RenderComp::new(p("/comps/inner"), 1000, 1000);
        inner
            .layers
            .push(solid("i", 1000, 1000, [1.0; 4], (0.0, 0.0)));
        let mut comp = RenderComp::new(p("/comps/main"), 500, 500);
        let mut pre = RenderLayer::new(
            p("/comps/main/layers/pre"),
            LayerContent::Comp(Arc::new(inner)),
        );
        // Inner comp placed at (-250, -250): only its [250, 750) square is visible.
        pre.transform = DAffine2::from_translation(DVec2::new(-250.0, -250.0));
        comp.layers.push(pre);
        let g = build_default(&comp, &RenderRequest::full());
        assert_eq!(g.nodes.len(), 2);
        let inner_node = &g.nodes[0];
        assert!(matches!(inner_node.kind, NodeKind::Composite(_)));
        assert!(inner_node.cacheable);
        assert_eq!(inner_node.rect, IRect::new(249, 249, 751, 751));
        assert_eq!(g.use_counts(), vec![1, 0]);
    }

    #[test]
    fn track_matte_plans_and_culls() {
        let mut comp = RenderComp::new(p("/comps/main"), 1000, 1000);
        // Layer 0 is the matte (eye off), layer 1 uses it.
        let mut matte = solid("matte", 100, 100, [1.0; 4], (0.0, 0.0));
        matte.visible = false;
        comp.layers.push(matte);
        let mut fill = solid("fill", 1000, 1000, [1.0, 0.0, 0.0, 1.0], (0.0, 0.0));
        fill.track_matte = Some(TrackMatte {
            layer: 0,
            mode: TrackMatteMode::Alpha,
        });
        comp.layers.push(fill);
        let g = build_default(&comp, &RenderRequest::full());
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert_eq!(c.ops.len(), 1, "the matte layer itself is hidden");
        let m = c.ops[0].matte.expect("matte");
        assert_eq!(g.node(m.node).rect, IRect::new(0, 0, 102, 102));

        // Matte outside the frame: the matted layer disappears (alpha) …
        comp.layers[0].transform = DAffine2::from_translation(DVec2::new(5000.0, 0.0));
        let g = build_default(&comp, &RenderRequest::full());
        assert_eq!(g.draw_count(), 0);
        // … or is fully visible without a matte (inverted).
        comp.layers[1].track_matte = Some(TrackMatte {
            layer: 0,
            mode: TrackMatteMode::AlphaInverted,
        });
        let g = build_default(&comp, &RenderRequest::full());
        assert_eq!(g.draw_count(), 1);
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert!(c.ops[0].matte.is_none());

        // Cycles are detected, not followed.
        comp.layers[0].track_matte = Some(TrackMatte {
            layer: 1,
            mode: TrackMatteMode::Alpha,
        });
        comp.layers[0].transform = DAffine2::IDENTITY;
        comp.layers[1].track_matte = Some(TrackMatte {
            layer: 0,
            mode: TrackMatteMode::Alpha,
        });
        let g = build_default(&comp, &RenderRequest::full());
        assert!(g.warnings.iter().any(|w| w.contains("cycle")));
    }

    #[test]
    fn source_density_follows_magnification() {
        let roi = IRect::new(0, 0, 100, 100);
        let t = DAffine2::from_scale(DVec2::new(4.0, 2.0));
        assert_eq!(source_density(&t, 1.0, roi), 4.0);
        assert_eq!(source_density(&t, 2.0, roi), 2.0, "half resolution");
        let r = DAffine2::from_angle(0.7) * DAffine2::from_scale(DVec2::splat(0.5));
        assert!((source_density(&r, 1.0, roi) - 0.5).abs() < 1e-12);
        assert_eq!(
            source_density(&DAffine2::from_scale(DVec2::splat(100.0)), 1.0, roi),
            16.0
        );
        assert_eq!(
            source_density(
                &DAffine2::from_scale(DVec2::splat(8.0)),
                1.0,
                IRect::new(0, 0, 4096, 10)
            ),
            2.0,
            "texture limit"
        );
    }

    #[test]
    fn hard_mix_reaches_compositor() {
        let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
        let mut l = solid("a", 10, 10, [1.0; 4], (0.0, 0.0));
        l.blend_mode = BlendMode::HardMix;
        comp.layers.push(l);
        let g = build_default(&comp, &RenderRequest::full());
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert_eq!(c.ops[0].blend, BlendMode::HardMix);
        assert!(g.warnings.is_empty());
    }

    #[test]
    fn keys_are_stable_and_parameter_sensitive() {
        let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
        comp.layers.push(solid("a", 10, 10, [1.0; 4], (0.0, 0.0)));
        let k1 = build_default(&comp, &RenderRequest::full())
            .output_node()
            .key;
        let k2 = build_default(&comp, &RenderRequest::full())
            .output_node()
            .key;
        assert_eq!(k1, k2);
        comp.layers[0].opacity = 0.5;
        let k3 = build_default(&comp, &RenderRequest::full())
            .output_node()
            .key;
        assert_ne!(k1, k3);
    }
}

//! The render graph: what to draw, in which order, and which pixels are needed.
//!
//! [`build`] turns a [`RenderComp`] plus a [`RenderRequest`] into a DAG of
//! [`Node`]s, children before parents (so `nodes` is already a valid execution
//! order). Node kinds follow AE's per-layer pipeline:
//!
//! ```text
//! layer content (solid / Source / nested Composite)
//!   → masks (Mask, reserved) → effects (Effect …) ─┐
//!                                                  ├→ LayerOp in the parent Composite:
//! track matte layer → matte Composite ─────────────┘   transform · opacity · matte · blend
//! ```
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
//! **Cache keys.** Each node carries `key`, a hash of (prim, time, every parameter
//! that affects its pixels, rect, scale) – the `(Prim, time, parameter hash)` key
//! of ARCHITECTURE §5. Identical subgraphs within a frame are shared.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bae_model::{PrimPath, Time};
use glam::DAffine2;
use rustc_hash::{FxHashMap, FxHasher};

use crate::blend::{BlendMode, TrackMatteMode};
use crate::effect::{Effect, EffectLibrary, EffectParams};
use crate::error::RenderError;
use crate::geom::{IRect, Rect, is_invertible, scale_affine};
use crate::input::{
    CustomContent, FootageRef, LayerContent, RenderComp, RenderEffect, RenderLayer,
};
use crate::request::RenderRequest;
use crate::source::{LayerSource, SourceRequest};

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
    /// Nodes in execution order (inputs before their consumers).
    pub nodes: Vec<Node>,
    /// The node producing the requested frame.
    pub output: NodeId,
    /// Things that could not be rendered faithfully (unimplemented blend modes,
    /// missing footage or effects …), for logs and the UI.
    pub warnings: Vec<String>,
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
                }
            }
            NodeKind::Effect(e) => f(e.input),
            NodeKind::Fill(_) | NodeKind::Source(_) => {}
        }
    }

    /// Short name for logs.
    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::Composite(_) => "composite",
            NodeKind::Fill(_) => "fill",
            NodeKind::Source(_) => "source",
            NodeKind::Effect(_) => "effect",
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
#[derive(Debug)]
pub struct LayerOp {
    /// The layer.
    pub layer: PrimPath,
    /// What is drawn.
    pub input: OpInput,
    /// Input space pixels → composite space pixels (full resolution).
    pub transform: DAffine2,
    /// Opacity `[0, 1]`.
    pub opacity: f32,
    /// Blend mode (already mapped to an implemented one).
    pub blend: BlendMode,
    /// Only draw where the backdrop has alpha.
    pub preserve_transparency: bool,
    /// Track matte image (composite space, same units as the composite).
    pub matte: Option<MatteRef>,
    /// Destination region in composite texels (clipped to the composite rect).
    pub bounds: IRect,
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
    /// Layer time.
    pub time: Time,
    /// Resolution scale passed to the effect.
    pub scale: f64,
    /// Input node.
    pub input: NodeId,
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

/// Builds the graph for `comp` under `request`.
pub fn build(
    comp: &RenderComp,
    request: &RenderRequest,
    sources: &mut dyn SourceResolver,
    effects: &EffectLibrary,
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
        clamp: request.depth.clamps(),
        nodes: Vec::new(),
        dedup: FxHashMap::default(),
        warnings: Vec::new(),
    };
    let output = builder.composite(comp, rect, scale, 0, false)?;
    Ok(RenderGraph {
        nodes: builder.nodes,
        output,
        warnings: builder.warnings,
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

struct Builder<'a> {
    sources: &'a mut dyn SourceResolver,
    effects: &'a EffectLibrary,
    clamp: bool,
    nodes: Vec<Node>,
    dedup: FxHashMap<u64, NodeId>,
    warnings: Vec<String>,
}

/// The content of a layer, planned: its bounds (in layer texel units) and how
/// to produce it.
enum Content {
    Solid { color: [f32; 4], rect: Rect },
    Fill { color: [f32; 4], rect: Rect },
    Source(Arc<dyn LayerSource>, Time),
    Comp(Arc<RenderComp>),
}

impl Builder<'_> {
    fn push(&mut self, node: Node) -> NodeId {
        if let Some(&id) = self.dedup.get(&node.key) {
            return id;
        }
        let id = NodeId(self.nodes.len() as u32);
        self.dedup.insert(node.key, id);
        self.nodes.push(node);
        id
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
            // AE draws from the bottom of the stack (highest index) to the top.
            for index in (0..comp.layers.len()).rev() {
                let layer = &comp.layers[index];
                if !layer.visible || !layer.active {
                    continue;
                }
                if layer.adjustment {
                    self.warn(format!(
                        "{}: adjustment layers are not rendered yet",
                        layer.path
                    ));
                    continue;
                }
                let mut visiting = vec![index];
                if let Some(op) = self.layer_op(comp, index, rect, scale, depth, &mut visiting)? {
                    ops.push(op);
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
        let layer = &comp.layers[index];
        if layer.opacity <= 0.0 || !is_invertible(&layer.transform) {
            return Ok(None);
        }
        let Some(content) = self.plan_content(layer, layer.time)? else {
            return Ok(None);
        };
        let content_rect = match &content {
            Content::Solid { rect, .. } | Content::Fill { rect, .. } => *rect,
            Content::Source(src, _) => src.bounds().to_rect(),
            Content::Comp(c) => c.rect(),
        };
        // Effects: bounds forward (in layer texel units).
        let effects = self.plan_effects(layer);
        let mut bounds_chain = vec![units(&content_rect, scale)];
        for (effect, instance) in &effects {
            let params = EffectParams {
                effect: instance,
                time: layer.time,
                scale: 1.0 / scale,
            };
            let prev = *bounds_chain.last().unwrap_or(&IRect::EMPTY);
            bounds_chain.push(effect.output_bounds(&params, prev));
        }
        let layer_bounds = *bounds_chain.last().unwrap_or(&IRect::EMPTY);
        if layer_bounds.is_empty() {
            return Ok(None);
        }
        // Destination bounds in composite units (+1 texel for edge anti-aliasing).
        let to_units = scale_affine(1.0 / scale) * layer.transform * scale_affine(scale);
        let bounds = layer_bounds
            .to_rect()
            .transformed_bounds(&to_units)
            .round_out()
            .inflate(1)
            .intersect(&canvas);
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
        let mut roi = bounds
            .to_rect()
            .transformed_bounds(&to_units.inverse())
            .round_out()
            .inflate(1)
            .intersect(&layer_bounds);
        for (i, (effect, instance)) in effects.iter().enumerate().rev() {
            let params = EffectParams {
                effect: instance,
                time: layer.time,
                scale: 1.0 / scale,
            };
            roi = effect.input_roi(&params, roi).intersect(&bounds_chain[i]);
        }

        let blend = layer.blend_mode.effective();
        if blend != layer.blend_mode {
            self.warn(format!(
                "{}: blend mode `{}` is not implemented yet, using normal",
                layer.path, layer.blend_mode
            ));
        }
        if !layer.masks.is_empty() {
            self.warn(format!("{}: masks are not rendered yet", layer.path));
        }

        let mut input = match content {
            Content::Solid { color, rect } => OpInput::Solid { color, rect },
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
                let mut h = FxHasher::default();
                "source".hash(&mut h);
                layer.path.hash(&mut h);
                source.content_hash(time).hash(&mut h);
                roi.hash(&mut h);
                scale.to_bits().hash(&mut h);
                let request = SourceRequest {
                    roi: Rect::new(
                        f64::from(roi.x0) * scale,
                        f64::from(roi.y0) * scale,
                        f64::from(roi.x1) * scale,
                        f64::from(roi.y1) * scale,
                    )
                    .round_out(),
                    scale: 1.0 / scale,
                    time,
                };
                OpInput::Image(self.push(Node {
                    kind: NodeKind::Source(SourceNode {
                        layer: layer.path.clone(),
                        source,
                        request,
                    }),
                    rect: roi,
                    pixel_scale: scale,
                    key: h.finish(),
                    cacheable: false,
                }))
            }
            Content::Comp(nested) => {
                OpInput::Image(self.composite(&nested, roi, scale, depth + 1, true)?)
            }
        };
        for (i, (effect, instance)) in effects.into_iter().enumerate() {
            let OpInput::Image(input_id) = input else {
                unreachable!("solids with effects are planned as fills")
            };
            let rect = bounds_chain[i + 1];
            let mut h = FxHasher::default();
            "effect".hash(&mut h);
            self.nodes[input_id.index()].key.hash(&mut h);
            // Effects may vary with time on their own (noise evolution, wiggle).
            layer.time.hash(&mut h);
            crate::input::hash_value(&bae_model::Value::String(instance.effect.clone()), &mut h);
            for (id, value) in &instance.params {
                id.hash(&mut h);
                crate::input::hash_value(value, &mut h);
            }
            rect.hash(&mut h);
            input = OpInput::Image(self.push(Node {
                kind: NodeKind::Effect(EffectNode {
                    layer: layer.path.clone(),
                    effect,
                    instance,
                    time: layer.time,
                    scale: 1.0 / scale,
                    input: input_id,
                }),
                rect,
                pixel_scale: scale,
                key: h.finish(),
                cacheable: true,
            }));
        }

        Ok(Some(LayerOp {
            layer: layer.path.clone(),
            input,
            transform: layer.transform,
            opacity: layer.opacity.clamp(0.0, 1.0),
            blend,
            preserve_transparency: layer.preserve_transparency,
            matte,
            bounds,
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

    fn plan_content(
        &mut self,
        layer: &RenderLayer,
        time: Time,
    ) -> Result<Option<Content>, RenderError> {
        let has_effects = layer
            .effects
            .iter()
            .any(|e| e.enabled && self.effects.get(&e.effect).is_some());
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
                Some(source) => Some(Content::Source(source, footage.source_time)),
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
        let mut out = Vec::new();
        for instance in layer.effects.iter().filter(|e| e.enabled) {
            match self.effects.get(&instance.effect) {
                Some(effect) => out.push((effect.clone(), instance.clone())),
                None => self.warn(format!(
                    "{}: effect `{}` is not available, skipped",
                    layer.path, instance.effect
                )),
            }
        }
        out
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
        assert_eq!(c.ops[2].bounds, IRect::new(0, 0, 101, 101));

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
        assert_eq!(c.ops[0].bounds, IRect::new(49, 49, 151, 101));
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
        assert_eq!(g.node(m.node).rect, IRect::new(0, 0, 101, 101));

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
    fn unimplemented_modes_warn_and_fall_back() {
        let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
        let mut l = solid("a", 10, 10, [1.0; 4], (0.0, 0.0));
        l.blend_mode = BlendMode::HardMix;
        comp.layers.push(l);
        let g = build_default(&comp, &RenderRequest::full());
        let NodeKind::Composite(c) = &g.output_node().kind else {
            panic!()
        };
        assert_eq!(c.ops[0].blend, BlendMode::Normal);
        assert_eq!(g.warnings.len(), 1);
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

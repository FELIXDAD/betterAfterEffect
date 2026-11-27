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

pub mod layer_styles;

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bae_model::{PrimPath, Time};
use glam::{DAffine2, DMat4, DVec2};
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
    /// A layer source was still warming up and is missing (not frame-cached).
    pub incomplete: bool,
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
    Effect(Box<EffectNode>),
    /// A layer's masks combined into one coverage image (layer space).
    Mask(MaskNode),
    /// An adjustment layer's result mixed into its backdrop.
    Mix(MixNode),
    /// A 3D layer drawn alone, blurred by the camera's depth of field.
    DepthBlur(DepthBlurNode),
    /// A 3D model layer drawn through the camera with a depth buffer.
    Mesh(Box<MeshNode>),
    /// An environment light's picture behind the layers, through the camera.
    Sky(Box<SkyNode>),
    /// A lit 3D layer's shadows in its own space.
    ShadowMap(ShadowMapNode),
}

/// The 3D model layers of a 3D group (AE's Advanced 3D models,
/// [`crate::model`]) drawn through the camera into one depth buffer (they
/// intersect each other) and lit by the comp's lights, premultiplied, in the
/// composite's space (`exec/mesh.rs`). The pass also keeps each pixel's
/// camera depth for the planes drawn over the models ([`Occlusion`]).
#[derive(Debug)]
pub struct MeshNode {
    pub models: Vec<MeshModel>,
    pub camera: crate::camera::Camera,
    /// The comp's size in pixels.
    pub comp_size: [u32; 2],
    /// A linear working space (the shading is not display-encoded).
    pub linear: bool,
}

/// One model layer of a [`MeshNode`].
#[derive(Debug)]
pub struct MeshModel {
    /// The layer (diagnostics).
    pub layer: PrimPath,
    pub model: Arc<crate::model::Model>,
    /// Model space → comp space.
    pub matrix: DMat4,
    /// The lights that reach the layer.
    pub lights: Arc<[crate::light::Light]>,
    /// The layer's opacity.
    pub opacity: f32,
    /// An environment light with a picture (diffuse light and reflections).
    pub environment: Option<EnvironmentInput>,
    /// Material Options: it casts shadows on the models (and itself).
    pub casts_shadows: bool,
    /// Material Options: the models' shadows fall on it.
    pub accepts_shadows: bool,
}

/// Where a 3D plane passes behind the 3D models drawn under it
/// ([`MeshNode`], earlier in the same composite): the composite drops the
/// plane's pixels farther than the models' (by their alpha), so the models
/// show through it there and it covers them where it is nearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occlusion {
    /// The models' pass (it keeps each pixel's camera depth).
    pub mesh: NodeId,
    /// The plane's camera depth on screen ([`plane_depth`]).
    pub depth: [f64; 3],
    pub orthographic: bool,
}

/// An environment light's picture as a node input (`exec/env.rs`).
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentInput {
    /// The source layer's pixels (equirectangular).
    pub input: NodeId,
    /// Width of the map's largest level, in texels.
    pub width: u32,
    /// Comp directions → picture directions (the light's rotation undone).
    pub rotation: DMat4,
    /// Colour × intensity.
    pub tint: [f32; 3],
}

/// AE's Background Visible of an environment light: its picture drawn
/// behind the layers through the camera, premultiplied (`exec/env.rs`).
#[derive(Debug)]
pub struct SkyNode {
    /// The comp (diagnostics).
    pub comp: PrimPath,
    pub environment: EnvironmentInput,
    pub camera: crate::camera::Camera,
    /// The comp's size in pixels.
    pub comp_size: [u32; 2],
    /// Background Opacity, 0–1.
    pub opacity: f32,
    /// Background Blur, 0–1.
    pub blur: f32,
    /// A linear working space (the picture is not display-encoded).
    pub linear: bool,
}

/// How much of each shadow-casting light reaches every point of a lit 3D
/// layer past the layer's shadow casters, in the layer's own pixels (AE's
/// shadow maps): drawn when the layer, its lights or casters change and kept
/// across frames; the layer's draws sample it (`exec/shadowmap.rs`).
#[derive(Debug)]
pub struct ShadowMapNode {
    /// The layer (diagnostics).
    pub layer: PrimPath,
    /// The layer's lighting with its casters.
    pub lighting: Arc<crate::light::Lighting>,
    /// Output texels the layer covers: when they outnumber a map slot's
    /// texels, testing the casters per output pixel costs more than the map.
    pub screen_texels: u64,
    /// Per slot, the key of its contents (the layer, its casters and that
    /// light): a slot whose light did not change is copied, not drawn.
    pub slot_keys: Vec<u64>,
}

/// Depth of field of one 3D layer (AE's camera Depth of Field): the layer
/// drawn alone, blurred per pixel by the distance of its plane from the
/// camera's focus plane (`exec/dof.rs`). All lengths in space pixels.
#[derive(Debug)]
pub struct DepthBlurNode {
    /// The layer (diagnostics).
    pub layer: PrimPath,
    /// The layer drawn alone onto transparent black.
    pub input: NodeId,
    /// `1 / depth` of the layer's plane at space pixel `(x, y)`:
    /// `a·x + b·y + c` (camera-space depth).
    pub inv_depth: [f64; 3],
    /// Camera depth of the plane in focus.
    pub focus: f64,
    /// The blur's diameter far away: at depth `z` it is
    /// `coc · |1 − focus / z|`.
    pub coc: f64,
    /// Largest blur radius over the layer.
    pub max_radius: f64,
    /// The blur's iris and highlights.
    pub iris: bae_anim::Iris,
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
                    if let Some(o) = &op.occlusion {
                        f(o.mesh);
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
            NodeKind::DepthBlur(d) => f(d.input),
            NodeKind::Mesh(m) => {
                for e in m.models.iter().filter_map(|m| m.environment.as_ref()) {
                    f(e.input);
                }
            }
            NodeKind::Sky(s) => f(s.environment.input),
            NodeKind::ShadowMap(m) => {
                for c in m.lighting.casters.iter() {
                    if let crate::light::CasterPixels::Image(id) = c.pixels {
                        f(id);
                    }
                }
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
            NodeKind::DepthBlur(_) => "depth of field",
            NodeKind::Mesh(_) => "3d model",
            NodeKind::Sky(_) => "environment background",
            NodeKind::ShadowMap(_) => "shadow map",
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
    /// Sampling quality of the source layer.
    pub quality: bae_anim::LayerQuality,
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
    /// 3D planes over 3D models: cut where they pass behind them.
    pub occlusion: Option<Occlusion>,
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
    /// Effect whose previous output supplies temporal feedback.
    pub feedback: Option<PrimPath>,
    /// The layer.
    pub layer: PrimPath,
    /// The implementation.
    pub effect: Arc<dyn Effect>,
    /// Evaluated parameters.
    pub instance: RenderEffect,
    /// Composition scene when requested.
    pub scene: Option<Box<crate::effect::EffectScene>>,
    /// Frame context for [`EffectParams`] (time, scale, bounds, rate, shutter …).
    pub frame: EffectFrame,
    /// Input node.
    pub input: NodeId,
    /// Layer images the effect reads ([`Effect::layer_dependencies`]).
    pub layers: Vec<(LayerDependency, NodeId)>,
    /// Mask coverage images it reads ([`Effect::mask_dependencies`]).
    pub masks: Vec<(MaskSelect, NodeId)>,
    /// Raw mask paths and historical parameters, for effects that read them.
    pub(crate) samples: Option<Box<EffectSamples>>,
}

pub(crate) struct EffectSamples {
    pub parameter_samples: Vec<crate::effect::ParameterSample>,
    pub tracks: Vec<(u32, Arc<crate::effect::ScalarTrack>)>,
    pub paths: Vec<RenderMask>,
    pub history: Vec<(Time, RenderEffect)>,
}

/// The frame context of an effect call, stored per effect node (everything in
/// [`EffectParams`] except the instance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectFrame {
    /// Layer time.
    pub time: Time,
    /// Comp time.
    pub comp_time: Time,
    /// Composition time corresponding to zero on the layer clock.
    pub layer_start_time: Time,
    /// Comp time when the layer first becomes active.
    pub layer_in: Time,
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
    /// Map a layer sample time to the corresponding composition time.
    pub fn comp_at(&self, local: Time) -> Time {
        self.layer_start_time
            + Time::from_flicks((local.flicks() as f64 * self.layer_time_scale).round() as i64)
    }

    /// The parameters of `instance` in this frame.
    pub fn params<'a>(&self, instance: &'a RenderEffect) -> EffectParams<'a> {
        EffectParams {
            time: self.time,
            comp_time: self.comp_time,
            layer_in: self.layer_in,
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
    /// Whether a sampled control may vary with time.
    fn animated_controls(&self, _effect: &PrimPath, _ids: &[u32]) -> bool {
        true
    }

    /// Sample only the selected controls at the requested layer times.
    fn parameters_at(
        &mut self,
        _effect: &RenderEffect,
        _ids: &[u32],
        _frame: &EffectFrame,
        _times: &[Time],
    ) -> Vec<crate::effect::ParameterSample> {
        Vec::new()
    }

    /// Random-access integral of an animated scalar control in layer time.
    fn scalar_track(
        &mut self,
        _effect: &PrimPath,
        _id: u32,
        _frame: &EffectFrame,
    ) -> Option<Arc<crate::effect::ScalarTrack>> {
        None
    }

    /// `comp` evaluated at comp time `time` (`None`: not available).
    fn comp_at(&mut self, comp: &PrimPath, time: Time) -> Result<Option<RenderComp>, RenderError>;

    /// Evaluated parameters of one effect at a comp time.
    fn effect_at(
        &mut self,
        comp: &PrimPath,
        layer: &PrimPath,
        effect: &PrimPath,
        time: Time,
    ) -> Result<Option<RenderEffect>, RenderError> {
        Ok(self.comp_at(comp, time)?.and_then(|comp| {
            comp.layers
                .into_iter()
                .find(|l| l.path == *layer)?
                .effects
                .into_iter()
                .find(|fx| fx.path == *effect)
        }))
    }
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
    let mut builder = Builder::new(request, sources, effects, frames);
    let output = builder.composite(comp, rect, scale, 0, false)?;
    Ok(builder.finish(request, output))
}

/// A graph whose output is `region` (layer texels at `request`'s resolution) of
/// one layer of `comp` in its layer space, as an effect layer input reads it
/// (`dep`: which layer, at which comp time, how far through masks and effects),
/// drawn into a working texture of its own (readable back). `None` when the
/// layer has no pixels. `sampleImage` expressions read layers through this.
pub fn build_layer_image(
    comp: &RenderComp,
    request: &RenderRequest,
    sources: &mut dyn SourceResolver,
    effects: &EffectLibrary,
    frames: Option<&mut dyn CompFrames>,
    dep: &LayerDependency,
    region: IRect,
) -> Result<Option<RenderGraph>, RenderError> {
    let scale = f64::from(request.resolution.factor());
    let mut builder = Builder::new(request, sources, effects, frames);
    let Some(layer) = builder.layer_input(comp, dep, scale, 0)? else {
        return Ok(None);
    };
    let node = &builder.nodes[layer.index()];
    let content = node.rect.to_rect();
    let mut h = FxHasher::default();
    (
        "layer image",
        node.key,
        region,
        scale.to_bits(),
        builder.clamp,
    )
        .hash(&mut h);
    let output = builder.push(Node {
        kind: NodeKind::Composite(CompositeNode {
            prim: dep.layer.clone(),
            time: dep.time,
            ops: vec![LayerOp {
                layer: dep.layer.clone(),
                input: OpInput::Image(layer),
                transform: DAffine2::IDENTITY,
                opacity: 1.0,
                blend: BlendMode::Normal,
                dissolve_seed: 0,
                quality: bae_anim::LayerQuality::Best,
                preserve_transparency: false,
                matte: None,
                mask: None,
                bounds: region,
                content: Rect::new(
                    content.x0 * scale,
                    content.y0 * scale,
                    content.x1 * scale,
                    content.y1 * scale,
                ),
                projection: None,
                motion: None,
                clips: Vec::new(),
                lighting: None,
                occlusion: None,
            }],
            clamp: builder.clamp,
        }),
        rect: region,
        pixel_scale: scale,
        key: h.finish(),
        cacheable: false,
    });
    Ok(Some(builder.finish(request, output)))
}

/// A 3D model layer's model space (centred, a largest side of 1) → comp
/// space: AE sizes an imported model to half the comp's smaller side, placed
/// by the layer's transform.
fn model_matrix(comp: &RenderComp, layer: &RenderLayer) -> DMat4 {
    let place = layer.matrix_3d.unwrap_or_else(|| {
        let t = layer.transform;
        DMat4::from_cols(
            glam::DVec4::new(t.matrix2.x_axis.x, t.matrix2.x_axis.y, 0.0, 0.0),
            glam::DVec4::new(t.matrix2.y_axis.x, t.matrix2.y_axis.y, 0.0, 0.0),
            glam::DVec4::new(0.0, 0.0, 1.0, 0.0),
            glam::DVec4::new(t.translation.x, t.translation.y, 0.0, 1.0),
        )
    });
    let size = f64::from(comp.width.min(comp.height)) / 2.0;
    place * DMat4::from_scale(glam::DVec3::splat(size))
}

/// Largest depth-of-field blur radius (space pixels).
const MAX_DOF_RADIUS: f64 = 500.0;

/// Camera depth over a plane (layer pixels → comp space `matrix`) on
/// screen: `1 / z = a·x + b·y + c` at comp pixel (x, y) through a
/// perspective camera, `z = a·x + b·y + c` through an orthographic one
/// (`true`). `None` when the camera sees the plane edge-on.
pub(crate) fn plane_depth(
    camera: &crate::camera::Camera,
    matrix: &DMat4,
    size: DVec2,
) -> Option<([f64; 3], bool)> {
    let to_camera = camera.view() * *matrix;
    let n = to_camera
        .x_axis
        .truncate()
        .cross(to_camera.y_axis.truncate());
    let d = n.dot(to_camera.w_axis.truncate());
    let (cx, cy) = (size.x / 2.0, size.y / 2.0);
    match camera.projection {
        crate::camera::Projection::Perspective => {
            let f = camera.zoom;
            (d.abs() > 1e-9 && f > 0.0).then(|| {
                (
                    [
                        n.x / (f * d),
                        n.y / (f * d),
                        (n.z - n.x * cx / f - n.y * cy / f) / d,
                    ],
                    false,
                )
            })
        }
        crate::camera::Projection::Orthographic { scale: s } => {
            (n.z.abs() > 1e-9 && s > 0.0).then(|| {
                (
                    [
                        -n.x / (s * n.z),
                        -n.y / (s * n.z),
                        (d + n.x * cx / s + n.y * cy / s) / n.z,
                    ],
                    true,
                )
            })
        }
    }
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
    native_motion_blur: bool,
    /// Other frames of the comp (effects reading other times).
    frames: Option<&'f mut dyn CompFrames>,
    /// Layers whose effects are being planned (cycle detection of layer inputs).
    resolving: Vec<PrimPath>,
    time_dependent: bool,
    incomplete: bool,
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

impl<'a, 'f> Builder<'a, 'f> {
    fn new(
        request: &RenderRequest,
        sources: &'a mut dyn SourceResolver,
        effects: &'a EffectLibrary,
        frames: Option<&'f mut dyn CompFrames>,
    ) -> Self {
        Self {
            sources,
            effects,
            linear: request.color.blend == crate::request::BlendSpace::Linear,
            purpose: request.purpose,
            clamp: request.depth.clamps(),
            nodes: Vec::new(),
            dedup: FxHashMap::default(),
            warnings: Vec::new(),
            skip_effects_of: None,
            native_motion_blur: true,
            frames,
            resolving: Vec::new(),
            time_dependent: false,
            incomplete: false,
            owner: None,
            owners: Vec::new(),
        }
    }

    fn finish(self, request: &RenderRequest, output: NodeId) -> RenderGraph {
        RenderGraph {
            depth: request.depth,
            nodes: self.nodes,
            output,
            warnings: self.warnings,
            time_dependent: self.time_dependent,
            incomplete: self.incomplete,
            owners: self.owners,
        }
    }
}

impl Builder<'_, '_> {
    fn push(&mut self, mut node: Node) -> NodeId {
        if !self.native_motion_blur {
            node.key = hash_of(&("without native motion blur", node.key));
        }
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
        self.composite_from(comp, 0, rect, scale, depth, cacheable)
    }

    fn composite_from(
        &mut self,
        comp: &RenderComp,
        first_layer: usize,
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
        if let Some(sky) = self.sky_op(comp, rect, scale, depth)? {
            ops.push(sky);
        }
        if !rect.is_empty() {
            // AE draws from the bottom of the stack (highest index) to the top;
            // contiguous 3D layers form groups drawn in depth order.
            for segment in crate::three_d::segments(&comp.layers[first_layer..]) {
                let index = match segment {
                    Segment::Group(mut indices) => {
                        for index in &mut indices {
                            *index += first_layer;
                        }
                        self.group_3d(comp, &indices, rect, scale, depth, &mut ops)?;
                        continue;
                    }
                    Segment::Layer(index) => first_layer + index,
                };
                let layer = &comp.layers[index];
                if layer.adjustment {
                    ops = self.adjustment(comp, index, rect, scale, depth, ops)?;
                    continue;
                }
                let mut visiting = vec![index];
                if let Some(styles) = &layer.layer_styles
                    && styles.enabled
                    && let Some(effect) = self.effects.get(&styles.effect).cloned()
                {
                    self.owned(comp, index, depth, |b| {
                        b.styled_layer(
                            comp,
                            index,
                            rect,
                            scale,
                            depth,
                            &mut visiting,
                            effect,
                            styles,
                            &mut ops,
                        )
                    })?;
                    continue;
                }
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
        first_layer.hash(&mut h);
        if depth == 0 {
            // The frame's own comp is the only root: its key only has to be
            // unique in this graph, so the whole comp is not hashed per frame
            // (1000 layers: ~6 % of render+submit). Nested comps hash their
            // content so identical pre-comp instances share one node.
            comp.path.hash(&mut h);
            comp.time.hash(&mut h);
        } else {
            comp.hash_content(&mut h, Some(self.effects));
        }
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
        self.attach_model_shadows(comp, &mut planned);
        // Casts Shadows: Only: a caster that is not drawn itself.
        planned.retain(|(index, _)| {
            comp.layers[*index].material.casts_shadows != crate::light::ShadowMode::Only
        });
        for (index, op) in &mut planned {
            self.shadow_map(comp, *index, op, depth);
        }
        // 3D models: one depth-buffered pass under the planes; each plane
        // over them is cut where it passes behind them.
        let models = self.merge_models(comp, &mut planned);
        let drawn = Self::depth_order_3d(comp, planned);
        if let Some((op, _)) = &models {
            ops.push(op.clone());
        }
        for (index, op) in drawn {
            let mut op = self.depth_of_field(comp, index, op, canvas, scale, depth);
            if let Some((models, nearest)) = &models {
                self.occlude(comp, index, &mut op, models, *nearest, scale);
            }
            ops.push(op);
        }
        Ok(())
    }

    /// 3D models with Casts Shadows on throw shadows on the lit layers of
    /// their 3D group that accept shadows (drawn into the layers' shadow
    /// maps, `exec/mesh.rs`).
    fn attach_model_shadows(&self, comp: &RenderComp, planned: &mut [(usize, LayerOp)]) {
        use crate::light::{ModelCaster, ShadowMode};
        let models: Arc<[ModelCaster]> = planned
            .iter()
            .filter(|(index, _)| {
                let layer = &comp.layers[*index];
                matches!(layer.content, LayerContent::Model(_))
                    && layer.material.casts_shadows != ShadowMode::Off
            })
            .filter_map(|(_, op)| match op.input {
                OpInput::Image(id) => match &self.nodes[id.index()].kind {
                    NodeKind::Mesh(m) => {
                        m.models
                            .first()
                            .filter(|m| m.opacity > 0.0)
                            .map(|m| ModelCaster {
                                model: m.model.clone(),
                                matrix: m.matrix,
                            })
                    }
                    _ => None,
                },
                OpInput::Solid { .. } => None,
            })
            .collect();
        if models.is_empty() {
            return;
        }
        for (index, op) in planned.iter_mut() {
            let layer = &comp.layers[*index];
            let Some(lighting) = &op.lighting else {
                continue;
            };
            if matches!(layer.content, LayerContent::Model(_))
                || layer.material.accepts_shadows == ShadowMode::Off
                || !lighting.lights.iter().any(|l| l.casts_shadows)
            {
                continue;
            }
            op.lighting = Some(Arc::new(crate::light::Lighting {
                models: models.clone(),
                ..(**lighting).clone()
            }));
        }
    }

    /// Takes the model layers out of `planned` and joins them into one
    /// [`MeshNode`] (one depth buffer). Returns its op and the camera depth
    /// of the models' nearest corner.
    fn merge_models(
        &mut self,
        comp: &RenderComp,
        planned: &mut Vec<(usize, LayerOp)>,
    ) -> Option<(LayerOp, f64)> {
        let is_model = |i: &usize| matches!(comp.layers[*i].content, LayerContent::Model(_));
        let mut found: Vec<LayerOp> = Vec::new();
        planned.retain(|(index, op)| {
            let model = is_model(index);
            if model {
                found.push(op.clone());
            }
            !model
        });
        let mut ids = found.iter().filter_map(|op| match op.input {
            OpInput::Image(id) => Some(id),
            OpInput::Solid { .. } => None,
        });
        let first = ids.next()?;
        let mut op = found[0].clone();
        let mut rect = self.nodes[first.index()].rect;
        let mut keys = vec![self.nodes[first.index()].key];
        let mut more = Vec::new();
        for id in ids {
            let node = &mut self.nodes[id.index()];
            rect = rect.union(&node.rect);
            keys.push(node.key);
            if let NodeKind::Mesh(m) = &mut node.kind {
                more.append(&mut m.models);
            }
        }
        let camera = comp.view_camera();
        let view = camera.view();
        let node = &mut self.nodes[first.index()];
        node.rect = rect;
        node.key = hash_of(&("3d models", keys));
        let NodeKind::Mesh(mesh) = &mut node.kind else {
            return None;
        };
        mesh.models.append(&mut more);
        let nearest = mesh
            .models
            .iter()
            .flat_map(|m| {
                (0..8).map(move |k| {
                    let corner = glam::DVec3::new(
                        if k & 1 == 0 { -0.5 } else { 0.5 },
                        if k & 2 == 0 { -0.5 } else { 0.5 },
                        if k & 4 == 0 { -0.5 } else { 0.5 },
                    );
                    view.transform_point3(m.matrix.transform_point3(corner)).z
                })
            })
            .fold(f64::MAX, f64::min);
        if found.len() > 1 {
            // Joined models draw with Normal (each keeps its opacity).
            op.blend = BlendMode::Normal;
            op.preserve_transparency = false;
        }
        let scale = node.pixel_scale;
        op.bounds = rect;
        op.content = Rect::new(
            f64::from(rect.x0) * scale,
            f64::from(rect.y0) * scale,
            f64::from(rect.x1) * scale,
            f64::from(rect.y1) * scale,
        );
        Some((op, nearest))
    }

    /// Cuts the 3D plane `op` (layer `index`) where it passes behind the
    /// models drawn by `models` ([`Occlusion`]). Planes in front of every
    /// model, or with a track matte, are left alone.
    fn occlude(
        &self,
        comp: &RenderComp,
        index: usize,
        op: &mut LayerOp,
        models: &LayerOp,
        nearest: f64,
        scale: f64,
    ) {
        let overlap = op.bounds.intersect(&models.bounds);
        let (OpInput::Image(mesh), None, false) = (&models.input, &op.matte, overlap.is_empty())
        else {
            return;
        };
        let Some(matrix) = comp.layers[index].matrix_3d else {
            return;
        };
        let camera = comp.view_camera();
        let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
        let Some((plane, orthographic)) = plane_depth(&camera, &matrix, size) else {
            return;
        };
        let farthest = [
            (overlap.x0, overlap.y0),
            (overlap.x1, overlap.y0),
            (overlap.x0, overlap.y1),
            (overlap.x1, overlap.y1),
        ]
        .into_iter()
        .map(|(x, y)| {
            let v = plane[0] * f64::from(x) * scale + plane[1] * f64::from(y) * scale + plane[2];
            match (orthographic, v > 0.0) {
                (true, _) => v,
                (false, true) => 1.0 / v,
                (false, false) => f64::MAX,
            }
        })
        .fold(f64::MIN, f64::max);
        if farthest < nearest {
            return;
        }
        op.occlusion = Some(Occlusion {
            mesh: *mesh,
            depth: plane,
            orthographic,
        });
    }

    /// The ops of a 3D group back to front, cut where layers intersect.
    fn depth_order_3d(comp: &RenderComp, planned: Vec<(usize, LayerOp)>) -> Vec<(usize, LayerOp)> {
        if planned.len() <= 1 {
            return planned;
        }
        let mut drawn = Vec::with_capacity(planned.len());
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
                drawn.push(planned[first].clone());
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
                let (index, op) = &planned[members[fragment.item]];
                let mut op = op.clone();
                op.clips = fragment.clips;
                drawn.push((*index, op));
            }
        }
        drawn
    }

    /// Gives a lit layer with shadow casters its receiver-space shadow map
    /// ([`ShadowMapNode`]). Motion-blurred layers (lit per sample) and
    /// textures whose pixels are turned relative to the layer keep testing
    /// the casters per pixel.
    fn shadow_map(&mut self, comp: &RenderComp, index: usize, op: &mut LayerOp, depth: usize) {
        let Some(lighting) = &op.lighting else {
            return;
        };
        if (lighting.casters.is_empty() && lighting.models.is_empty())
            || !lighting.samples.is_empty()
            || lighting.shadow_slots().is_empty()
        {
            return;
        }
        let mut h = FxHasher::default();
        "shadow map".hash(&mut h);
        for v in lighting.matrix.to_cols_array() {
            v.to_bits().hash(&mut h);
        }
        let r = lighting.rect;
        [r.x0, r.y0, r.x1, r.y1].map(f64::to_bits).hash(&mut h);
        for c in lighting.casters.iter() {
            for v in c.matrix.to_cols_array() {
                v.to_bits().hash(&mut h);
            }
            [c.rect.x0, c.rect.y0, c.rect.x1, c.rect.y1]
                .map(f64::to_bits)
                .hash(&mut h);
            c.opacity.to_bits().hash(&mut h);
            c.transmission.to_bits().hash(&mut h);
            match c.pixels {
                crate::light::CasterPixels::Solid(color) => color.map(f32::to_bits).hash(&mut h),
                crate::light::CasterPixels::Image(id) => self.nodes[id.index()].key.hash(&mut h),
            }
        }
        for m in lighting.models.iter() {
            (Arc::as_ptr(&m.model) as usize).hash(&mut h);
            m.matrix.to_cols_array().map(f64::to_bits).hash(&mut h);
        }
        let base = h.finish();
        let slot_keys: Vec<u64> = lighting
            .shadow_slots()
            .into_iter()
            .map(|li| {
                let mut h = FxHasher::default();
                base.hash(&mut h);
                crate::input::hash_light(&lighting.lights[li], &mut h);
                h.finish()
            })
            .collect();
        let key = hash_of(&(base, &slot_keys));
        let (_, [w, ht]) = lighting.shadow_map_size();
        let slots = slot_keys.len() as i32;
        let node = self.owned(comp, index, depth, |b| {
            b.push(Node {
                kind: NodeKind::ShadowMap(ShadowMapNode {
                    layer: op.layer.clone(),
                    lighting: lighting.clone(),
                    screen_texels: u64::from(op.bounds.width()) * u64::from(op.bounds.height()),
                    slot_keys,
                }),
                rect: IRect::new(0, 0, w as i32 * slots, ht as i32),
                pixel_scale: 1.0,
                key,
                cacheable: true,
            })
        });
        op.lighting = Some(Arc::new(crate::light::Lighting {
            shadow_map: Some(node),
            ..(**lighting).clone()
        }));
    }

    /// Depth of field (AE's camera Depth of Field): a 3D layer away from the
    /// focus plane is drawn alone onto transparent black, blurred by the
    /// distance of its plane from the focus plane ([`DepthBlurNode`]), and the
    /// result drawn in its place with its blend mode.
    fn depth_of_field(
        &mut self,
        comp: &RenderComp,
        index: usize,
        op: LayerOp,
        canvas: IRect,
        scale: f64,
        depth: usize,
    ) -> LayerOp {
        let camera = comp.view_camera();
        let Some(matrix) = comp.layers[index]
            .matrix_3d
            .filter(|_| !matches!(comp.layers[index].content, LayerContent::Model(_)))
        else {
            return op;
        };
        if !camera.depth_of_field
            || camera.projection != crate::camera::Projection::Perspective
            || camera.focus_distance <= 0.0
            || camera.aperture <= 0.0
            || camera.blur_level <= 0.0
            || op.blend.is_stencil()
            || op.preserve_transparency
        {
            return op;
        }
        let focus = camera.focus_distance;
        let coc = camera.blur_level / 100.0 * camera.aperture * camera.zoom / focus;
        // The plane's homography has the camera depth as w, so 1 / depth is
        // affine on screen (composite pixels are comp pixels).
        let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
        let inv = camera.plane_homography(&matrix, size).inverse();
        let inv_depth = [inv.x_axis.z, inv.y_axis.z, inv.z_axis.z];
        // The blur is largest at a corner of the drawn area.
        let b = op.bounds;
        let max_radius = [(b.x0, b.y0), (b.x1, b.y0), (b.x0, b.y1), (b.x1, b.y1)]
            .into_iter()
            .map(|(x, y)| {
                let (x, y) = (f64::from(x) * scale, f64::from(y) * scale);
                let inv_z = inv_depth[0] * x + inv_depth[1] * y + inv_depth[2];
                if inv_z <= 0.0 || 1.0 / inv_z <= crate::camera::NEAR {
                    MAX_DOF_RADIUS
                } else {
                    0.5 * coc * (1.0 - focus * inv_z).abs()
                }
            })
            .fold(0.0, f64::max)
            .min(MAX_DOF_RADIUS);
        // Under half a texel everywhere: sharp.
        if max_radius < 0.5 * scale {
            return op;
        }
        let grow = (max_radius / scale).ceil() as i32 + 1;
        let rect = IRect::new(
            op.bounds.x0 - grow,
            op.bounds.y0 - grow,
            op.bounds.x1 + grow,
            op.bounds.y1 + grow,
        )
        .intersect(&canvas);
        if rect.is_empty() {
            return op;
        }
        let blend = op.blend;
        let dissolve_seed = op.dissolve_seed;
        let layer = op.layer.clone();
        let key = hash_of(&(
            "depth of field",
            &comp.path,
            &layer,
            op.clips
                .iter()
                .map(|c| c.map(f64::to_bits))
                .collect::<Vec<_>>(),
            rect,
            scale.to_bits(),
            self.nodes.len(),
        ));
        let clamp = self.clamp;
        let blurred = self.owned(comp, index, depth, |b| {
            let mut alone = op;
            alone.blend = BlendMode::Normal;
            let input = b.push(Node {
                kind: NodeKind::Composite(CompositeNode {
                    prim: comp.path.clone(),
                    time: comp.time,
                    ops: vec![alone],
                    clamp,
                }),
                rect,
                pixel_scale: scale,
                key,
                cacheable: false,
            });
            b.push(Node {
                kind: NodeKind::DepthBlur(DepthBlurNode {
                    layer: layer.clone(),
                    input,
                    inv_depth,
                    focus,
                    coc,
                    max_radius,
                    iris: camera.iris,
                }),
                rect,
                pixel_scale: scale,
                key: hash_of(&("depth blur", key)),
                cacheable: false,
            })
        });
        LayerOp {
            layer,
            input: OpInput::Image(blurred),
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend,
            dissolve_seed,
            quality: bae_anim::LayerQuality::Best,
            preserve_transparency: false,
            matte: None,
            mask: None,
            bounds: rect,
            content: Rect::new(
                f64::from(rect.x0) * scale,
                f64::from(rect.y0) * scale,
                f64::from(rect.x1) * scale,
                f64::from(rect.y1) * scale,
            ),
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
        }
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
            let layer = &comp.layers[index];
            if let Some(styles) = &layer.layer_styles
                && styles.enabled
                && let Some(effect) = b.effects.get(&styles.effect).cloned()
            {
                let mut isolated = Vec::new();
                b.styled_layer(
                    comp,
                    index,
                    canvas,
                    scale,
                    depth,
                    visiting,
                    effect,
                    styles,
                    &mut isolated,
                )?;
                return Ok(isolated.pop());
            }
            b.plan_layer(comp, index, canvas, scale, depth, visiting, true)
        })
    }

    /// A 3D model layer: its model drawn through the comp's camera
    /// ([`MeshNode`]), the image drawn in the layer's place. The frame is
    /// incomplete while the model loads.
    fn plan_model(
        &mut self,
        comp: &RenderComp,
        index: usize,
        model: &crate::input::ModelRef,
        canvas: IRect,
        scale: f64,
        depth: usize,
    ) -> Result<Option<LayerOp>, RenderError> {
        let layer = &comp.layers[index];
        if layer.opacity <= 0.0 {
            return Ok(None);
        }
        let Some(file) = &model.file else {
            return Ok(None);
        };
        let mesh = match crate::model::get(file, !crate::is_interactive_thread()) {
            crate::model::State::Ready(mesh) => mesh,
            crate::model::State::Loading => {
                self.incomplete = true;
                return Ok(None);
            }
            crate::model::State::Failed(e) => {
                self.warn(format!("{}: {e}", layer.path));
                return Ok(None);
            }
        };
        let matrix = model_matrix(comp, layer);
        let camera = comp.view_camera();
        // Where the model's box lands on screen (all of the canvas when a
        // corner is behind the camera).
        let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
        let mut lo = DVec2::splat(f64::MAX);
        let mut hi = DVec2::splat(f64::MIN);
        let mut everywhere = false;
        for k in 0..8 {
            let c = glam::DVec3::new(
                if k & 1 == 0 { -0.5 } else { 0.5 },
                if k & 2 == 0 { -0.5 } else { 0.5 },
                if k & 4 == 0 { -0.5 } else { 0.5 },
            );
            match camera.project(matrix.transform_point3(c), size) {
                Some(p) => {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
                None => everywhere = true,
            }
        }
        let rect = if everywhere {
            canvas
        } else {
            units(
                &Rect::new(lo.x - 2.0, lo.y - 2.0, hi.x + 2.0, hi.y + 2.0),
                scale,
            )
            .intersect(&canvas)
        };
        if rect.is_empty() {
            return Ok(None);
        }
        let lights: Arc<[_]> = comp
            .lights
            .iter()
            .filter(|l| l.scope_below.is_none_or(|light| index > light))
            .copied()
            .collect();
        let key = hash_of(&(
            "3d model",
            &layer.path,
            Arc::as_ptr(&mesh) as usize,
            matrix.to_cols_array().map(f64::to_bits),
            comp.view_camera().world.to_cols_array().map(f64::to_bits),
            comp.view_camera().zoom.to_bits(),
            lights.len(),
            rect,
            scale.to_bits(),
            self.nodes.len(),
        ));
        let environment = match lights.iter().find(|l| l.environment.is_some()) {
            Some(light) => self.environment_input(comp, light, depth)?,
            None => None,
        };
        let key = hash_of(&(key, environment.map(|e| self.nodes[e.input.index()].key)));
        let linear = self.linear;
        let node = self.push(Node {
            kind: NodeKind::Mesh(Box::new(MeshNode {
                models: vec![MeshModel {
                    layer: layer.path.clone(),
                    model: mesh,
                    matrix,
                    lights,
                    opacity: layer.opacity.clamp(0.0, 1.0),
                    environment,
                    casts_shadows: layer.material.casts_shadows != crate::light::ShadowMode::Off,
                    accepts_shadows: layer.material.accepts_shadows
                        != crate::light::ShadowMode::Off,
                }],
                camera,
                comp_size: [comp.width, comp.height],
                linear,
            })),
            rect,
            pixel_scale: scale,
            key,
            cacheable: false,
        });
        Ok(Some(LayerOp {
            layer: layer.path.clone(),
            input: OpInput::Image(node),
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend: layer.blend_mode,
            dissolve_seed: layer.blend_mode.dissolve_seed(layer.path.as_str(), comp),
            // Best in final renders; previews follow the layer's switch.
            quality: layer.quality,
            preserve_transparency: layer.preserve_transparency,
            matte: None,
            mask: None,
            bounds: rect,
            content: Rect::new(
                f64::from(rect.x0) * scale,
                f64::from(rect.y0) * scale,
                f64::from(rect.x1) * scale,
                f64::from(rect.y1) * scale,
            ),
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
        }))
    }

    /// The picture of environment light `light` (AE's Source) as a node
    /// input: its source layer's pixels with masks and effects. The map is
    /// 512 texels wide for lighting, 2048 when it also shows as the
    /// background (never more than the layer has).
    fn environment_input(
        &mut self,
        comp: &RenderComp,
        light: &crate::light::Light,
        depth: usize,
    ) -> Result<Option<EnvironmentInput>, RenderError> {
        let Some(env) = light.environment else {
            return Ok(None);
        };
        let Some(layer) = comp.layers.get(env.layer) else {
            return Ok(None);
        };
        let source_width = layer
            .content
            .known_size()
            .map_or(comp.width, |s| s[0])
            .max(1);
        let cap = if env.background.is_some() { 2048 } else { 512 };
        let width = source_width.next_power_of_two().clamp(16, cap);
        // About two layer texels per map texel (level 0 takes four taps).
        let scale = (f64::from(source_width) / f64::from(width * 2)).max(1.0);
        let Some(input) =
            self.layer_image(comp, layer, LayerStage::EffectsAndMasks, scale, depth)?
        else {
            return Ok(None);
        };
        let k = light.intensity / 100.0;
        Ok(Some(EnvironmentInput {
            input,
            width,
            rotation: crate::exec::env::rotation(&light.world),
            tint: light.color.map(|c| c * k),
        }))
    }

    /// AE's Background Visible: the first environment light that shows its
    /// picture draws it under every layer of the comp.
    fn sky_op(
        &mut self,
        comp: &RenderComp,
        canvas: IRect,
        scale: f64,
        depth: usize,
    ) -> Result<Option<LayerOp>, RenderError> {
        let Some((light, [opacity, blur])) = comp
            .lights
            .iter()
            .find_map(|l| Some((l, l.environment?.background?)))
        else {
            return Ok(None);
        };
        if opacity <= 0.0 || canvas.is_empty() {
            return Ok(None);
        }
        let Some(environment) = self.environment_input(comp, light, depth)? else {
            return Ok(None);
        };
        let camera = comp.view_camera();
        let key = hash_of(&(
            "sky",
            &comp.path,
            self.nodes[environment.input.index()].key,
            environment.rotation.to_cols_array().map(f64::to_bits),
            camera.world.to_cols_array().map(f64::to_bits),
            camera.zoom.to_bits(),
            [opacity, blur].map(f32::to_bits),
            canvas,
            scale.to_bits(),
        ));
        let linear = self.linear;
        let node = self.push(Node {
            kind: NodeKind::Sky(Box::new(SkyNode {
                comp: comp.path.clone(),
                environment,
                camera,
                comp_size: [comp.width, comp.height],
                opacity,
                blur,
                linear,
            })),
            rect: canvas,
            pixel_scale: scale,
            key,
            cacheable: false,
        });
        Ok(Some(LayerOp {
            layer: comp.path.clone(),
            input: OpInput::Image(node),
            transform: DAffine2::IDENTITY,
            opacity: 1.0,
            blend: BlendMode::Normal,
            dissolve_seed: 0,
            quality: bae_anim::LayerQuality::Best,
            preserve_transparency: false,
            matte: None,
            mask: None,
            bounds: canvas,
            content: Rect::new(
                f64::from(canvas.x0) * scale,
                f64::from(canvas.y0) * scale,
                f64::from(canvas.x1) * scale,
                f64::from(canvas.y1) * scale,
            ),
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_layer(
        &mut self,
        comp: &RenderComp,
        index: usize,
        canvas: IRect,
        scale: f64,
        depth: usize,
        visiting: &mut Vec<usize>,
        apply_matte: bool,
    ) -> Result<Option<LayerOp>, RenderError> {
        let layer = &comp.layers[index];
        if let LayerContent::Model(model) = &layer.content {
            return self.plan_model(comp, index, model, canvas, scale, depth);
        }
        let motion_samples = if self.native_motion_blur {
            &layer.motion_samples[..]
        } else {
            &[]
        };
        let flat = layer.matrix_3d.is_none() && motion_samples.is_empty();
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
        let placement = placement(comp, layer, motion_samples, &content_rect, scale);
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
            layer_start_time: layer.start_time,
            layer_time_scale: layer.time_scale,
            layer_in: layer.in_point,
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

        let matte = match layer.track_matte.filter(|_| apply_matte) {
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
            if effect.full_surface() {
                roi = bounds_chain[i + 1];
                rois[i + 1] = roi;
            }
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
                    // Motion blur: the layer, its lights and the eye at every
                    // sample.
                    let samples: Arc<[_]> = match &comp.motion_blur {
                        Some(mb) if mb.lights.len() == motion_samples.len() => motion_samples
                            .iter()
                            .zip(&mb.lights)
                            .enumerate()
                            .filter_map(|(k, (s, sample_lights))| {
                                Some(crate::light::LightingSample {
                                    matrix: s.matrix_3d?,
                                    lights: sample_lights
                                        .iter()
                                        .filter(|l| l.scope_below.is_none_or(|light| index > light))
                                        .copied()
                                        .collect(),
                                    eye: comp.sample_camera(k).eye(),
                                })
                            })
                            .collect(),
                        _ => Arc::new([]),
                    };
                    Arc::new(Lighting {
                        matrix,
                        material: layer.material,
                        lights,
                        eye: camera.eye(),
                        world_per_pixel: camera.world_per_pixel(),
                        casters: Arc::new([]),
                        samples,
                        rect: content_rect,
                        shadow_map: None,
                        models: Arc::new([]),
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
            quality: layer.quality,
            preserve_transparency: layer.preserve_transparency,
            matte,
            mask: op_mask,
            bounds,
            content: content_px,
            projection,
            motion,
            clips: Vec::new(),
            lighting,
            occlusion: None,
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
                quality: bae_anim::LayerQuality::Best,
                preserve_transparency: false,
                matte: None,
                mask: None,
                bounds: roi,
                content,
                projection: None,
                motion: None,
                clips: Vec::new(),
                lighting: None,
                occlusion: None,
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
            quality: bae_anim::LayerQuality::Best,
            preserve_transparency: false,
            matte: None,
            mask: Some(mask),
            bounds: roi,
            content,
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
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
            layer_start_time: layer.start_time,
            layer_time_scale: layer.time_scale,
            layer_in: layer.in_point,
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
            if effects[i].0.full_surface() {
                rois[i + 1] = domain;
            }
            rois[i] = effects[i].0.required_input(&params, rois[i + 1], domain);
        }
        let base_rect = rois[0].union(&canvas).intersect(&domain);

        let mut h = FxHasher::default();
        "adjustment base".hash(&mut h);
        comp.hash_content(&mut h, Some(self.effects));
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
                comp.hash_content(&mut h, Some(self.effects));
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
            quality: bae_anim::LayerQuality::Best,
            preserve_transparency: false,
            matte: None,
            mask: None,
            bounds: canvas,
            content: canvas_px,
            projection: None,
            motion: None,
            clips: Vec::new(),
            lighting: None,
            occlusion: None,
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
        mut instance: RenderEffect,
        frame: EffectFrame,
        input: NodeId,
        rect: IRect,
        scale: f64,
        depth: usize,
    ) -> Result<NodeId, RenderError> {
        effect.prepare_scene(&mut instance, comp, layer);
        let mut h = FxHasher::default();
        "effect".hash(&mut h);
        self.nodes[input.index()].key.hash(&mut h);
        // Effects may vary with time on their own (noise evolution, wiggle).
        layer.time.hash(&mut h);
        instance.path.hash(&mut h);
        crate::input::hash_value(&bae_model::Value::String(instance.effect.clone()), &mut h);
        for (id, value) in &instance.params {
            id.hash(&mut h);
            crate::input::hash_value(value, &mut h);
        }
        rect.hash(&mut h);
        let params = frame.params(&instance);
        let feedback = effect.feedback_source(&params, &layer.effects);
        self.time_dependent |= feedback.is_some();
        let scene = effect.uses_scene(&params).then(|| {
            let world = layer.matrix_3d.unwrap_or_else(|| {
                let m = layer.transform;
                DMat4::from_cols(
                    m.matrix2.x_axis.extend(0.).extend(0.),
                    m.matrix2.y_axis.extend(0.).extend(0.),
                    glam::DVec4::Z,
                    m.translation.extend(0.).extend(1.),
                )
            });
            let projection = comp
                .view_camera()
                .plane_homography(&world, DVec2::new(comp.width.into(), comp.height.into()));
            let light = comp.lights.first().map(|light| {
                let local = world
                    .inverse()
                    .transform_point3(light.position())
                    .to_array();
                let color = light.color.map(|v| v * light.intensity * 0.01);
                (local, color)
            });
            projection.to_cols_array().map(f64::to_bits).hash(&mut h);
            if let Some((position, color)) = light {
                position.map(f64::to_bits).hash(&mut h);
                color.map(f32::to_bits).hash(&mut h);
            }
            let layer_light = comp
                .lights
                .iter()
                .find(|light| {
                    light.scope_below.is_none_or(|index| {
                        comp.layers
                            .iter()
                            .skip(index + 1)
                            .any(|l| l.path == layer.path)
                    })
                })
                .copied();
            [comp.width, comp.height].hash(&mut h);
            for v in layer.transform.to_cols_array() {
                v.to_bits().hash(&mut h);
            }
            crate::input::hash_camera(&comp.camera, &mut h);
            if let Some(light) = &layer_light {
                crate::input::hash_light(light, &mut h);
            }
            Box::new(crate::effect::EffectScene {
                projection,
                light,
                camera: comp.camera,
                size: [comp.width, comp.height],
                placement: layer.transform,
                layer_light,
            })
        });
        let mut tracks = Vec::new();
        let mut parameter_samples = Vec::new();
        if let Some(frames) = self.frames.as_mut() {
            for &id in effect.integrated_parameters() {
                if let Some(track) = frames.scalar_track(&instance.path, id, &frame) {
                    (id, track.fingerprint()).hash(&mut h);
                    tracks.push((id, track));
                }
            }
            let params = EffectParams {
                tracks: &tracks,
                ..params
            };
            if frames.animated_controls(&instance.path, effect.parameter_ids()) {
                let times = effect.parameter_times(&params);
                parameter_samples =
                    frames.parameters_at(&instance, effect.parameter_ids(), &frame, &times);
                for sample in &parameter_samples {
                    sample.time.hash(&mut h);
                    for (id, value) in &sample.values {
                        id.hash(&mut h);
                        crate::input::hash_value(value, &mut h);
                    }
                }
            }
            self.time_dependent |= !parameter_samples.is_empty() || !tracks.is_empty();
        }
        let params = EffectParams {
            parameter_samples: &parameter_samples,
            tracks: &tracks,
            ..params
        };
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
        let paths: Vec<_> = match effect.path_dependencies(&params) {
            Some(MaskSelect::All) => layer.masks.clone(),
            Some(MaskSelect::One(path)) => layer
                .masks
                .iter()
                .filter(|mask| mask.path == path)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        for mask in &paths {
            mask.hash_into(&mut h);
        }
        let mut history = Vec::new();
        for time in effect.parameter_sample_times(&params) {
            let sample = if time == comp.time {
                Some(instance.clone())
            } else {
                self.time_dependent = true;
                match self.frames.as_mut() {
                    Some(frames) => {
                        frames.effect_at(&comp.path, &layer.path, &instance.path, time)?
                    }
                    None => None,
                }
            };
            if let Some(sample) = sample {
                time.hash(&mut h);
                sample.hash_into(&mut h);
                history.push((time, sample));
            }
        }
        let samples = (!paths.is_empty()
            || !history.is_empty()
            || !parameter_samples.is_empty()
            || !tracks.is_empty())
        .then(|| {
            Box::new(EffectSamples {
                paths,
                history,
                parameter_samples,
                tracks,
            })
        });
        Ok(self.push(Node {
            kind: NodeKind::Effect(Box::new(EffectNode {
                feedback,
                scene,
                layer: layer.path.clone(),
                effect,
                instance,
                frame,
                input,
                layers,
                masks,
                samples,
            })),
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
        if matches!(dep.stage, LayerStage::BeforeEffect { .. }) && !layer.active {
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
        let native = self.native_motion_blur;
        self.native_motion_blur &= !matches!(
            stage,
            LayerStage::BeforeEffect {
                native_motion_blur: false,
                ..
            }
        );
        let result = self.layer_image(at, layer, stage, scale, depth);
        self.native_motion_blur = native;
        result
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
        let backdrop = layer.adjustment && matches!(stage, LayerStage::BeforeEffect { .. });
        let (content_rect, mut node) = if backdrop {
            let index = comp
                .layers
                .iter()
                .position(|l| l.path == layer.path)
                .expect("layer of comp");
            let rect = comp.rect();
            (
                rect,
                self.composite_from(
                    comp,
                    index + 1,
                    units(&rect, scale),
                    scale,
                    depth + 1,
                    false,
                )?,
            )
        } else {
            let Some(content) = self.plan_content(layer, layer.time)? else {
                return Ok(None);
            };
            let content_rect = match &content {
                Content::Solid { rect, .. } | Content::Fill { rect, .. } => *rect,
                Content::Source(src, _) | Content::Mix(src, _) => src.bounds().to_rect(),
                Content::Comp(c) => c.rect(),
            };
            let roi = units(&content_rect, scale);
            let node = match content {
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
                Content::Source(source, time) => {
                    self.effect_source(layer, &source, time, roi, scale)
                }
                Content::Mix(source, frames) => {
                    self.mix(layer, &frames, roi, scale, content_rect, |b, time| {
                        b.effect_source(layer, &source, time, roi, scale)
                    })
                }
                Content::Comp(nested) => self.composite(&nested, roi, scale, depth + 1, true)?,
            };
            (content_rect, node)
        };
        let roi = units(&content_rect, scale);
        if !backdrop
            && stage != LayerStage::Source
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
            LayerStage::BeforeEffect { effect: path, .. } => layer
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
                layer_start_time: layer.start_time,
                layer_time_scale: layer.time_scale,
                layer_in: layer.in_point,
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
            LayerContent::None | LayerContent::Model(_) => None,
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
                            if let Some(interpolator) = self.effects.frame_interpolator() {
                                let time = footage.source_time;
                                let source = frame_blend::pixel_motion(
                                    source,
                                    interpolator,
                                    &layer.path,
                                    time,
                                    rate,
                                );
                                return Ok(Some(Content::Source(source, time)));
                            }
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
                // Drawn again when it is ready (the frame is not kept).
                Err(error) if error.is_warming_up() => {
                    self.incomplete = true;
                    None
                }
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
                Some(effect) if effect.renders_pixels() => out.push(
                    effect
                        .resolve_alias(instance)
                        .unwrap_or_else(|| (effect.clone(), instance.clone())),
                ),
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
fn placement(
    comp: &RenderComp,
    layer: &RenderLayer,
    samples: &[crate::LayerSample],
    content: &Rect,
    scale: f64,
) -> Placement {
    let size = DVec2::new(f64::from(comp.width), f64::from(comp.height));
    if let Some(m) = layer.matrix_3d {
        let frame = Homography(comp.view_camera().plane_homography(&m, size));
        let motion = (!samples.is_empty())
            .then(|| {
                samples
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
    if samples.len() > 1 && !crate::motion::is_static(samples) {
        let transforms: Vec<DAffine2> = samples.iter().map(|s| s.transform).collect();
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
            comp.layers[*index].material.casts_shadows != ShadowMode::Off
                && op.opacity > 0.0
                && !matches!(comp.layers[*index].content, LayerContent::Model(_))
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
                    footprints: Arc::new([]),
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
        // Per light, where each caster's shadow can land: casters that miss
        // the receiver for every light are dropped, the rest are skipped by
        // the shader outside their footprint.
        let mine: Arc<[ShadowCaster]> = casters
            .iter()
            .filter(|(i, _)| i != index)
            .filter_map(|(_, c)| {
                let footprints: Arc<[Rect]> = lighting
                    .lights
                    .iter()
                    .map(|l| {
                        if !l.casts_shadows {
                            return Rect::new(1.0, 1.0, 0.0, 0.0);
                        }
                        match shadow_footprint(&receiver, c, l) {
                            None => Rect::new(1.0, 1.0, 0.0, 0.0),
                            Some(None) => Rect::new(f64::MIN, f64::MIN, f64::MAX, f64::MAX),
                            Some(Some(b))
                                if b.x1 <= rect.x0
                                    || b.x0 >= rect.x1
                                    || b.y1 <= rect.y0
                                    || b.y0 >= rect.y1 =>
                            {
                                Rect::new(1.0, 1.0, 0.0, 0.0)
                            }
                            Some(Some(b)) => b,
                        }
                    })
                    .collect();
                footprints
                    .iter()
                    .any(|f| !f.is_empty())
                    .then(|| ShadowCaster {
                        footprints,
                        ..c.clone()
                    })
            })
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

/// Where `caster` can throw a shadow of `light` on the receiver plane
/// `receiver`, in receiver layer pixels with the softest edge included
/// (conservative): `None` nowhere (behind the receiver as seen from the
/// light), `Some(None)` anywhere (a corner's shadow is unbounded).
fn shadow_footprint(
    receiver: &glam::DMat4,
    caster: &crate::light::ShadowCaster,
    light: &crate::light::Light,
) -> Option<Option<Rect>> {
    use crate::light::LightKind;
    use glam::DVec3;
    let Some(n) = receiver
        .x_axis
        .truncate()
        .cross(receiver.y_axis.truncate())
        .try_normalize()
    else {
        return Some(None);
    };
    let d = -n.dot(receiver.w_axis.truncate());
    let side = |p: DVec3| n.dot(p) + d;
    let light_side = match light.kind {
        LightKind::Parallel => -n.dot(light.direction()),
        _ => side(light.position()),
    };
    if light_side.abs() < 1e-9 {
        return Some(None);
    }
    // The caster grown by its softest shadow edge (diffusion plus the
    // shader's minimum softness), in its own pixels.
    let caster_px = caster.matrix.x_axis.truncate().length().max(1e-12);
    let grow = (f64::from(light.shadow_diffusion) + 8.0) / caster_px;
    let r = caster.rect;
    let grown = Rect::new(r.x0 - grow, r.y0 - grow, r.x1 + grow, r.y1 + grow);
    let corners = grown
        .corners()
        .map(|c| caster.matrix.transform_point3(c.extend(0.0)));
    // Only the part on the light's side of the receiver plane casts onto it
    // (a point on the plane shadows itself).
    let ahead = |p: DVec3| side(p) * light_side.signum();
    let mut poly: Vec<DVec3> = Vec::with_capacity(8);
    for i in 0..4 {
        let (a, b) = (corners[i], corners[(i + 1) % 4]);
        let (sa, sb) = (ahead(a), ahead(b));
        if sa > 0.0 {
            poly.push(a);
        }
        if (sa > 0.0) != (sb > 0.0) {
            poly.push(a + (b - a) * (sa / (sa - sb)));
        }
    }
    if poly.is_empty() {
        // Behind the receiver (or in its plane) as seen from the light.
        return None;
    }
    let mut shadows = Vec::with_capacity(poly.len());
    for v in poly {
        let sv = side(v);
        let p = match light.kind {
            LightKind::Parallel => {
                let dir = light.direction();
                v - dir * (sv / n.dot(dir))
            }
            _ => {
                let lp = light.position();
                // At or past the light's own plane: the shadow is unbounded.
                if sv.abs() >= 0.999 * light_side.abs() {
                    return Some(None);
                }
                lp + (v - lp) * (light_side / (light_side - sv))
            }
        };
        shadows.push(p);
    }
    let to_layer = receiver.inverse();
    let pts: Vec<glam::DVec2> = shadows
        .iter()
        .map(|p| to_layer.transform_point3(*p).truncate())
        .collect();
    let b = crate::three_d::points_bounds(&pts);
    // Anti-aliasing margin.
    let pad = 2.0;
    Some(Some(Rect::new(
        b.x0 - pad,
        b.y0 - pad,
        b.x1 + pad,
        b.y1 + pad,
    )))
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

pub(crate) fn hash_of(value: &impl Hash) -> u64 {
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
        // A nested comp's node key follows its content (identical pre-comp
        // instances share a node); the root's only has to be stable.
        let mut inner = RenderComp::new(p("/comps/inner"), 50, 50);
        inner.layers.push(solid("a", 10, 10, [1.0; 4], (0.0, 0.0)));
        let nested_key = |inner: &RenderComp| {
            let mut comp = RenderComp::new(p("/comps/main"), 100, 100);
            comp.layers.push(RenderLayer::new(
                p("/comps/main/layers/pre"),
                LayerContent::Comp(Arc::new(inner.clone())),
            ));
            let g = build_default(&comp, &RenderRequest::full());
            let nested = g
                .nodes
                .iter()
                .find(|n| matches!(&n.kind, NodeKind::Composite(c) if c.prim == inner.path))
                .map(|n| n.key);
            (g.output_node().key, nested.expect("nested composite"))
        };
        let (root1, k1) = nested_key(&inner);
        let (root2, k2) = nested_key(&inner);
        assert_eq!((root1, k1), (root2, k2));
        inner.layers[0].opacity = 0.5;
        let (_, k3) = nested_key(&inner);
        assert_ne!(k1, k3);
    }
}

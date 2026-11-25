//! The evaluation result handed to the renderer (04), the UI and agents.
//!
//! An [`EvaluatedComp`] is one comp at one comp time, flattened: every layer's final
//! transform (local and world matrices), opacity, visibility, source time, masks,
//! effect parameter values and shape/text contents, with keyframes and (later)
//! expressions already applied. Consumers never evaluate animation themselves.
//!
//! Units follow AE except where a field says otherwise: [`EvaluatedLayer::opacity`]
//! and [`EvaluatedMask::opacity`] are normalised to 0–1 for the renderer, while
//! [`Transform::opacity`] keeps AE's percent.

use std::collections::BTreeMap;

use bae_model::{BezierPath, FrameRate, LayerKind, PrimPath, PrimType, TextDocument, Time, Value};
use glam::{DMat4, DVec3};
use serde::Serialize;

use crate::time::{LayerTiming, MotionBlur};
use crate::transform::Transform;

/// One comp evaluated at one time.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluatedComp {
    pub path: PrimPath,
    /// The comp time that was evaluated.
    pub time: Time,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub frame_rate: FrameRate,
    pub duration: Time,
    pub bg_color: [f64; 4],
    pub motion_blur: MotionBlur,
    /// Layers in AE stacking order: `layers[0]` is the top layer (AE index 1). The
    /// renderer composites from the last to the first. Every layer is listed, including
    /// invisible ones (track mattes, parents, cameras), so indices are stable.
    pub layers: Vec<EvaluatedLayer>,
    /// Index of the active camera: the topmost camera layer that is enabled and active
    /// at this time. `None` = AE's default comp camera.
    pub camera: Option<usize>,
    /// Attribute evaluation problems (expression errors, type mismatches), shown on the
    /// offending property in the UI like AE's expression error bar.
    pub errors: Vec<AttrError>,
}

impl EvaluatedComp {
    /// Layer by prim path.
    pub fn layer(&self, path: &PrimPath) -> Option<&EvaluatedLayer> {
        self.layers.iter().find(|l| &l.path == path)
    }

    /// Whether any layer has its solo switch on (then only soloed layers are visible).
    pub fn has_solo(&self) -> bool {
        self.layers.iter().any(|l| l.switches.solo)
    }
}

/// One layer at one time.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluatedLayer {
    pub path: PrimPath,
    /// Display name (`name` attribute, falling back to the prim name).
    pub name: String,
    pub kind: LayerKind,
    pub timing: LayerTiming,
    /// The layer time that was evaluated (keyframe time base, see [`crate::time`]).
    pub layer_time: Time,
    /// Between its in and out points at this time.
    pub active: bool,
    /// Would be drawn as normal content: video switch on, active, and not hidden by
    /// another layer's solo. Track mattes are still used when invisible; guide layers
    /// are visible here and skipped by the renderer when rendering for output.
    pub visible: bool,
    pub switches: LayerSwitches,
    /// Where the layer's pixels come from, with the time to sample them at.
    pub source: LayerSource,
    /// Comp times at which to sample this layer for motion blur (AE-ANI-103), in
    /// shutter order. Empty when motion blur is off for the comp or the layer.
    pub motion_blur_times: Vec<Time>,
    /// Index of the parent layer in [`EvaluatedComp::layers`].
    pub parent: Option<usize>,
    pub transform: Transform,
    /// Layer space → parent space (or comp space without a parent).
    pub local_matrix: DMat4,
    /// Layer space → comp space, parents applied (AE-ANI-015/016).
    pub world_matrix: DMat4,
    /// 0–1, not inherited from parents.
    pub opacity: f64,
    /// Blend mode token (`bae_model::schema::BLEND_MODES`).
    pub blend_mode: String,
    pub track_matte: TrackMatte,
    /// Index of the matte layer (the layer above when the attribute is unset).
    pub track_matte_layer: Option<usize>,
    pub masks: Vec<EvaluatedMask>,
    pub effects: Vec<EvaluatedEffect>,
    /// Shape layer contents (groups, paths, fills, operators …), in order.
    pub contents: Vec<EvaluatedPrim>,
    /// Text layers: the source text at this time (`text:source`).
    pub text: Option<Box<TextDocument>>,
    /// Text layers: animators with their selectors (`animators` folder), evaluated.
    pub text_animators: Vec<EvaluatedPrim>,
    pub camera: Option<CameraState>,
    pub light: Option<LightState>,
    /// Other layers this layer's expressions read at this time (`thisComp.layer(…)`,
    /// `parent`, `comp(…)`), so caches can invalidate it when they change.
    pub depends_on: Vec<PrimPath>,
}

/// Layer switches that matter for rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LayerSwitches {
    /// Video switch (eye).
    pub enabled: bool,
    /// Effect switch (fx): the layer's effects render (AE default on).
    pub effects: bool,
    pub solo: bool,
    pub shy: bool,
    pub guide: bool,
    pub adjustment: bool,
    pub three_d: bool,
    pub motion_blur: bool,
    pub collapse_transformation: bool,
    /// AE "Preserve Underlying Transparency" (T switch).
    pub preserve_transparency: bool,
}

/// Where a layer's pixels come from.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerSource {
    /// Generated content or no pixels: shape, text, null, camera, light, adjustment
    /// layers without a source.
    None,
    /// Footage or solid (`Asset` prim). `time` is the source time after start time,
    /// stretch and time remap (AE-ANI-090); choosing the frame for footage of a given
    /// frame rate is the media layer's job (AE-ANI-095).
    Asset {
        asset: PrimPath,
        time: Time,
        info: AssetInfo,
    },
    /// Pre-comp. `time` is the inner comp time (AE-ANI-093/094: already snapped when
    /// the inner comp preserves its frame rate). `nested` is the inner comp evaluated
    /// at that time (recursively); `None` when the comp is missing or nesting is cyclic.
    Comp {
        comp: PrimPath,
        time: Time,
        nested: Option<Box<EvaluatedComp>>,
    },
}

/// What a layer needs to know about its `Asset` source. Resolving the file on disk is
/// `bae_model::AssetResolver`'s job.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AssetInfo {
    /// `footage` / `solid` / `placeholder` / `sequence`.
    pub kind: String,
    pub width: u32,
    pub height: u32,
    pub pixel_aspect: f64,
    pub frame_rate: Option<FrameRate>,
    /// `None` for stills and solids.
    pub duration: Option<Time>,
    /// Solids: the fill colour.
    pub solid_color: Option<[f64; 4]>,
}

/// Track matte mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackMatte {
    #[default]
    None,
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}

impl TrackMatte {
    pub fn from_token(s: &str) -> TrackMatte {
        match s {
            "alpha" => TrackMatte::Alpha,
            "alpha_inverted" => TrackMatte::AlphaInverted,
            "luma" => TrackMatte::Luma,
            "luma_inverted" => TrackMatte::LumaInverted,
            _ => TrackMatte::None,
        }
    }
}

/// A mask at one time, in layer space.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluatedMask {
    pub path: PrimPath,
    /// `none` / `add` / `subtract` / `intersect` / `lighten` / `darken` / `difference`.
    pub mode: String,
    pub inverted: bool,
    pub shape: BezierPath,
    /// Pixels, x and y.
    pub feather: [f64; 2],
    /// 0–1.
    pub opacity: f64,
    /// Pixels.
    pub expansion: f64,
}

/// An effect instance at one time.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluatedEffect {
    pub path: PrimPath,
    pub match_name: String,
    pub enabled: bool,
    /// Every `param:NNNN` attribute, evaluated, keyed by its numeric ID (`"0001"`).
    pub params: BTreeMap<String, Value>,
    /// Child prims, generically evaluated (the Paint effect's strokes).
    pub children: Vec<EvaluatedPrim>,
}

impl EvaluatedEffect {
    /// Parameter by numeric ID (`1` → `param:0001`).
    pub fn param(&self, id: u32) -> Option<&Value> {
        self.params.get(&format!("{id:04}"))
    }
}

/// Any prim below a layer, generically evaluated: every attribute at this time.
/// Used for shape contents (and later text animators), whose interpretation belongs
/// to 05.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EvaluatedPrim {
    pub path: PrimPath,
    pub prim_type: Option<PrimType>,
    pub attrs: BTreeMap<String, Value>,
    pub children: Vec<EvaluatedPrim>,
}

impl EvaluatedPrim {
    pub fn get(&self, attr: &str) -> Option<&Value> {
        self.attrs.get(attr)
    }

    pub fn f64(&self, attr: &str) -> Option<f64> {
        self.attrs.get(attr).and_then(Value::as_f64)
    }
}

/// Camera layer state (AE-ANI-009/022).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CameraState {
    /// Two-node cameras aim at their point of interest.
    pub two_node: bool,
    /// Pixels: the distance from the camera at which a z = 0 layer appears at 100 %.
    pub zoom: f64,
    pub depth_of_field: bool,
    pub focus_distance: f64,
    pub aperture: f64,
    /// Percent.
    pub blur_level: f64,
    /// In the camera's parent space.
    pub point_of_interest: DVec3,
    /// Camera space → comp space. The camera looks along local +z, local +y is
    /// screen-down. The view matrix is its inverse.
    pub world_matrix: DMat4,
}

/// Light layer state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct LightState {
    pub light_type: LightType,
    /// Percent.
    pub intensity: f64,
    pub color: [f64; 4],
    pub cone_angle: f64,
    pub cone_feather: f64,
    pub falloff: LightFalloff,
    pub radius: f64,
    pub falloff_distance: f64,
    pub casts_shadows: bool,
    pub shadow_darkness: f64,
    pub shadow_diffusion: f64,
    pub point_of_interest: DVec3,
    /// Light space → comp space; spot/parallel lights shine along local +z.
    pub world_matrix: DMat4,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LightType {
    Parallel,
    #[default]
    Spot,
    Point,
    Ambient,
    Environment,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LightFalloff {
    #[default]
    None,
    Smooth,
    InverseSquareClamped,
}

/// A problem evaluating one attribute. Evaluation continues with the attribute's
/// value before the failing step (AE shows the pre-expression value when an
/// expression fails).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AttrError {
    pub prim: PrimPath,
    pub attr: String,
    pub message: String,
}

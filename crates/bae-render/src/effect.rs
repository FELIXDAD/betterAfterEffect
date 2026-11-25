//! The effect interface of the render graph (built-in effects, workstream 07's
//! library and plugin effects from 19 all implement [`Effect`]).
//!
//! Parameter *definitions* live in `bae-model` ([`bae_model::EffectDef`], shared with
//! the UI and agents); this trait adds the GPU implementation. An effect declares
//! how it changes bounds and which input region it needs (ROI negotiation, the
//! equivalent of AE SmartFX pre-render), then encodes GPU work. Effects whose
//! definition says `thread_safe: false` are called one at a time.

use std::fmt;
use std::sync::Arc;

use bae_model::{EffectDef, FrameRate, PrimPath, Time, Value};
use rustc_hash::FxHashMap;

use crate::error::RenderError;
use crate::geom::IRect;
use crate::input::RenderEffect;
use crate::request::RenderPurpose;
use crate::source::{EncodeContext, GpuImage};

/// Values of selected effect controls at one layer time.
#[derive(Clone, Debug)]
pub struct ParameterSample {
    /// Layer time of the sample.
    pub time: Time,
    /// Requested controls authored on the instance.
    pub values: Vec<(u32, Value)>,
}

/// Evaluated parameters of one effect instance plus the frame context an
/// effect needs (the equivalents of AE's `PF_InData`), with typed accessors.
#[derive(Clone, Copy, Debug)]
pub struct EffectParams<'a> {
    /// The instance (prim, effect ID, values).
    pub effect: &'a RenderEffect,
    /// Prior samples of this instance requested by the effect, in layer time.
    pub history: &'a [ParameterSample],
    /// Layer time of the frame.
    pub time: Time,
    /// Time in the comp containing the layer.
    pub comp_time: Time,
    /// Comp-time seconds per layer-time second (AE time stretch).
    pub layer_time_scale: f64,
    /// Resolution scale (0.5 at half resolution): pixel-sized parameters such as a
    /// blur radius must be multiplied by it (AE `downsample_x/y`).
    pub scale: f64,
    /// The layer's content bounds in layer pixels at full resolution, before any
    /// effect (AE "layer bounds", `PF_InData::width/height` + origin).
    pub layer_bounds: IRect,
    /// Comp frame rate.
    pub frame_rate: FrameRate,
    /// Duration of one comp frame (AE `time_step`; not yet rescaled for
    /// time-stretched layers).
    pub time_step: Time,
    /// The working space is linearized for blending (effects that mix colours
    /// should do so in linear light too).
    pub linear: bool,
    /// Preview or final render.
    pub purpose: RenderPurpose,
    /// 32-bpc float project: values outside `[0, 1]` survive between effects.
    /// `false` (8 / 16 bpc, AE's default) means every effect's output is
    /// clamped like AE's integer buffers, and AE options such as Levels' "Off
    /// for 32 bpc Color" clip.
    pub float: bool,
    /// 16-bpc integer project (AE uses 32768 for white).
    pub sixteen_bpc: bool,
    /// Comp shutter angle in degrees (AE default 180).
    pub shutter_angle: f64,
    /// Comp shutter phase in degrees (AE default −90).
    pub shutter_phase: f64,
    /// Stable per effect instance (hash of the effect prim path): seeds random
    /// generators so noise does not change between frames or renders.
    pub seed: u64,
}

impl<'a> EffectParams<'a> {
    /// Parameters of `effect` with default frame context: time 0, full
    /// resolution, empty layer bounds, 30 fps, preview, 8 bpc, 180° / −90°
    /// shutter.
    pub fn new(effect: &'a RenderEffect) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = rustc_hash::FxHasher::default();
        effect.path.hash(&mut h);
        Self {
            effect,
            history: &[],
            time: Time::ZERO,
            comp_time: Time::ZERO,
            layer_time_scale: 1.0,
            scale: 1.0,
            layer_bounds: IRect::EMPTY,
            frame_rate: FrameRate::FPS_30,
            time_step: FrameRate::FPS_30.frame_duration(),
            linear: false,
            purpose: RenderPurpose::Preview,
            float: false,
            sixteen_bpc: false,
            shutter_angle: 180.0,
            shutter_phase: -90.0,
            seed: h.finish(),
        }
    }
}

impl EffectParams<'_> {
    /// Raw value of parameter `id`.
    pub fn value(&self, id: u32) -> Option<&Value> {
        self.effect.param(id)
    }

    /// A slider / angle value.
    pub fn float(&self, id: u32) -> Option<f64> {
        self.value(id).and_then(Value::as_f64)
    }

    /// A checkbox value.
    pub fn bool(&self, id: u32) -> Option<bool> {
        match self.value(id)? {
            Value::Bool(b) => Some(*b),
            Value::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    /// A popup value (1-based, as in AE).
    pub fn popup(&self, id: u32) -> Option<i64> {
        match self.value(id)? {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(f.round() as i64),
            _ => None,
        }
    }

    /// A colour value (straight RGBA).
    pub fn color(&self, id: u32) -> Option<[f64; 4]> {
        match self.value(id)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    /// A 2D point value.
    pub fn point(&self, id: u32) -> Option<[f64; 2]> {
        match self.value(id)? {
            Value::Vec2(p) => Some(*p),
            Value::Vec3([x, y, _]) => Some([*x, *y]),
            _ => None,
        }
    }
}

/// A layer's pixels an effect reads (AE layer parameters; also the effect's
/// own layer before its effect stack). The image is in that layer's own
/// space: its transform, opacity, visibility and track matte do not apply.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerDependency {
    /// The layer (of the comp the effect's layer is in).
    pub layer: PrimPath,
    /// The comp time to sample it at (another time: previous / next frames).
    pub time: Time,
    /// How much of the layer: its source, with its masks, or with masks and
    /// effects (AE's Source / Masks / Effects & Masks).
    pub stage: LayerStage,
}

impl LayerDependency {
    /// The source of `layer` at comp time `time` (AE's default).
    pub fn source(layer: PrimPath, time: Time) -> Self {
        Self {
            layer,
            time,
            stage: LayerStage::Source,
        }
    }

    /// The layer with masks and the effects that precede `effect` in its stack.
    pub fn before_effect(layer: PrimPath, time: Time, effect: PrimPath) -> Self {
        Self {
            layer,
            time,
            stage: LayerStage::BeforeEffect(effect),
        }
    }
}

/// How much of a referenced layer an effect reads (AE's layer popup).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayerStage {
    /// The layer's source.
    #[default]
    Source,
    /// The source with the layer's masks.
    Masks,
    /// The source with masks and effects. A reference back to a layer whose
    /// effects are being resolved (A reads B, B reads A) uses `Masks` and
    /// warns.
    EffectsAndMasks,
    /// The prefix of the layer’s effect stack, stopping at this effect prim.
    BeforeEffect(PrimPath),
}

/// Masks of the effect's own layer an effect reads (AE path parameters, Fill's
/// All Masks): delivered as a layer-space image whose alpha is the coverage.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MaskSelect {
    /// One mask (a `Mask` prim of the layer), with its inversion, feather and
    /// expansion; its mode is ignored.
    One(PrimPath),
    /// Every mask of the layer combined, as they cut the layer.
    All,
}

/// GPU implementation of an effect.
pub trait Effect: Send + Sync {
    /// The parameter definition (same object as in `bae_model::EffectRegistry`).
    fn def(&self) -> &EffectDef;

    /// Whether this effect contributes a render node. Parameter-only controls
    /// are registered so their definitions are available without encoding GPU work.
    fn renders_pixels(&self) -> bool {
        true
    }

    /// Output bounds for input bounds (layer pixels at the request scale).
    /// Default: unchanged. A blur grows them by its radius.
    fn output_bounds(&self, _params: &EffectParams<'_>, input: IRect) -> IRect {
        input
    }

    /// Input region needed to produce `output_roi`. Default: the same region.
    fn input_roi(&self, _params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        output_roi
    }

    /// Layer times at which the effect needs earlier parameter values.
    fn parameter_times(&self, _params: &EffectParams<'_>) -> Vec<Time> {
        Vec::new()
    }

    /// Controls to sample at parameter_times. Empty selects all authored controls.
    fn parameter_ids(&self) -> &'static [u32] {
        &[]
    }

    /// Other layers this instance reads (scheduled before it). Default: none.
    fn layer_dependencies(&self, _params: &EffectParams<'_>) -> Vec<LayerDependency> {
        Vec::new()
    }

    /// Masks of its own layer this instance reads. Default: none.
    fn mask_dependencies(&self, _params: &EffectParams<'_>) -> Vec<MaskSelect> {
        Vec::new()
    }

    /// Encodes the effect: reads `input`, returns an image covering at least
    /// `output_roi` (usually allocated with [`EncodeContext::allocate`]).
    fn render(
        &self,
        ctx: &mut EncodeContext<'_>,
        params: &EffectParams<'_>,
        input: &GpuImage,
        output_roi: IRect,
    ) -> Result<GpuImage, RenderError>;
}

impl fmt::Debug for dyn Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Effect")
            .field("match_name", &self.def().match_name)
            .finish_non_exhaustive()
    }
}

/// The id of effect `match_name` in the app registries
/// (`Registry<bae_model::EffectDef>`, `Registry<dyn Effect>`): registry ids
/// have no whitespace, AE matchNames do (`ADBE Gaussian Blur 2`), so
/// whitespace becomes `%20` and `%` becomes `%25`. Names without either
/// (plugin effects, `<plugin-id>/<effect>`) are their own id.
pub fn registry_id(match_name: &str) -> std::borrow::Cow<'_, str> {
    if !match_name.contains(|c: char| c == '%' || c.is_whitespace()) {
        return std::borrow::Cow::Borrowed(match_name);
    }
    let mut out = String::with_capacity(match_name.len() + 8);
    for c in match_name.chars() {
        match c {
            '%' => out.push_str("%25"),
            c if c.is_whitespace() => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
            c => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Effect implementations by effect ID (match name).
#[derive(Default)]
pub struct EffectLibrary {
    effects: FxHashMap<String, Arc<dyn Effect>>,
    /// Generation of the app registry last synced from ([`Self::sync`]).
    synced: Option<u64>,
}

impl EffectLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) an implementation; returns the previous one.
    pub fn register(&mut self, effect: Arc<dyn Effect>) -> Option<Arc<dyn Effect>> {
        self.effects.insert(effect.def().match_name.clone(), effect)
    }

    /// Removes an implementation.
    pub fn unregister(&mut self, match_name: &str) -> Option<Arc<dyn Effect>> {
        self.effects.remove(match_name)
    }

    /// The implementation of `match_name`.
    pub fn get(&self, match_name: &str) -> Option<&Arc<dyn Effect>> {
        self.effects.get(match_name)
    }

    /// Number of registered effects.
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// True when no effect is registered.
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Makes the library hold exactly the effects of the app's
    /// `Registry<dyn Effect>` (built-in and plugin effects; keyed by their
    /// match names), dropping effects registered by hand. Cheap when the
    /// registry has not changed since the last call; returns whether the
    /// library changed (callers drop cached frames then).
    pub fn sync(&mut self, registry: &bae_app::Registry<dyn Effect>) -> bool {
        let generation = registry.generation();
        if self.synced == Some(generation) {
            return false;
        }
        self.synced = Some(generation);
        self.effects.clear();
        for entry in registry.snapshot().iter() {
            let effect = entry.item().clone();
            self.effects.insert(effect.def().match_name.clone(), effect);
        }
        true
    }
}

impl fmt::Debug for EffectLibrary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.effects.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_have_no_whitespace() {
        assert_eq!(
            registry_id("bae.posterize/posterize"),
            "bae.posterize/posterize"
        );
        assert_eq!(
            registry_id("ADBE Gaussian Blur 2"),
            "ADBE%20Gaussian%20Blur%202"
        );
        assert_eq!(registry_id("a%b c"), "a%25b%20c");
        assert_ne!(registry_id("a b"), registry_id("a%20b"));
    }
}

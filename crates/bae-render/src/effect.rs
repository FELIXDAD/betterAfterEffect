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

use bae_model::{EffectDef, PrimPath, Time, Value};
use rustc_hash::FxHashMap;

use crate::error::RenderError;
use crate::geom::IRect;
use crate::input::RenderEffect;
use crate::source::{EncodeContext, GpuImage};

/// Evaluated parameters of one effect instance, with typed accessors.
#[derive(Clone, Copy, Debug)]
pub struct EffectParams<'a> {
    /// The instance (prim, effect ID, values).
    pub effect: &'a RenderEffect,
    /// Layer time of the frame.
    pub time: Time,
    /// Resolution scale (0.5 at half resolution): pixel-sized parameters such as a
    /// blur radius must be multiplied by it.
    pub scale: f64,
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

/// A dependency of an effect on another layer's pixels (AE layer parameters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerDependency {
    /// The layer.
    pub layer: PrimPath,
    /// The comp time to sample it at.
    pub time: Time,
}

/// GPU implementation of an effect.
pub trait Effect: Send + Sync {
    /// The parameter definition (same object as in `bae_model::EffectRegistry`).
    fn def(&self) -> &EffectDef;

    /// Output bounds for input bounds (layer pixels at the request scale).
    /// Default: unchanged. A blur grows them by its radius.
    fn output_bounds(&self, _params: &EffectParams<'_>, input: IRect) -> IRect {
        input
    }

    /// Input region needed to produce `output_roi`. Default: the same region.
    fn input_roi(&self, _params: &EffectParams<'_>, output_roi: IRect) -> IRect {
        output_roi
    }

    /// Other layers this instance reads (scheduled before it). Default: none.
    fn layer_dependencies(&self, _params: &EffectParams<'_>) -> Vec<LayerDependency> {
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

/// Effect implementations by effect ID (match name).
#[derive(Default)]
pub struct EffectLibrary {
    effects: FxHashMap<String, Arc<dyn Effect>>,
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
}

impl fmt::Debug for EffectLibrary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.effects.keys()).finish()
    }
}

//! Typed parameter access with the definition's defaults as fallback.

use bae_model::{EffectDef, EffectParamDef, Value, effect::PointDefaultSpace};
use bae_render::EffectParams;

/// Parameter values of one evaluation, falling back to the defaults of `def`
/// (an instance may omit parameters that were never changed).
#[derive(Clone, Copy)]
pub(crate) struct Params<'a> {
    pub def: &'a EffectDef,
    pub p: &'a EffectParams<'a>,
}

impl<'a> Params<'a> {
    pub fn new(def: &'a EffectDef, p: &'a EffectParams<'a>) -> Self {
        Self { def, p }
    }

    pub fn value(&self, id: u32) -> Option<&'a Value> {
        self.p
            .value(id)
            .or_else(|| self.def.param(id).and_then(|d| d.default.as_ref()))
    }

    /// Authored points are layer pixels. Relative defaults use the rendered layer rectangle.
    pub fn render_point(
        self,
        param: &EffectParamDef,
        origin: [f64; 2],
        size: [f64; 2],
    ) -> Option<[f64; 2]> {
        let authored = self.p.point(param.id);
        let space = param.flags.point_default_space();
        if let Some(xy) = authored {
            return Some(xy.map(|v| v * self.p.scale));
        }
        let Value::Vec2(xy) = param.default.as_ref()? else {
            return None;
        };
        if space == PointDefaultSpace::Pixels {
            return Some(xy.map(|v| v * self.p.scale));
        }
        let [x, y] = space.layer_pixels(*xy, size);
        Some([origin[0] + x, origin[1] + y])
    }

    /// Slider / angle value.
    pub fn f(&self, id: u32) -> f64 {
        match self.value(id) {
            Some(Value::Float(v)) => *v,
            Some(Value::Int(v)) => *v as f64,
            Some(Value::Bool(b)) => f64::from(u8::from(*b)),
            _ => 0.0,
        }
    }

    /// Checkbox value.
    pub fn b(&self, id: u32) -> bool {
        match self.value(id) {
            Some(Value::Bool(b)) => *b,
            Some(Value::Int(i)) => *i != 0,
            Some(Value::Float(f)) => *f != 0.0,
            _ => false,
        }
    }

    /// Popup value (1-based).
    pub fn popup(&self, id: u32) -> i64 {
        match self.value(id) {
            Some(Value::Int(i)) => *i,
            Some(Value::Float(f)) => f.round() as i64,
            _ => 1,
        }
    }
}

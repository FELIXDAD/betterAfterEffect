//! Per-layer stereo processors, in the effect stack's order.
use bae_anim::{Curve, CurveKind};
use bae_model::effect::param_attr;
use bae_model::{PrimPath, Stage, Time, Value};

pub(super) trait Processor: Send {
    fn apply(&mut self, frame: [f32; 2], values: &[f64], rate: u32) -> [f32; 2];
}
struct Definition {
    name: &'static str,
    defaults: &'static [f64],
    reverse: bool,
    create: fn() -> Box<dyn Processor>,
}
mod delay;
const DEFINITIONS: &[Definition] = &[Definition {
    name: "ADBE Aud Delay",
    defaults: &[500.0, 50.0, 50.0, 75.0, 75.0],
    reverse: false,
    create: || Box::new(delay::Main::default()),
}];

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|b| u8::from(b) as f64))
}
struct Instance {
    def: &'static Definition,
    state: Box<dyn Processor>,
    values: Vec<f64>,
    animated: Vec<(usize, Curve)>,
}
#[derive(Default)]
pub(super) struct Rack {
    effects: Vec<Instance>,
    next: Option<(i64, u32)>,
    pub reverse: bool,
}
impl Rack {
    pub fn read(stage: &Stage, layer: &PrimPath) -> Self {
        let mut rack = Self::default();
        if stage
            .prim(layer)
            .and_then(|v| v.value("effects_enabled"))
            .and_then(Value::as_bool)
            == Some(false)
        {
            return rack;
        }
        for path in stage.children(&layer.child("effects").expect("valid name")) {
            let Some(view) = stage.prim(&path) else {
                continue;
            };
            let Some(def) = DEFINITIONS
                .iter()
                .find(|d| view.value("match_name").and_then(Value::as_str) == Some(d.name))
            else {
                continue;
            };
            if view.value("enabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let mut values = def.defaults.to_vec();
            let mut animated = Vec::new();
            for (index, value) in values.iter_mut().enumerate() {
                let attr = param_attr(index as u32 + 1);
                if let Some(v) = view.value(&attr).and_then(number) {
                    *value = v;
                }
                if let Some(c) = view
                    .attr(&attr)
                    .and_then(|s| Curve::new(&s.keyframes, CurveKind::PerDimension))
                {
                    animated.push((index, c));
                }
            }
            rack.reverse ^= def.reverse;
            rack.effects.push(Instance {
                def,
                state: (def.create)(),
                values,
                animated,
            });
        }
        rack
    }
    pub fn begin(&mut self, start: i64, end: i64, rate: u32) {
        if self.next.is_some_and(|expected| expected != (start, rate)) {
            for i in &mut self.effects {
                i.state = (i.def.create)();
            }
        }
        self.next = Some((end, rate));
    }
    pub fn process(&mut self, mut frame: [f32; 2], time: Time, rate: u32) -> [f32; 2] {
        for i in &mut self.effects {
            for (index, curve) in &i.animated {
                if let Some(value) = number(&curve.sample(time)) {
                    i.values[*index] = value;
                }
            }
            frame = i.state.apply(frame, &i.values, rate);
        }
        frame
    }
}

//! Per-layer stereo processors, in the effect stack's order.
use bae_anim::{Curve, CurveKind};
use bae_model::effect::param_attr;
use bae_model::{PrimPath, Stage, Time, Value};

pub(super) trait Processor: Send {
    fn prepare(&mut self, _rate: u32, _values: &[f64], _animated: u32) {}
    fn update(&mut self, _values: &[f64], _rate: u32) {}
    fn seek(&mut self, _seconds: f64, _rate: u32, _speed: f64, _controls: &[f64]) {}
    /// Clear delay history while retaining producer-owned buffers on a seek.
    fn clear_history(&mut self) -> bool {
        false
    }
    fn apply(&mut self, frame: [f32; 2], values: &[f64], rate: u32) -> [f32; 2];
}
struct Definition {
    name: &'static str,
    defaults: &'static [f64],
    reverse: bool,
    create: fn() -> Box<dyn Processor>,
}
mod delay;
mod distortion;
mod dynamics;
mod eq;
mod modulation;
mod tone;
pub use eq::eq_band_response_db;
mod filter;
mod reverb;
mod reverse;
mod stereo;
mod svf;
const DEFINITIONS: &[Definition] = &[
    Definition {
        name: "ADBE Aud Compressor",
        defaults: &[-16., 3., 15., 6., 440., 1., 0., -1.],
        reverse: false,
        create: || Box::new(dynamics::Compressor::default()),
    },
    Definition {
        name: "ADBE Aud Distortion",
        defaults: &[0., 25., 25., 100., 11., 24., 1., 1., 0., 0., 0., 0.],
        reverse: false,
        create: || Box::new(distortion::Distortion::default()),
    },
    Definition {
        name: "ADBE Aud Gate",
        defaults: &[-60., 10., 40., 220.],
        reverse: false,
        create: || Box::new(dynamics::Gate::default()),
    },
    Definition {
        name: "ADBE Aud Modulator",
        defaults: &[1., 10., 2., 50.],
        reverse: false,
        create: || Box::new(modulation::Modulator::default()),
    },
    Definition {
        name: "ADBE Aud Tone",
        defaults: &[
            1., 440., 493.679993, 554.400024, 587.400024, 659.119995, 20.,
        ],
        reverse: false,
        create: || Box::new(tone::Tone::default()),
    },
    Definition {
        name: "ADBE Aud_Flange",
        defaults: &[3., 1., 0.3, 50., 0., 0., 0., 50., 50.],
        reverse: false,
        create: || Box::new(modulation::Chorus::default()),
    },
    Definition {
        name: "ADBE Aud Stereo Mixer",
        defaults: &[100.0, 100.0, -100.0, 100.0, 0.0],
        reverse: false,
        create: || Box::new(stereo::Main),
    },
    Definition {
        name: "ADBE Aud Reverse",
        defaults: &[0.0],
        reverse: true,
        create: || Box::new(reverse::Main),
    },
    Definition {
        name: "ADBE Aud HiLo",
        defaults: &[1.0, 2000.0, 0.0, 100.0],
        reverse: false,
        create: || Box::new(filter::Main::default()),
    },
    Definition {
        name: "ADBE Param EQ",
        defaults: &[
            0.0, 1.0, 1000.0, 20.0, 50.0, 0.0, 5000.0, 20.0, 50.0, 0.0, 10000.0, 20.0, 50.0,
        ],
        reverse: false,
        create: || Box::new(eq::Main::default()),
    },
    Definition {
        name: "ADBE Aud BT",
        defaults: &[0.0, 0.0],
        reverse: false,
        create: || Box::new(eq::Shelves::default()),
    },
    Definition {
        name: "ADBE Aud Delay",
        defaults: &[500.0, 50.0, 50.0, 75.0, 75.0],
        reverse: false,
        create: || Box::new(delay::Main::default()),
    },
    Definition {
        name: "ADBE Aud Reverb",
        defaults: &[100.0, 75.0, 25.0, 10.0, 90.0, 10.0],
        reverse: false,
        create: || Box::new(reverb::Main::default()),
    },
];

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
impl Instance {
    fn prepare(&mut self, rate: u32) {
        let animated = self
            .animated
            .iter()
            .fold(0, |bits, (id, _)| bits | (1u32 << id));
        self.state.prepare(rate, &self.values, animated);
    }
}
#[derive(Default)]
pub(super) struct Rack {
    effects: Vec<Instance>,
    next: Option<(i64, u32)>,
    pub reverse: bool,
    pub generates: bool,
    prepared_rate: Option<u32>,
    seek_origin: Option<(f64, f64)>,
    until_update: u8,
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
            if def.name == "ADBE Aud Tone" {
                rack.generates = true;
                rack.effects.clear();
                rack.reverse = false;
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
    pub fn prepare(&mut self, rate: u32) {
        for instance in &mut self.effects {
            instance.prepare(rate);
        }
        self.prepared_rate = Some(rate);
    }
    pub fn begin(&mut self, start: i64, end: i64, rate: u32, source_clock: (f64, f64)) {
        self.seek_origin = (self.next != Some((start, rate))).then_some(source_clock);
        if self.seek_origin.is_some() {
            self.until_update = 0;
        }
        if self.next.is_some_and(|expected| expected != (start, rate)) {
            for i in &mut self.effects {
                if !i.state.clear_history() {
                    i.state = (i.def.create)();
                    if self.prepared_rate == Some(rate) {
                        i.prepare(rate);
                    }
                }
            }
        }
        if self.prepared_rate != Some(rate) {
            self.prepare(rate);
        }
        self.next = Some((end, rate));
    }
    pub fn process(
        &mut self,
        mut frame: [f32; 2],
        rate: u32,
        time: impl FnOnce() -> Time,
    ) -> [f32; 2] {
        if self.until_update == 0 {
            let time = time();
            for instance in &mut self.effects {
                for (index, curve) in &instance.animated {
                    if let Some(value) = number(&curve.sample(time)) {
                        instance.values[*index] = value;
                    }
                }
                instance.state.update(&instance.values, rate);
                if let Some(source) = self.seek_origin {
                    let (origin, speed) = if instance.def.name == "ADBE Aud Tone" {
                        source
                    } else {
                        (time.as_seconds(), 1.)
                    };
                    instance.state.seek(origin, rate, speed, &instance.values);
                }
            }
            self.seek_origin = None;
            self.until_update = 32;
        }
        self.until_update -= 1;
        for instance in &mut self.effects {
            frame = instance.state.apply(frame, &instance.values, rate);
        }
        frame
    }
}

#[test]
fn automated_level_ticks_every_32_samples_across_producer_buffers() {
    use bae_model::Keyframe;
    use std::collections::BTreeMap;
    let def = DEFINITIONS
        .iter()
        .find(|d| d.name == "ADBE Aud Tone")
        .unwrap();
    let keys = BTreeMap::from([
        (Time::ZERO, Keyframe::linear(0.)),
        (Time::from_seconds(0.064), Keyframe::linear(100.)),
    ]);
    let mut rack = Rack {
        effects: vec![Instance {
            def,
            state: (def.create)(),
            values: vec![4., 1., 0., 0., 0., 0., 0.],
            animated: vec![(6, Curve::new(&keys, CurveKind::PerDimension).unwrap())],
        }],
        generates: true,
        ..Default::default()
    };
    for (start, end) in [(0, 11), (11, 48), (48, 65)] {
        rack.begin(start, end, 1000, (start as f64 / 1000., 1.));
        for index in start..end {
            let out = rack.process([0.75; 2], 1000, || Time::from_seconds(index as f64 / 1000.));
            let expected = (index / 32) as f32 * 0.5;
            assert!((out[0] - expected).abs() < 0.0001);
            assert_eq!(out[0], out[1]);
        }
    }
}

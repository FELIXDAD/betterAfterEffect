//! Layer > Time commands (AE-ANI-087 … 092) through the registry, checked
//! through bae-anim's evaluation of the pre-comp source time.

use bae_anim::LayerSource;
use bae_model::{AuthorId, CommandRegistry, Interpolation, PrimPath, Stage, Time};
use bae_motion::query;
use serde_json::{Value as Json, json};

const SEC: i64 = Time::FLICKS_PER_SECOND;

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

impl Fx {
    /// An outer comp with a pre-comp layer of a 4 s inner comp.
    fn new() -> (Self, PrimPath, PrimPath) {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        let mut fx = Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
        };
        let outer = fx.run(
            "comp.create",
            json!({"width": 100, "height": 100, "fps": 30, "duration": 10 * SEC}),
        );
        let outer = p(outer["path"].as_str().unwrap());
        let inner = fx.run(
            "comp.create",
            json!({"width": 100, "height": 100, "fps": 30, "duration": 4 * SEC}),
        );
        let inner = p(inner["path"].as_str().unwrap());
        let layer = fx.run(
            "layer.create",
            json!({"comp": outer, "kind": "pre_comp", "source": inner}),
        );
        let layer = p(layer["path"].as_str().unwrap());
        (fx, outer, layer)
    }

    fn run(&mut self, id: &str, params: Json) -> Json {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"))
            .output
    }

    fn try_run(&mut self, id: &str, params: Json) -> Result<Json, String> {
        self.reg
            .execute(&mut self.stage, &self.author, id, params)
            .map(|o| o.output)
            .map_err(|e| e.to_string())
    }

    /// The inner comp time the pre-comp layer shows at comp time `t`.
    fn source_at(&self, comp: &PrimPath, layer: &PrimPath, t: Time) -> Time {
        let e = bae_anim::evaluate_comp(&self.stage, comp, t).unwrap();
        match &e.layer(layer).unwrap().source {
            LayerSource::Comp { time, .. } => *time,
            other => panic!("{other:?}"),
        }
    }
}

fn s(secs: f64) -> Time {
    Time::from_seconds(secs)
}

#[test]
fn enable_time_remapping_is_the_identity() {
    let (mut fx, comp, layer) = Fx::new();
    fx.run(
        "layer.time_remap",
        json!({"layers": [layer], "enabled": true}),
    );
    let spec = query::attr_spec(&fx.stage, &layer, "time_remap").unwrap();
    let keys: Vec<Time> = spec.keyframes.keys().copied().collect();
    assert_eq!(
        keys,
        vec![Time::ZERO, s(4.0)],
        "keys at the source start and end"
    );
    assert_eq!(fx.source_at(&comp, &layer, s(1.5)), s(1.5));
    // Off deletes the keys and returns to layer time.
    fx.run(
        "layer.time_remap",
        json!({"layers": [layer], "enabled": false}),
    );
    assert!(
        query::attr_spec(&fx.stage, &layer, "time_remap").is_none_or(|s| s.keyframes.is_empty())
    );
    assert_eq!(fx.source_at(&comp, &layer, s(2.0)), s(2.0));
    // Solids have no time-based source.
    let solid = fx.run("layer.create", json!({"comp": comp, "kind": "solid"}));
    let solid = p(solid["path"].as_str().unwrap());
    assert!(
        fx.try_run(
            "layer.time_remap",
            json!({"layers": [solid], "enabled": true})
        )
        .is_err()
    );
}

#[test]
fn time_stretch_holds_the_chosen_point() {
    let (mut fx, comp, layer) = Fx::new();
    fx.run(
        "layer.trim",
        json!({"layers": [layer], "in_point": SEC, "out_point": 3 * SEC}),
    );
    // 200 % holding the in point: the in point stays, the out point moves out,
    // half speed.
    fx.run(
        "layer.time_stretch",
        json!({"layers": [layer], "stretch": 200.0, "hold": "in_point"}),
    );
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!((t.in_point, t.out_point), (s(1.0), s(5.0)));
    assert_eq!(fx.source_at(&comp, &layer, s(1.0)), s(1.0));
    assert_eq!(fx.source_at(&comp, &layer, s(3.0)), s(2.0));
    // Back to 100 % holding the out point (5 s keeps showing source 3 s).
    fx.run(
        "layer.time_stretch",
        json!({"layers": [layer], "stretch": 100.0, "hold": "out_point"}),
    );
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!((t.in_point, t.out_point), (s(3.0), s(5.0)));
    assert_eq!(fx.source_at(&comp, &layer, s(4.9)), s(2.9));
    // Holding the current frame at 4 s with a new duration of 4 s (stretch 200 %).
    fx.run(
        "layer.time_stretch",
        json!({"layers": [layer], "new_duration": 4 * SEC, "hold": "current_frame", "time": 4 * SEC}),
    );
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!(t.stretch, 200.0);
    assert_eq!((t.in_point, t.out_point), (s(2.0), s(6.0)));
    assert_eq!(
        fx.source_at(&comp, &layer, s(4.0)),
        s(2.0),
        "still shows what 4 s showed"
    );
    // Needs exactly one of stretch / new_duration, and the time for current_frame.
    assert!(
        fx.try_run("layer.time_stretch", json!({"layers": [layer]}))
            .is_err()
    );
    assert!(
        fx.try_run(
            "layer.time_stretch",
            json!({"layers": [layer], "stretch": 50.0, "hold": "current_frame"})
        )
        .is_err()
    );
}

#[test]
fn time_reverse_keeps_in_and_out() {
    let (mut fx, comp, layer) = Fx::new();
    fx.run(
        "layer.trim",
        json!({"layers": [layer], "in_point": SEC, "out_point": 3 * SEC}),
    );
    fx.run("layer.time_reverse", json!({"layers": [layer]}));
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!(t.stretch, -100.0);
    assert_eq!((t.in_point, t.out_point), (s(1.0), s(3.0)));
    // Frame for frame (AE, #764): the in point shows the last frame (3 s − 1
    // frame), the last frame the first (1 s); never a time past the end.
    let frame = Time(SEC / 30);
    assert_eq!(fx.source_at(&comp, &layer, s(1.0)), s(3.0) - frame);
    assert_eq!(fx.source_at(&comp, &layer, s(3.0) - frame), s(1.0));
    // Reversing again restores the layer.
    fx.run("layer.time_reverse", json!({"layers": [layer]}));
    assert_eq!(
        query::layer_timing(&fx.stage, &layer).start_time,
        Time::ZERO
    );
    assert_eq!(fx.source_at(&comp, &layer, s(2.5)), s(2.5));
}

#[test]
fn freeze_frame_holds_the_current_frame() {
    let (mut fx, comp, layer) = Fx::new();
    fx.run(
        "layer.time_stretch",
        json!({"layers": [layer], "stretch": 50.0}),
    );
    // At comp 1 s the 50 % layer shows source 2 s; freezing holds it everywhere.
    fx.run(
        "layer.freeze_frame",
        json!({"layers": [layer], "time": SEC}),
    );
    let spec = query::attr_spec(&fx.stage, &layer, "time_remap").unwrap();
    assert_eq!(spec.keyframes.len(), 1);
    let key = spec.keyframes.values().next().unwrap();
    assert_eq!(
        (key.interp_in, key.interp_out),
        (Interpolation::Hold, Interpolation::Hold)
    );
    for t in [0.0, 1.0, 1.7] {
        assert_eq!(fx.source_at(&comp, &layer, s(t)), s(2.0), "at {t}");
    }
    // Freezing again where time is already remapped holds the remapped frame.
    fx.run(
        "layer.freeze_frame",
        json!({"layers": [layer], "time": SEC / 2}),
    );
    assert_eq!(fx.source_at(&comp, &layer, s(1.9)), s(2.0));
}

#[test]
fn freeze_on_last_frame_holds_to_the_comp_end() {
    let (mut fx, comp, layer) = Fx::new();
    fx.run("layer.freeze_on_last_frame", json!({"layers": [layer]}));
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!(t.out_point, s(10.0), "extended to the comp end");
    assert_eq!(
        fx.source_at(&comp, &layer, s(1.0)),
        s(1.0),
        "plays normally"
    );
    // AE 26.2.1: the hold key sits one comp frame before the source end.
    let last = s(4.0) - bae_model::FrameRate::FPS_30.frame_duration();
    assert_eq!(
        fx.source_at(&comp, &layer, s(8.0)),
        last,
        "then holds the last frame"
    );
}

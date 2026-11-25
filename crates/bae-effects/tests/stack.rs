//! The effect stack in the app: effects applied with the motion commands
//! (`effect.*`), evaluated by bae-anim and rendered by bae-render with the
//! built-in effects from `EffectsModule` — stack order, the layer's fx switch,
//! an effect's own switch, undo, adjustment layers, saving and reopening.

use std::sync::Arc;

use bae_app::AppBuilder;
use bae_model::{AuthorId, CommandRegistry, EffectDef, PrimPath, Project, Stage, Time};
use bae_render::{AnimEvaluator, CompEvaluator, Effect, OutputAlpha, RenderRequest, Renderer};
use serde_json::{Value as Json, json};

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
    renderer: Option<Renderer>,
    eval: AnimEvaluator,
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

impl Fx {
    fn new() -> Self {
        let mut app = AppBuilder::new();
        app.add_module(bae_effects::EffectsModule);
        let app = app.build();
        let defs = app.get_registry::<EffectDef>().expect("defs");
        let effects: Arc<bae_app::Registry<dyn Effect>> =
            app.get_registry::<dyn Effect>().expect("effects");
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        bae_motion::effect::register_effect_commands(&mut reg, defs);
        let renderer = bae_gpu::testing::shared_context().map(|gpu| {
            let mut r = Renderer::new(gpu);
            r.sync_effects(&effects);
            r
        });
        Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
            renderer,
            eval: AnimEvaluator::new(None),
        }
    }

    fn run(&mut self, id: &str, params: Json) -> Json {
        let out = self
            .reg
            .execute(&mut self.stage, &self.author, id, params)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
        self.invalidate(&out.event);
        out.output
    }

    /// Drops the evaluator's and the renderer's caches for `event` (as the
    /// app does after every command).
    fn invalidate(&mut self, event: &bae_model::StageEvent) {
        self.eval.invalidate(event);
        if let Some(r) = self.renderer.as_mut() {
            r.invalidate(event);
        }
    }

    fn undo(&mut self) {
        let event = self.stage.undo(&self.author).expect("undo");
        self.invalidate(&event);
    }

    fn comp(&mut self) -> PrimPath {
        let out = self.run("comp.create", json!({"width": 64, "height": 32, "fps": 30}));
        p(out["path"].as_str().unwrap())
    }

    fn solid(&mut self, comp: &PrimPath, color: [f64; 3], size: [u32; 2]) -> PrimPath {
        let out = self.run(
            "layer.create",
            json!({"comp": comp, "kind": "solid", "color": [color[0], color[1], color[2], 1.0], "size": size}),
        );
        p(out["path"].as_str().unwrap())
    }

    fn adjustment(&mut self, comp: &PrimPath, size: Option<[u32; 2]>) -> PrimPath {
        let mut params = json!({"comp": comp, "kind": "adjustment"});
        if let Some(size) = size {
            params["size"] = json!(size);
        }
        let out = self.run("layer.create", params);
        p(out["path"].as_str().unwrap())
    }

    fn apply(&mut self, layer: &PrimPath, match_name: &str) -> PrimPath {
        let out = self.run(
            "effect.apply",
            json!({"layers": [layer], "match_name": match_name}),
        );
        p(out["effects"][0].as_str().unwrap())
    }

    fn param(&mut self, effect: &PrimPath, id: u32, value: Json) {
        self.run(
            "attr.set",
            json!({"path": effect, "attr": format!("param:{id:04}"), "value": value}),
        );
    }

    /// Straight RGBA8 of pixel (x, y) at time 0; `None` without a GPU.
    fn pixel(&mut self, comp: &PrimPath, x: u32, y: u32) -> Option<[u8; 4]> {
        self.pixel_at(comp, x, y, Time::ZERO)
    }

    /// Straight RGBA8 of pixel (x, y) at `time`; `None` without a GPU.
    fn pixel_at(&mut self, comp: &PrimPath, x: u32, y: u32, time: Time) -> Option<[u8; 4]> {
        let renderer = self.renderer.as_mut()?;
        let rc = self.eval.evaluate(&self.stage, comp, time).unwrap();
        let frame = renderer.render_frame(&rc, &RenderRequest::full()).unwrap();
        let data = renderer.read_rgba8(&frame, OutputAlpha::Straight).unwrap();
        let px = data.pixel(x, y);
        Some([px[0], px[1], px[2], px[3]])
    }
}

fn close(got: [u8; 4], want: [f64; 4], tol: u8, what: &str) {
    let want = want.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let ok = (0..4).all(|k| got[k].abs_diff(want[k]) <= tol);
    assert!(ok, "{what}: got {got:?}, want {want:?}");
}

const LEVELS: &str = "ADBE Easy Levels2";
const EXPOSURE: &str = "ADBE Exposure2";

/// Levels with output black 0.2 (y = 0.2 + 0.8 x) and Exposure +1 without
/// linearization (y = 2 x): the order shows in the result.
fn levels_then_exposure(fx: &mut Fx, layer: &PrimPath) -> (PrimPath, PrimPath) {
    let levels = fx.apply(layer, LEVELS);
    fx.param(&levels, 6, json!({"type": "float", "value": 0.2}));
    let exposure = fx.apply(layer, EXPOSURE);
    fx.param(&exposure, 3, json!({"type": "float", "value": 1.0}));
    fx.param(&exposure, 22, json!({"type": "bool", "value": true}));
    (levels, exposure)
}

#[test]
fn distort_point_defaults_use_the_source_on_apply_and_reset() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.5; 3], [31, 13]);
    for (effect_name, id, expected) in [
        ("ADBE Mirror", 1, [31., 6.5]),
        ("ADBE Corner Pin", 4, [31., 13.]),
        ("ADBE Geometry2", 2, [15.5, 6.5]),
    ] {
        let effect = fx.apply(&layer, effect_name);
        let prop = format!("param:{id:04}");
        let point = |fx: &Fx| fx.stage.prim(&effect).unwrap().value(&prop).cloned();
        assert_eq!(point(&fx), Some(bae_model::Value::Vec2(expected)));
        fx.param(&effect, id, json!({"type": "vec2", "value": [4., 5.]}));
        fx.run("effect.reset", json!({"effects": [effect]}));
        assert_eq!(point(&fx), Some(bae_model::Value::Vec2(expected)));
    }
}

#[test]
fn effects_render_top_to_bottom() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.2, 0.3, 0.1], [64, 32]);
    let (levels, _exposure) = levels_then_exposure(&mut fx, &layer);
    let l = |x: f64| 0.2 + 0.8 * x;
    let e = |x: f64| 2.0 * x;
    let Some(px) = fx.pixel(&comp, 10, 10) else {
        return;
    };
    close(
        px,
        [e(l(0.2)), e(l(0.3)), e(l(0.1)), 1.0],
        2,
        "levels, then exposure",
    );
    // Drag Levels below Exposure.
    fx.run("effect.reorder", json!({"effects": [levels], "index": 1}));
    let px = fx.pixel(&comp, 10, 10).unwrap();
    close(
        px,
        [l(e(0.2)), l(e(0.3)), l(e(0.1)), 1.0],
        2,
        "exposure, then levels",
    );
    // One undo step puts the order back.
    fx.undo();
    let px = fx.pixel(&comp, 10, 10).unwrap();
    close(px, [e(l(0.2)), e(l(0.3)), e(l(0.1)), 1.0], 2, "after undo");
}

/// Effect parameters animate through bae-anim: keyframes and expressions.
#[test]
fn parameters_follow_keyframes_and_expressions() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.0, 0.0, 0.0], [64, 32]);
    let levels = fx.apply(&layer, LEVELS);
    // Output Black 0 at 0 s → 0.4 at 1 s (linear): black becomes 0.2 at 0.5 s.
    for (t, v) in [(0, 0.0), (1, 0.4)] {
        fx.run(
            "key.set",
            json!({
                "path": levels,
                "attr": "param:0006",
                "time": Time::from_seconds(f64::from(t)).flicks(),
                "keyframe": bae_model::Keyframe::linear(bae_model::Value::Float(v)),
            }),
        );
    }
    let Some(px) = fx.pixel_at(&comp, 5, 5, Time::from_seconds(0.5)) else {
        return;
    };
    close(
        px,
        [0.2, 0.2, 0.2, 1.0],
        1,
        "keyframed output black at 0.5 s",
    );
    let px = fx.pixel_at(&comp, 5, 5, Time::from_seconds(1.0)).unwrap();
    close(px, [0.4, 0.4, 0.4, 1.0], 1, "keyframed output black at 1 s");
    // An expression wins over the keyframes.
    fx.run(
        "attr.set_expression",
        json!({"path": levels, "attr": "param:0006", "source": "0.1 + time * 0.2"}),
    );
    let px = fx.pixel_at(&comp, 5, 5, Time::from_seconds(1.0)).unwrap();
    close(px, [0.3, 0.3, 0.3, 1.0], 1, "expression at 1 s");
}

#[test]
fn switches_turn_effects_off() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.2, 0.3, 0.1], [64, 32]);
    let (levels, exposure) = levels_then_exposure(&mut fx, &layer);
    let original = [0.2, 0.3, 0.1, 1.0];
    // The layer's fx switch: no effects at all.
    fx.run(
        "layer.set_attr",
        json!({"layers": [layer], "attr": "effects_enabled", "value": {"type": "bool", "value": false}}),
    );
    let Some(px) = fx.pixel(&comp, 3, 3) else {
        return;
    };
    close(px, original, 1, "fx switch off");
    fx.run(
        "layer.set_attr",
        json!({"layers": [layer], "attr": "effects_enabled", "value": {"type": "bool", "value": true}}),
    );
    // One effect off: only Levels renders.
    fx.run(
        "effect.set_enabled",
        json!({"effects": [exposure], "enabled": false}),
    );
    let px = fx.pixel(&comp, 3, 3).unwrap();
    close(px, [0.36, 0.44, 0.28, 1.0], 2, "exposure off");
    // Removing Levels leaves nothing enabled.
    fx.run("effect.remove", json!({"effects": [levels]}));
    let px = fx.pixel(&comp, 3, 3).unwrap();
    close(px, original, 1, "levels removed, exposure off");
}

#[test]
fn apply_writes_defaults_and_names_like_ae() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.5, 0.5, 0.5], [64, 32]);
    let a = fx.apply(&layer, "ADBE Gaussian Blur 2");
    let b = fx.apply(&layer, "ADBE Gaussian Blur 2");
    let name = |fx: &Fx, e: &PrimPath| bae_motion::query::display_name_of(&fx.stage, e);
    assert_eq!(name(&fx, &a), "Gaussian Blur");
    assert_eq!(name(&fx, &b), "Gaussian Blur 2");
    let v = fx.stage.prim(&a).unwrap();
    // AE 26 defaults: Blurriness 25, Horizontal and Vertical, Repeat Edge Pixels.
    assert_eq!(
        v.attr("param:0001").and_then(|s| s.value.clone()),
        Some(bae_model::Value::Float(25.0))
    );
    assert!(v.attr("param:0003").is_some());
    assert_eq!(
        bae_motion::effect::effects_of(&fx.stage, &layer),
        vec![a.clone(), b.clone()]
    );
    // Duplicate goes right below its original, Reset restores defaults.
    fx.param(&a, 1, json!({"type": "float", "value": 3.0}));
    let out = fx.run("effect.duplicate", json!({"effects": [a]}));
    let c = p(out["effects"][0].as_str().unwrap());
    assert_eq!(
        bae_motion::effect::effects_of(&fx.stage, &layer),
        vec![a.clone(), c.clone(), b.clone()]
    );
    assert_eq!(name(&fx, &c), "Gaussian Blur 3");
    fx.run("effect.reset", json!({"effects": [a]}));
    assert_eq!(
        fx.stage
            .prim(&a)
            .unwrap()
            .attr("param:0001")
            .and_then(|s| s.value.clone()),
        Some(bae_model::Value::Float(25.0))
    );
    fx.run("effect.remove_all", json!({"layers": [layer]}));
    assert!(bae_motion::effect::effects_of(&fx.stage, &layer).is_empty());
    let err = fx
        .reg
        .execute(
            &mut fx.stage,
            &fx.author,
            "effect.apply",
            json!({"layers": [layer], "match_name": "ADBE Nope"}),
        )
        .unwrap_err();
    assert!(err.to_string().contains("unknown effect"), "{err}");
}

#[test]
fn adjustment_layer_changes_layers_below_only() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    fx.solid(&comp, [1.0, 0.0, 0.0], [64, 32]);
    let adj = fx.adjustment(&comp, None);
    // Without effects an adjustment layer changes nothing.
    let Some(px) = fx.pixel(&comp, 40, 16) else {
        return;
    };
    close(px, [1.0, 0.0, 0.0, 1.0], 1, "adjustment without effects");
    // Tint (black → white): luminance grey, Rec.601.
    fx.apply(&adj, "ADBE Tint");
    let grey = 0.299;
    let px = fx.pixel(&comp, 40, 16).unwrap();
    close(px, [grey, grey, grey, 1.0], 1, "tinted below");
    // A layer above is not adjusted.
    fx.solid(&comp, [0.0, 0.0, 1.0], [8, 8]);
    let px = fx.pixel(&comp, 32, 16).unwrap();
    close(px, [0.0, 0.0, 1.0, 1.0], 1, "layer above");
    let px = fx.pixel(&comp, 4, 4).unwrap();
    close(px, [grey, grey, grey, 1.0], 1, "still tinted beside it");
}

#[test]
fn adjustment_layer_coverage_and_opacity() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    fx.solid(&comp, [1.0, 0.0, 0.0], [64, 32]);
    // 32 × 32 in the middle of the 64 × 32 comp: x in [16, 48).
    let adj = fx.adjustment(&comp, Some([32, 32]));
    fx.apply(&adj, "ADBE Tint");
    let grey = 0.299;
    let Some(px) = fx.pixel(&comp, 4, 16) else {
        return;
    };
    close(px, [1.0, 0.0, 0.0, 1.0], 1, "outside the adjustment layer");
    let px = fx.pixel(&comp, 32, 16).unwrap();
    close(px, [grey, grey, grey, 1.0], 1, "inside");
    // 50 % opacity: half way between the layers below and the adjusted result.
    fx.run(
        "attr.set",
        json!({"path": adj, "attr": "transform:opacity", "value": {"type": "float", "value": 50.0}}),
    );
    let px = fx.pixel(&comp, 32, 16).unwrap();
    close(
        px,
        [(1.0 + grey) / 2.0, grey / 2.0, grey / 2.0, 1.0],
        2,
        "50 % opacity",
    );
    // The fx switch of the adjustment layer turns it off.
    fx.run(
        "layer.set_attr",
        json!({"layers": [adj], "attr": "effects_enabled", "value": {"type": "bool", "value": false}}),
    );
    let px = fx.pixel(&comp, 32, 16).unwrap();
    close(px, [1.0, 0.0, 0.0, 1.0], 1, "adjustment fx off");
}

/// A blur on an adjustment layer over one untransformed layer equals the same
/// blur on the layer itself: the adjusted result replaces what is below (no
/// sharp copy shows through where the blur made pixels transparent).
#[test]
fn adjustment_blur_matches_layer_blur() {
    let blur = |on_adjustment: bool| -> Option<Vec<[u8; 4]>> {
        let mut fx = Fx::new();
        let comp = fx.comp();
        let layer = fx.solid(&comp, [1.0, 1.0, 1.0], [16, 16]);
        let target = if on_adjustment {
            fx.adjustment(&comp, None)
        } else {
            layer
        };
        let e = fx.apply(&target, "ADBE Gaussian Blur 2");
        fx.param(&e, 1, json!({"type": "float", "value": 12.0}));
        fx.param(&e, 3, json!({"type": "bool", "value": false}));
        (0..64)
            .map(|x| fx.pixel(&comp, x, 16))
            .collect::<Option<Vec<_>>>()
    };
    let (Some(a), Some(b)) = (blur(true), blur(false)) else {
        return;
    };
    for (x, (pa, pb)) in a.iter().zip(&b).enumerate() {
        assert!(
            (0..4).all(|k| pa[k].abs_diff(pb[k]) <= 1),
            "x {x}: adjustment {pa:?} vs layer {pb:?}"
        );
    }
    // The blur reaches outside the 16 px square and is soft inside its edge.
    assert!(a[22][3] > 0 && a[22][3] < 255, "{:?}", a[22]);
    assert!(a[24][3] < 255, "{:?}", a[24]);
}

#[test]
fn effects_and_fx_switch_survive_save_and_reopen() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.2, 0.3, 0.1], [64, 32]);
    let blur = fx.apply(&layer, "ADBE Gaussian Blur 2");
    fx.param(&blur, 1, json!({"type": "float", "value": 7.5}));
    fx.run(
        "layer.set_attr",
        json!({"layers": [layer], "attr": "effects_enabled", "value": {"type": "bool", "value": false}}),
    );
    let other = fx.solid(&comp, [0.5, 0.5, 0.5], [8, 8]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fx.bae");
    let mut project = Project::from_stage(std::mem::take(&mut fx.stage));
    project.save_as(&path).unwrap();
    let reopened = Project::open(&path).unwrap();
    let stage = &reopened.stage;
    let enabled = |layer: &PrimPath| {
        stage
            .prim(layer)
            .and_then(|v| v.attr("effects_enabled").and_then(|a| a.value.clone()))
    };
    assert_eq!(enabled(&layer), Some(bae_model::Value::Bool(false)));
    // Layers without the attribute (older projects) read the default: on.
    assert_eq!(enabled(&other), None);
    let mut eval = AnimEvaluator::new(None);
    let rc = eval.evaluate(stage, &comp, Time::ZERO).unwrap();
    let find = |l: &PrimPath| rc.layers.iter().find(|x| &x.path == l).unwrap();
    assert!(find(&layer).effects.is_empty(), "fx switch off");
    let effects = bae_motion::effect::effects_of(stage, &layer);
    assert_eq!(effects, vec![blur.clone()]);
    assert_eq!(
        stage
            .prim(&blur)
            .unwrap()
            .attr("param:0001")
            .and_then(|a| a.value.clone()),
        Some(bae_model::Value::Float(7.5))
    );
    let ec = bae_anim::evaluate_comp(stage, &comp, Time::ZERO).unwrap();
    let l = ec.layers.iter().find(|l| l.path == other).unwrap();
    assert!(l.switches.effects, "missing attribute reads as on");
}

/// Every built-in effect: apply it, change a parameter, keyframe it, render,
/// and undo back to no effects (one undo step per command).
#[test]
fn every_builtin_applies_animates_renders_and_undoes() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.6, 0.4, 0.2], [64, 32]);
    for b in bae_effects::builtins() {
        let def = b.def();
        let e = fx.apply(&layer, &def.match_name);
        let mut steps = 1;
        // The first numeric parameter: a value, then two keyframes.
        if let Some(p) = def
            .params
            .iter()
            .find(|p| matches!(p.kind, bae_model::effect::ParamKind::Slider { .. }))
        {
            let attr = format!("param:{:04}", p.id);
            let v = |x: f64| json!({"type": "float", "value": x});
            fx.run(
                "attr.set",
                json!({"path": e, "attr": attr, "value": v(1.0)}),
            );
            for (t, x) in [(0.0, 0.5), (1.0, 2.0)] {
                fx.run(
                    "key.set",
                    json!({
                        "path": e,
                        "attr": attr,
                        "time": Time::from_seconds(t).flicks(),
                        "keyframe": bae_model::Keyframe::linear(bae_model::Value::Float(x)),
                    }),
                );
            }
            steps += 3;
        }
        // Renders (some effects change alpha, so only that it works).
        let _ = fx.pixel_at(&comp, 10, 10, Time::from_seconds(0.5));
        for _ in 0..steps {
            fx.undo();
        }
        assert!(
            bae_motion::effect::effects_of(&fx.stage, &layer).is_empty(),
            "{}: undo leaves effects",
            def.match_name
        );
    }
}

#[test]
fn spatial_point_defaults_follow_source_dimensions_on_apply_and_reset() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.solid(&comp, [0.5; 3], [31, 13]);
    for (name, id, expected) in [
        ("ADBE Radial Blur", 2, [15.5, 6.5]),
        ("CC Radial Blur", 4, [15.5, 6.5]),
        ("CC Radial Fast Blur", 1, [15.5, 6.5]),
        ("ADBE Tile", 1, [15.5, 6.5]),
        ("ADBE Radial Shadow", 3, [7.75, 3.25]),
    ] {
        let effect = fx.apply(&layer, name);
        let attr = format!("param:{id:04}");
        let point = |fx: &Fx| fx.stage.prim(&effect).unwrap().value(&attr).cloned();
        let expected = Some(bae_model::Value::Vec2(expected));
        assert_eq!(point(&fx), expected);
        fx.param(&effect, id, json!({"type":"vec2","value":[4.0,5.0]}));
        fx.run("effect.reset", json!({"effects":[effect]}));
        assert_eq!(point(&fx), expected);
    }
}

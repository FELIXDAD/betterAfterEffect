//! Motion commands through the command registry, as the UI and agents run them.

use bae_anim::glam::DVec3;
use bae_model::{
    AuthorId, CommandRegistry, DynCommand, FrameRate, Interpolation, PrimPath, Stage, Time, Value,
};
use bae_motion::query;
use serde_json::{Value as Json, json};

struct Fx {
    stage: Stage,
    reg: CommandRegistry,
    author: AuthorId,
}

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

const SEC: i64 = Time::FLICKS_PER_SECOND;

#[test]
fn precomp_creation_rejects_direct_and_transitive_cycles() {
    let mut f = Fx::new();
    let (a, b, c) = (f.comp(), f.comp(), f.comp());
    f.layer(&a, "pre_comp", json!({"source": b}));
    f.layer(&b, "pre_comp", json!({"source": c}));
    f.layer(&a, "pre_comp", json!({"source": c}));
    for source in [&c, &a] {
        assert!(
            f.try_run(
                "layer.create",
                json!({"comp": c, "kind": "pre_comp", "source": source})
            )
            .is_err()
        );
        assert!(query::comp_layers(&f.stage, &c).is_empty());
    }
}

impl Fx {
    fn new() -> Self {
        let mut reg = CommandRegistry::with_builtins();
        bae_motion::register_commands(&mut reg);
        Fx {
            stage: Stage::new(),
            reg,
            author: AuthorId::new("user:test").unwrap(),
        }
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

    fn comp(&mut self) -> PrimPath {
        let out = self.run(
            "comp.create",
            json!({"width": 1920, "height": 1080, "fps": 30}),
        );
        p(out["path"].as_str().unwrap())
    }

    fn layer(&mut self, comp: &PrimPath, kind: &str, extra: Json) -> PrimPath {
        let mut params = json!({"comp": comp, "kind": kind});
        if let Json::Object(m) = extra {
            params.as_object_mut().unwrap().extend(m);
        }
        let out = self.run("layer.create", params);
        p(out["path"].as_str().unwrap())
    }

    fn value(&self, path: &PrimPath, attr: &str) -> Option<Value> {
        query::static_value(&self.stage.prim(path).unwrap(), attr)
    }

    fn keys(&self, path: &PrimPath, attr: &str) -> Vec<i64> {
        query::attr_spec(&self.stage, path, attr)
            .map(|s| s.keyframes.keys().map(|t| t.0).collect())
            .unwrap_or_default()
    }

    fn world_pos(&self, comp: &PrimPath, layer: &PrimPath) -> DVec3 {
        let e = bae_anim::evaluate_comp(&self.stage, comp, Time::ZERO).unwrap();
        e.layer(layer)
            .unwrap()
            .world_matrix
            .transform_point3(DVec3::ZERO)
    }
}

#[test]
fn source_styles_convert_with_keyframes_and_remove_per_instance() {
    use bae_model::layer_styles::{self as styles, effective_styles};
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let asset = fx
        .value(&a, "source")
        .unwrap()
        .as_prim_ref()
        .unwrap()
        .clone();
    let b = fx.layer(&comp, "solid", json!({"source":asset}));
    let source = asset.child(styles::PRIM).unwrap();
    fx.run("prim.create", json!({"path":source,"prim_type":"Effect"}));
    let attr = bae_model::effect::param_attr(styles::group("solidFill").unwrap().enabled);
    fx.run(
        "attr.set",
        json!({"path":source,"attr":"match_name","value":Value::String(styles::RENDER_ID.into())}),
    );
    fx.run(
        "attr.set",
        json!({"path":source,"attr":attr,"value":Value::Bool(true)}),
    );
    let opacity = "param:0703";
    fx.run(
        "attr.set",
        json!({"path":source,"attr":opacity,"value":Value::Float(10.)}),
    );
    fx.run(
        "attr.set_animated",
        json!({"path":source,"attr":opacity,"animated":true,"time":0}),
    );
    fx.run(
        "attr.set_at",
        json!({"path":source,"attr":opacity,"time":SEC,"value":Value::Float(90.)}),
    );
    fx.run("layer.styles", json!({"layers":[a],"operation":"convert"}));
    let local = a.child(styles::PRIM).unwrap();
    assert_eq!(effective_styles(&fx.stage, &a), Some(local.clone()));
    assert_eq!(effective_styles(&fx.stage, &b), Some(source.clone()));
    assert_eq!(fx.keys(&local, opacity), vec![0, SEC]);
    let sample = bae_anim::evaluate_comp(&fx.stage, &comp, Time::from_flicks(SEC)).unwrap();
    for layer in &sample.layers {
        assert_eq!(
            layer.layer_styles.as_ref().unwrap().param(703),
            Some(&Value::Float(90.))
        );
    }
    fx.run(
        "layer.styles",
        json!({"layers":[a],"operation":"remove_all"}),
    );
    assert_eq!(effective_styles(&fx.stage, &a), None);
    assert_eq!(effective_styles(&fx.stage, &b), Some(source));
}

#[test]
fn showing_all_styles_preserves_values_and_leaves_new_styles_disabled() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let local = a.child(bae_model::layer_styles::PRIM).unwrap();
    fx.run(
        "layer.styles",
        json!({"layers":[a],"operation":"add","style":"dropShadow"}),
    );
    fx.run(
        "attr.set",
        json!({"path":local,"attr":"param:0103","value":Value::Float(27.)}),
    );
    fx.run("layer.styles", json!({"layers":[a],"operation":"show_all"}));
    assert_eq!(fx.value(&local, "param:0103"), Some(Value::Float(27.)));
    assert_eq!(fx.value(&local, "param:0900"), Some(Value::Bool(false)));
    fx.run(
        "layer.styles",
        json!({"layers":[a],"operation":"add","style":"frameFX"}),
    );
    assert_eq!(fx.value(&local, "param:0900"), Some(Value::Bool(true)));
}

fn key(path: &PrimPath, attr: &str, t: i64) -> Json {
    json!({"path": path, "attr": attr, "time": t})
}

#[test]
fn create_comp_defaults_and_names() {
    let mut fx = Fx::new();
    let a = fx.comp();
    assert_eq!(a, p("/comps/Comp_1"));
    let info = query::comp_info(&fx.stage, &a).unwrap();
    assert_eq!(info.name, "Comp 1");
    assert_eq!((info.width, info.height), (1920, 1080));
    assert_eq!(info.frame_rate, FrameRate::FPS_30);
    assert_eq!(info.duration, Time::from_seconds(10.0));
    assert_eq!(info.work_area.end, info.duration);
    assert!(fx.stage.has_prim(&p("/comps/Comp_1/layers")));

    let out = fx.run(
        "comp.create",
        json!({"fps": 29.97, "name": "主合成", "duration": 5 * SEC}),
    );
    let b = p(out["path"].as_str().unwrap());
    let info = query::comp_info(&fx.stage, &b).unwrap();
    assert_eq!(info.frame_rate, FrameRate::FPS_29_97);
    assert_eq!(info.name, "主合成");
    assert_eq!(fx.comp(), p("/comps/Comp_2"));
    // The whole comp is one undo step.
    assert!(fx.stage.undo(&fx.author).is_some());
    assert!(!fx.stage.has_prim(&p("/comps/Comp_2")));
    assert!(fx.try_run("comp.create", json!({"width": 0})).is_err());
}

#[test]
fn layers_are_added_on_top_with_ae_defaults() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({"color": [1.0, 0.0, 0.0, 1.0]}));
    let shape = fx.layer(&comp, "shape", json!({"shape": "rect"}));
    let text = fx.layer(&comp, "text", json!({"text": "Hello"}));
    let null = fx.layer(&comp, "null", json!({}));
    assert_eq!(
        query::comp_layers(&fx.stage, &comp),
        vec![null.clone(), text.clone(), shape.clone(), solid.clone()]
    );
    assert_eq!(query::display_name_of(&fx.stage, &shape), "Shape Layer 1");
    assert_eq!(query::display_name_of(&fx.stage, &text), "Hello");
    assert_eq!(query::display_name_of(&fx.stage, &solid), "Solid 1");
    // Solid: an asset in /assets/solids, anchored at its center, centered in the comp.
    let src = fx.value(&solid, "source").unwrap();
    let asset = src.as_prim_ref().unwrap().clone();
    assert!(asset.as_str().starts_with("/assets/solids/"));
    assert_eq!(
        fx.value(&solid, "transform:anchor_point"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
    assert_eq!(
        fx.value(&shape, "transform:position"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
    // Shape preset: contents > group > contents > rect + fill.
    assert!(fx.stage.has_prim(&p(&format!(
        "{}/contents/Rectangle_1/contents/Rectangle_Path_1",
        shape
    ))));
    // The animation evaluator sees all of it.
    let e = bae_anim::evaluate_comp(&fx.stage, &comp, Time::ZERO).unwrap();
    assert_eq!(e.layers.len(), 4);
    assert!(!e.layer(&shape).unwrap().contents.is_empty());
    assert_eq!(
        e.layer(&solid)
            .unwrap()
            .world_matrix
            .transform_point3(DVec3::ZERO),
        DVec3::new(0.0, 0.0, 0.0)
    );
    // Second shape layer is numbered.
    let shape2 = fx.layer(&comp, "shape", json!({"index": 4}));
    assert_eq!(query::display_name_of(&fx.stage, &shape2), "Shape Layer 2");
    assert_eq!(query::comp_layers(&fx.stage, &comp).last(), Some(&shape2));
    // Pre-comps need another comp as source.
    assert!(
        fx.try_run("layer.create", json!({"comp": comp, "kind": "pre_comp"}))
            .is_err()
    );
    let inner = fx.comp();
    let pre = fx.layer(&comp, "pre_comp", json!({"source": inner}));
    assert_eq!(query::display_name_of(&fx.stage, &pre), "Comp 2");
    assert_eq!(
        query::layer_timing(&fx.stage, &pre).out_point,
        Time(10 * SEC)
    );
}

#[test]
fn reorder_duplicate_split_trim() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({"name": "A"}));
    let b = fx.layer(&comp, "solid", json!({"name": "B"}));
    let c = fx.layer(&comp, "solid", json!({"name": "C"}));
    assert_eq!(
        query::comp_layers(&fx.stage, &comp),
        vec![c.clone(), b.clone(), a.clone()]
    );
    fx.run("layer.reorder", json!({"layers": [c], "index": 99}));
    assert_eq!(
        query::comp_layers(&fx.stage, &comp),
        vec![b.clone(), a.clone(), c.clone()]
    );
    fx.run("layer.reorder", json!({"layers": [a, c], "index": 0}));
    assert_eq!(
        query::comp_layers(&fx.stage, &comp),
        vec![a.clone(), c.clone(), b.clone()]
    );

    // Duplicate goes right above the original, with its keyframes.
    fx.run("key.set", json!({"path": b, "attr": "transform:opacity", "time": 0, "keyframe": {"value": {"type": "float", "value": 0.0}}}));
    let out = fx.run("layer.duplicate", json!({"layers": [b]}));
    let b2 = p(out["paths"][0].as_str().unwrap());
    assert_eq!(query::display_name_of(&fx.stage, &b2), "B 2");
    assert_eq!(
        query::comp_layers(&fx.stage, &comp),
        vec![a.clone(), c.clone(), b2.clone(), b.clone()]
    );
    assert_eq!(fx.keys(&b2, "transform:opacity"), vec![0]);

    // Split at 4 s.
    let out = fx.run("layer.split", json!({"layers": [a], "time": 4 * SEC}));
    let a2 = p(out["paths"][0].as_str().unwrap());
    assert_eq!(query::layer_timing(&fx.stage, &a).out_point, Time(4 * SEC));
    assert_eq!(query::layer_timing(&fx.stage, &a2).in_point, Time(4 * SEC));
    assert_eq!(query::comp_layers(&fx.stage, &comp)[0], a2);
    assert!(
        fx.try_run("layer.split", json!({"layers": [a], "time": 6 * SEC}))
            .is_err()
    );

    // Trim.
    fx.run(
        "layer.trim",
        json!({"layers": [c], "in_point": SEC, "out_point": 3 * SEC}),
    );
    let t = query::layer_timing(&fx.stage, &c);
    assert_eq!((t.in_point, t.out_point), (Time(SEC), Time(3 * SEC)));
    assert!(
        fx.try_run("layer.trim", json!({"layers": [c], "in_point": 5 * SEC}))
            .is_err()
    );
}

#[test]
fn parenting_keeps_the_layer_in_place() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let child = fx.layer(&comp, "null", json!({"position": [300.0, 200.0]}));
    let parent = fx.layer(&comp, "null", json!({"position": [1000.0, 500.0]}));
    fx.run("attr.set", json!({"path": parent, "attr": "transform:rotation", "value": {"type": "float", "value": 90.0}}));
    fx.run("attr.set", json!({"path": parent, "attr": "transform:scale", "value": {"type": "vec3", "value": [50.0, 50.0, 100.0]}}));
    let before = fx.world_pos(&comp, &child);
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": parent}),
    );
    assert_eq!(
        fx.value(&child, "parent"),
        Some(Value::PrimRef(parent.clone()))
    );
    let after = fx.world_pos(&comp, &child);
    assert!((before - after).length() < 1e-6, "{before} vs {after}");
    assert!(
        (fx.value(&child, "transform:rotation")
            .unwrap()
            .as_f64()
            .unwrap()
            + 90.0)
            .abs()
            < 1e-9
    );
    // Cycles are refused.
    assert!(
        fx.try_run(
            "layer.set_parent",
            json!({"layers": [parent], "parent": child})
        )
        .is_err()
    );
    // Unparent: back in place too.
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": null}),
    );
    assert_eq!(fx.value(&child, "parent"), None);
    let back = fx.world_pos(&comp, &child);
    assert!((before - back).length() < 1e-6);
    // Jump (no compensation).
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": parent, "keep_transform": false}),
    );
    assert!((fx.world_pos(&comp, &child) - before).length() > 1.0);
}

#[test]
fn stopwatch_set_at_and_keyframe_assistants() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let l = fx.layer(&comp, "shape", json!({"shape": "ellipse"}));
    let pos = "transform:position";
    // Stopwatch on: first key with the current value.
    fx.run(
        "attr.set_animated",
        json!({"path": l, "attr": pos, "animated": true, "time": 0}),
    );
    assert_eq!(fx.keys(&l, pos), vec![0]);
    // Editing at another time adds a key; at an existing key changes its value.
    fx.run("attr.set_at", json!({"path": l, "attr": pos, "time": SEC, "value": {"type": "vec3", "value": [100.0, 100.0, 0.0]}}));
    fx.run("attr.set_at", json!({"path": l, "attr": pos, "time": 2 * SEC, "value": {"type": "vec3", "value": [200.0, 100.0, 0.0]}}));
    assert_eq!(fx.keys(&l, pos), vec![0, SEC, 2 * SEC]);
    // Not animated: a static value.
    fx.run("attr.set_at", json!({"path": l, "attr": "transform:opacity", "time": SEC, "value": {"type": "float", "value": 50.0}}));
    assert_eq!(fx.value(&l, "transform:opacity"), Some(Value::Float(50.0)));
    assert!(fx.keys(&l, "transform:opacity").is_empty());
    // Type checks.
    assert!(
        fx.try_run(
            "attr.set_at",
            json!({"path": l, "attr": pos, "time": 0, "value": {"type": "bool", "value": true}})
        )
        .is_err()
    );

    // F9 on the middle key; scale gets one ease per dimension.
    fx.run("key.ease", json!({"keys": [key(&l, pos, SEC)]}));
    let spec = query::attr_spec(&fx.stage, &l, pos).unwrap();
    let k = &spec.keyframes[&Time(SEC)];
    assert_eq!(
        (k.interp_in, k.interp_out),
        (Interpolation::Bezier, Interpolation::Bezier)
    );
    assert_eq!(k.ease_in.len(), 1);
    assert_eq!(k.ease_in[0].speed, 0.0);
    assert!((k.ease_in[0].influence - 100.0 / 3.0).abs() < 1e-9);
    fx.run(
        "attr.set_animated",
        json!({"path": l, "attr": "transform:scale", "animated": true, "time": 0}),
    );
    fx.run(
        "key.ease",
        json!({"keys": [key(&l, "transform:scale", 0)], "mode": "ease_out"}),
    );
    let k = &query::attr_spec(&fx.stage, &l, "transform:scale")
        .unwrap()
        .keyframes[&Time(0)];
    assert_eq!(k.ease_out.len(), 3);
    assert_eq!(k.interp_in, Interpolation::Linear);
    // Hold.
    fx.run(
        "key.set_interpolation",
        json!({"keys": [key(&l, pos, 0)], "interp_out": "hold"}),
    );
    assert_eq!(
        query::attr_spec(&fx.stage, &l, pos).unwrap().keyframes[&Time(0)].interp_out,
        Interpolation::Hold
    );

    // Move two keys by half a second; output reports the new times.
    let out = fx.run(
        "key.move",
        json!({"keys": [key(&l, pos, SEC), key(&l, pos, 2 * SEC)], "delta": SEC / 2}),
    );
    assert_eq!(out["keys"].as_array().unwrap().len(), 2);
    assert_eq!(fx.keys(&l, pos), vec![0, 3 * SEC / 2, 5 * SEC / 2]);
    // Moving onto an existing key replaces it.
    fx.run(
        "key.move",
        json!({"keys": [key(&l, pos, 3 * SEC / 2)], "delta": -3 * SEC / 2}),
    );
    assert_eq!(fx.keys(&l, pos), vec![0, 5 * SEC / 2]);
    // Missing keys are an error and change nothing.
    assert!(
        fx.try_run("key.move", json!({"keys": [key(&l, pos, 7)], "delta": 1}))
            .is_err()
    );

    // Reverse.
    fx.run("attr.set_at", json!({"path": l, "attr": pos, "time": SEC, "value": {"type": "vec3", "value": [1.0, 2.0, 0.0]}}));
    fx.run(
        "key.reverse",
        json!({"keys": [key(&l, pos, 0), key(&l, pos, SEC), key(&l, pos, 5 * SEC / 2)]}),
    );
    assert_eq!(fx.keys(&l, pos), vec![0, 3 * SEC / 2, 5 * SEC / 2]);
    let spec = query::attr_spec(&fx.stage, &l, pos).unwrap();
    assert_eq!(
        spec.keyframes[&Time(3 * SEC / 2)].value,
        Value::Vec3([1.0, 2.0, 0.0])
    );
    // The eased key that replaced the first one is now last: its outgoing ease became
    // the incoming side.
    let last = &spec.keyframes[&Time(5 * SEC / 2)];
    assert_eq!(last.interp_in, Interpolation::Bezier);

    // Paste.
    let copied = json!([
        {"offset": 0, "keyframe": {"value": {"type": "float", "value": 10.0}}},
        {"offset": SEC, "keyframe": {"value": {"type": "float", "value": 90.0}}}
    ]);
    fx.run(
        "key.paste",
        json!({"path": l, "attr": "transform:rotation", "time": 2 * SEC, "keys": copied}),
    );
    assert_eq!(fx.keys(&l, "transform:rotation"), vec![2 * SEC, 3 * SEC]);
    // Paste Reversed Keyframes: the last copied value first.
    fx.run(
        "key.paste",
        json!({"path": l, "attr": "transform:rotation", "time": 5 * SEC, "keys": copied, "reversed": true}),
    );
    let at = |fx: &Fx, t: i64| {
        query::attr_spec(&fx.stage, &l, "transform:rotation")
            .and_then(|s| s.keyframes.get(&Time(t)).map(|k| k.value.clone()))
    };
    assert_eq!(at(&fx, 5 * SEC), Some(Value::Float(90.0)));
    assert_eq!(at(&fx, 6 * SEC), Some(Value::Float(10.0)));
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.keys(&l, "transform:rotation"), vec![2 * SEC, 3 * SEC]);

    // Delete all: stopwatch off with the last deleted value.
    fx.run("key.delete", json!({"keys": [key(&l, "transform:rotation", 2 * SEC), key(&l, "transform:rotation", 3 * SEC)]}));
    assert!(fx.keys(&l, "transform:rotation").is_empty());
    assert_eq!(fx.value(&l, "transform:rotation"), Some(Value::Float(90.0)));

    // Stopwatch off keeps the value at the time.
    fx.run(
        "attr.set_animated",
        json!({"path": l, "attr": pos, "animated": false, "time": 3 * SEC / 2}),
    );
    assert!(fx.keys(&l, pos).is_empty());
    assert_eq!(fx.value(&l, pos), Some(Value::Vec3([1.0, 2.0, 0.0])));
    // Not animatable.
    assert!(
        fx.try_run(
            "attr.set_animated",
            json!({"path": l, "attr": "name", "animated": true, "time": 0})
        )
        .is_err()
    );
}

#[test]
fn work_area_and_trim_comp() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let l = fx.layer(&comp, "solid", json!({}));
    fx.run(
        "comp.set_work_area",
        json!({"comp": comp, "start": 2 * SEC, "end": 6 * SEC}),
    );
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!(
        (info.work_area.start, info.work_area.end),
        (Time(2 * SEC), Time(6 * SEC))
    );
    // Start past the end drags the end along.
    fx.run(
        "comp.set_work_area",
        json!({"comp": comp, "start": 7 * SEC}),
    );
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!(info.work_area.start, Time(7 * SEC));
    assert_eq!(
        info.work_area.end,
        Time(7 * SEC) + FrameRate::FPS_30.frame_duration()
    );
    fx.run(
        "comp.set_work_area",
        json!({"comp": comp, "start": 2 * SEC, "end": 6 * SEC}),
    );
    fx.run("comp.trim_to_work_area", json!({"comp": comp}));
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!(info.duration, Time(4 * SEC));
    assert_eq!(
        query::layer_timing(&fx.stage, &l).start_time,
        Time(-2 * SEC)
    );
}

#[test]
fn module_publishes_commands() {
    let mut b = bae_app::AppBuilder::new();
    b.add_module(bae_motion::MotionModule);
    let mut app = b.build();
    let reg = app.registry::<dyn DynCommand>();
    assert!(reg.contains("comp.create"));
    assert!(reg.contains("key.ease"));
    // Plus the effect commands, which read the app's effect definitions.
    assert!(reg.contains("effect.apply"));
    let effects = bae_motion::effect::effect_commands(Default::default()).len();
    assert_eq!(reg.len(), bae_motion::commands().len() + effects);
    // Every command has a JSON schema with documented parameters.
    for c in bae_motion::commands() {
        assert!(c.info().params_schema.is_object(), "{}", c.info().id);
    }
}

#[test]
fn delete_and_batch_edits() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let child = fx.layer(&comp, "null", json!({"position": [300.0, 200.0]}));
    let parent = fx.layer(&comp, "null", json!({"position": [1000.0, 500.0]}));
    fx.run("attr.set", json!({"path": parent, "attr": "transform:rotation", "value": {"type": "float", "value": 45.0}}));
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": parent}),
    );
    let before = fx.world_pos(&comp, &child);
    // Deleting the parent unparents the child in place, in one undo step.
    fx.run("layer.delete", json!({"layers": [parent]}));
    assert!(!fx.stage.has_prim(&parent));
    assert_eq!(fx.value(&child, "parent"), None);
    assert!((fx.world_pos(&comp, &child) - before).length() < 1e-6);
    fx.stage.undo(&fx.author).unwrap();
    assert!(fx.stage.has_prim(&parent));
    assert_eq!(
        fx.value(&child, "parent"),
        Some(Value::PrimRef(parent.clone()))
    );

    // Several values in one step.
    fx.run("attr.set_many", json!({"edits": [
        {"path": child, "attr": "transform:opacity", "time": 0, "value": {"type": "float", "value": 10.0}},
        {"path": parent, "attr": "transform:opacity", "time": 0, "value": {"type": "float", "value": 20.0}}
    ]}));
    assert_eq!(
        fx.value(&child, "transform:opacity"),
        Some(Value::Float(10.0))
    );
    fx.stage.undo(&fx.author).unwrap();
    // Both are nulls: AE 26 makes nulls at 0% opacity.
    assert_eq!(
        fx.value(&parent, "transform:opacity"),
        Some(Value::Float(0.0))
    );
    assert_eq!(
        fx.value(&child, "transform:opacity"),
        Some(Value::Float(0.0))
    );

    // A solid layer from an existing solid item reuses it.
    let solid = fx.layer(&comp, "solid", json!({"size": [200, 100]}));
    let item = fx
        .value(&solid, "source")
        .unwrap()
        .as_prim_ref()
        .unwrap()
        .clone();
    let again = fx.layer(&comp, "solid", json!({"source": item}));
    assert_eq!(
        fx.value(&again, "source"),
        Some(Value::PrimRef(item.clone()))
    );
    assert_eq!(
        fx.value(&again, "transform:anchor_point"),
        Some(Value::Vec3([100.0, 50.0, 0.0]))
    );

    // Project items: a comp and a folder with contents, together.
    let out = fx.run("folder.create", json!({"name": "Old"}));
    let folder = p(out["path"].as_str().unwrap());
    let inner = fx.run("comp.create", json!({"parent": folder}));
    let inner = p(inner["path"].as_str().unwrap());
    fx.run("item.delete", json!({"paths": [folder, inner, comp]}));
    assert!(!fx.stage.has_prim(&folder));
    assert!(!fx.stage.has_prim(&comp));
    assert!(fx.stage.has_prim(&item));
}

/// Issue #124 (AE): a locked layer, its properties and keyframes cannot be
/// changed by commands until it is unlocked; the Lock switch itself still can.
#[test]
fn locked_layers_refuse_edits_until_unlocked() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let b = fx.layer(&comp, "solid", json!({}));
    fx.run(
        "attr.set_animated",
        json!({"path": b, "attr": "transform:opacity", "animated": true, "time": 0}),
    );
    let lock = |fx: &mut Fx, on: bool| {
        fx.run(
            "attr.set",
            json!({"path": b, "attr": "locked", "value": {"type": "bool", "value": on}}),
        );
    };
    lock(&mut fx, true);
    assert!(query::is_locked(&fx.stage, &b));
    let key = json!([{"path": b, "attr": "transform:opacity", "time": 0}]);
    for (id, params) in [
        ("layer.delete", json!({"layers": [b]})),
        ("layer.reorder", json!({"layers": [b], "index": 0})),
        ("layer.split", json!({"layers": [b], "time": SEC})),
        ("layer.trim", json!({"layers": [b], "in_point": SEC})),
        ("layer.shift_time", json!({"layers": [b], "delta": SEC})),
        ("layer.set_parent", json!({"layers": [b], "parent": a})),
        (
            "attr.set_at",
            json!({"path": b, "attr": "transform:rotation", "time": 0, "value": {"type": "float", "value": 45.0}}),
        ),
        ("key.delete", json!({"keys": key})),
        ("key.move", json!({"keys": key, "delta": SEC})),
    ] {
        let error = fx.try_run(id, params).expect_err(id);
        assert!(error.contains("locked"), "{id}: {error}");
    }
    // Other layers are unaffected, even when the locked one moves in the stack.
    fx.run("layer.reorder", json!({"layers": [a], "index": 1}));
    // Duplicating reads the locked layer without changing it.
    fx.run("layer.duplicate", json!({"layers": [b]}));
    // Unlocked: editable again.
    lock(&mut fx, false);
    fx.run("layer.delete", json!({"layers": [b]}));
    assert!(fx.stage.prim(&b).is_none());
}

#[test]
fn locked_layers_follow_comp_level_changes() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let child = fx.layer(&comp, "null", json!({"position": [300.0, 200.0]}));
    let parent = fx.layer(&comp, "null", json!({"position": [1000.0, 500.0]}));
    fx.run("attr.set", json!({"path": parent, "attr": "transform:rotation", "value": {"type": "float", "value": 90.0}}));
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": parent}),
    );
    fx.run(
        "attr.set",
        json!({"path": child, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    // The locked child itself: a typed error that names it.
    let error = fx
        .reg
        .execute(
            &mut fx.stage,
            &fx.author,
            "layer.set_parent",
            json!({"layers": [child], "parent": null}),
        )
        .unwrap_err();
    assert!(
        matches!(
            &error,
            bae_model::CommandError::Model(bae_model::ModelError::LayerLocked { layer, .. })
                if *layer == child
        ),
        "{error:?}"
    );
    // AE-PRJ-386: deleting the parent unparents the locked child in place.
    let before = fx.world_pos(&comp, &child);
    fx.run("layer.delete", json!({"layers": [parent]}));
    assert!(fx.stage.prim(&parent).is_none());
    assert_eq!(fx.value(&child, "parent"), None);
    let after = fx.world_pos(&comp, &child);
    assert!((before - after).length() < 1e-6, "{before} vs {after}");
    assert!(query::is_locked(&fx.stage, &child));
    // Still locked for direct edits afterwards.
    assert!(
        fx.try_run(
            "attr.set",
            json!({"path": child, "attr": "transform:opacity", "value": {"type": "float", "value": 5.0}}),
        )
        .unwrap_err()
        .contains("locked")
    );
    // AE-PRJ-319: Trim Comp to Work Area moves every layer, locked ones too.
    fx.run(
        "comp.set_work_area",
        json!({"comp": comp, "start": 2 * SEC, "end": 6 * SEC}),
    );
    fx.run("comp.trim_to_work_area", json!({"comp": comp}));
    assert_eq!(
        query::layer_timing(&fx.stage, &child).start_time,
        Time(-2 * SEC)
    );
}

#[test]
fn edit_keys_changes_time_value_and_ease_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let attr = "transform:rotation";
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": attr, "animated": true, "time": 0}),
    );
    fx.run("attr.set_at", json!({"path": solid, "attr": attr, "time": SEC, "value": {"type": "float", "value": 90.0}}));
    let out = fx.run(
        "key.edit",
        json!({"edits": [{
            "key": key(&solid, attr, SEC),
            "time": 2 * SEC,
            "value": {"type": "float", "value": 180.0},
            "interp_in": "bezier",
            "ease_in": [{"speed": 10.0, "influence": 250.0}],
        }]}),
    );
    assert_eq!(out["keys"][0]["time"], json!(2 * SEC));
    assert_eq!(fx.keys(&solid, attr), vec![0, 2 * SEC]);
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    let k = &spec.keyframes[&Time(2 * SEC)];
    assert_eq!(k.value, Value::Float(180.0));
    assert_eq!(k.interp_in, Interpolation::Bezier);
    assert_eq!(k.interp_out, Interpolation::Linear);
    // Influence is clamped to AE's 0.1–100 %.
    assert_eq!((k.ease_in[0].speed, k.ease_in[0].influence), (10.0, 100.0));
    // One undo step restores the key at 1 s with its old value.
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.keys(&solid, attr), vec![0, SEC]);
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    assert_eq!(spec.keyframes[&Time(SEC)].value, Value::Float(90.0));
}

#[test]
fn edit_keys_swaps_and_replaces() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let attr = "transform:opacity";
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": attr, "animated": true, "time": 0}),
    );
    for (t, v) in [(SEC, 50.0), (2 * SEC, 0.0)] {
        fx.run(
            "attr.set_at",
            json!({"path": solid, "attr": attr, "time": t, "value": {"type": "float", "value": v}}),
        );
    }
    // Swap the keys at 0 and 1 s.
    fx.run(
        "key.edit",
        json!({"edits": [
            {"key": key(&solid, attr, 0), "time": SEC},
            {"key": key(&solid, attr, SEC), "time": 0},
        ]}),
    );
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    assert_eq!(spec.keyframes[&Time(0)].value, Value::Float(50.0));
    assert_eq!(spec.keyframes[&Time(SEC)].value, Value::Float(100.0));
    // A key moved onto an unedited key replaces it.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, 0), "time": 2 * SEC}]}),
    );
    assert_eq!(fx.keys(&solid, attr), vec![SEC, 2 * SEC]);
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    assert_eq!(spec.keyframes[&Time(2 * SEC)].value, Value::Float(50.0));
}

#[test]
fn auto_bezier_is_recomputed_after_edits_and_moves() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let attr = "transform:rotation";
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": attr, "animated": true, "time": 0}),
    );
    for (t, v) in [(SEC, 100.0), (2 * SEC, 300.0)] {
        fx.run(
            "attr.set_at",
            json!({"path": solid, "attr": attr, "time": t, "value": {"type": "float", "value": v}}),
        );
    }
    // Cmd+click: Linear → Auto Bezier on the middle key.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, SEC), "auto_bezier": true}]}),
    );
    let speed = |fx: &Fx, t: i64| {
        let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
        let k = &spec.keyframes[&Time(t)];
        assert!(k.flags.temporal_auto_bezier);
        (k.ease_in[0].speed, k.ease_out[0].speed)
    };
    // Slope between the neighbours: (300 − 0) / 2 s.
    assert_eq!(speed(&fx, SEC), (150.0, 150.0));
    // Moving the last key recomputes it: (300 − 0) / 3 s.
    fx.run(
        "key.move",
        json!({"keys": [key(&solid, attr, 2 * SEC)], "delta": SEC}),
    );
    assert_eq!(speed(&fx, SEC), (100.0, 100.0));
    // Dragging its handle makes it a continuous Bezier.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, SEC), "ease_out": [{"speed": 20.0, "influence": 50.0}]}]}),
    );
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    let k = &spec.keyframes[&Time(SEC)];
    assert!(!k.flags.temporal_auto_bezier && k.flags.temporal_continuous);
    assert_eq!(k.ease_out[0].speed, 20.0);
}

#[test]
fn edit_keys_sets_spatial_interpolation_and_roving() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let attr = "transform:position";
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": attr, "animated": true, "time": 0}),
    );
    for (t, x, y) in [(SEC, 100.0, 0.0), (2 * SEC, 100.0, 100.0)] {
        fx.run(
            "attr.set_at",
            json!({"path": solid, "attr": attr, "time": t, "value": {"type": "vec3", "value": [x, y, 0.0]}}),
        );
    }
    let flags = |fx: &Fx, t: i64| {
        let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
        let k = &spec.keyframes[&Time(t)];
        (
            k.flags.spatial_continuous,
            k.flags.spatial_auto_bezier,
            k.flags.roving,
            k.spatial_out,
        )
    };
    // Spatial Auto Bezier: continuous too, tangents computed from the neighbours.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, SEC), "spatial_auto_bezier": true}]}),
    );
    let (c, a, _, out) = flags(&fx, SEC);
    assert!(c && a);
    assert!(out.is_some_and(|t| t != [0.0; 3]), "{out:?}");
    // Continuous Bezier: keeps the tangents, no longer automatic.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, SEC), "spatial_auto_bezier": false, "spatial_continuous": true}]}),
    );
    let (c, a, _, kept) = flags(&fx, SEC);
    assert!(c && !a);
    assert_eq!(kept, out);
    // Linear: zero tangents, neither flag; and Rove Across Time.
    fx.run(
        "key.edit",
        json!({"edits": [{"key": key(&solid, attr, SEC), "spatial_in": [0.0, 0.0, 0.0], "spatial_out": [0.0, 0.0, 0.0], "spatial_continuous": false, "spatial_auto_bezier": false, "roving": true}]}),
    );
    let spec = query::attr_spec(&fx.stage, &solid, attr).unwrap();
    let k = spec.keyframes.values().nth(1).unwrap();
    assert!(!k.flags.spatial_continuous && !k.flags.spatial_auto_bezier && k.flags.roving);
    assert_eq!(k.spatial_out, Some([0.0; 3]));
    // Each edit is one undo step.
    fx.stage.undo(&fx.author).unwrap();
    let (c, a, r, _) = flags(&fx, SEC);
    assert!(c && !a && !r);
}

#[test]
fn separate_and_recombine_position() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let pos = "transform:position";
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": pos, "animated": true, "time": 0}),
    );
    fx.run("attr.set_at", json!({"path": solid, "attr": pos, "time": 2 * SEC, "value": {"type": "vec3", "value": [1160.0, 340.0, 0.0]}}));
    let mid = |fx: &Fx| {
        let e = bae_anim::evaluate_comp(&fx.stage, &comp, Time(SEC)).unwrap();
        e.layer(&solid)
            .unwrap()
            .world_matrix
            .transform_point3(DVec3::ZERO)
    };
    let before = mid(&fx);
    fx.run("transform.separate_dimensions", json!({"layers": [solid]}));
    assert_eq!(
        fx.value(&solid, "transform:separate_dimensions"),
        Some(Value::Bool(true))
    );
    assert_eq!(fx.keys(&solid, "transform:position_x"), vec![0, 2 * SEC]);
    assert_eq!(fx.keys(&solid, "transform:position_y"), vec![0, 2 * SEC]);
    // 2D layer: Z stays static.
    assert!(fx.keys(&solid, "transform:position_z").is_empty());
    let spec = query::attr_spec(&fx.stage, &solid, "transform:position_x").unwrap();
    assert_eq!(spec.keyframes[&Time(2 * SEC)].value, Value::Float(1160.0));
    // A linear move evaluates the same separated.
    let after = mid(&fx);
    assert!((before - after).length() < 1e-6, "{before} {after}");
    // Move Y's last key in time: recombining keys every component key time.
    fx.run(
        "key.move",
        json!({"keys": [key(&solid, "transform:position_y", 2 * SEC)], "delta": SEC}),
    );
    fx.run(
        "transform.separate_dimensions",
        json!({"layers": [solid], "separated": false}),
    );
    assert_eq!(
        fx.value(&solid, "transform:separate_dimensions"),
        Some(Value::Bool(false))
    );
    assert_eq!(fx.keys(&solid, pos), vec![0, 2 * SEC, 3 * SEC]);
    let spec = query::attr_spec(&fx.stage, &solid, pos).unwrap();
    assert_eq!(
        spec.keyframes[&Time(3 * SEC)].value,
        Value::Vec3([1160.0, 340.0, 0.0])
    );
    // Undo goes back to separated.
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        fx.value(&solid, "transform:separate_dimensions"),
        Some(Value::Bool(true))
    );
}

#[test]
fn comp_settings_resize_keeps_layers_in_place_and_undo_restores() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let nul = fx.layer(&comp, "null", json!({}));
    let child = fx.layer(&comp, "shape", json!({"shape": "rect"}));
    let camera = fx.layer(&comp, "camera", json!({}));
    fx.run(
        "layer.set_parent",
        json!({"layers": [child], "parent": nul}),
    );
    // A keyframed position moves with the frame, keyframes included.
    fx.run(
        "attr.set_animated",
        json!({"path": solid, "attr": "transform:position", "animated": true, "time": 0}),
    );
    fx.run(
        "attr.set_at",
        json!({"path": solid, "attr": "transform:position", "time": SEC, "value": {"type": "vec3", "value": [100.0, 200.0, 0.0]}}),
    );
    let child_before = fx.value(&child, "transform:position");
    let child_world = fx.world_pos(&comp, &child);
    fx.run(
        "comp.set_settings",
        json!({"comp": comp, "name": "Main", "width": 1280, "height": 720,
               "fps": 29.97, "duration": 5 * SEC, "bg_color": [1.0, 1.0, 1.0, 1.0],
               "start_timecode": SEC, "shutter_angle": 90.0, "renderer": "advanced_3d"}),
    );
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!((info.width, info.height), (1280, 720));
    assert_eq!(info.name, "Main");
    assert_eq!(info.frame_rate, FrameRate::FPS_29_97);
    assert_eq!(info.duration, Time(5 * SEC));
    assert_eq!(info.display_start, Time(SEC));
    assert_eq!(info.bg_color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(info.work_area.end, Time(5 * SEC));
    // Center anchor: top-level layers move by half the size change.
    let spec = query::attr_spec(&fx.stage, &solid, "transform:position").unwrap();
    let at = |t: i64| spec.keyframes.get(&Time(t)).unwrap().value.clone();
    assert_eq!(at(0), Value::Vec3([640.0, 360.0, 0.0]));
    assert_eq!(at(SEC), Value::Vec3([-220.0, 20.0, 0.0]));
    assert_eq!(
        fx.value(&nul, "transform:position"),
        Some(Value::Vec3([640.0, 360.0, 0.0]))
    );
    // The camera's point of interest moves too; children stay relative to
    // their parent (and so keep their place on screen).
    assert_eq!(
        fx.value(&camera, "transform:point_of_interest"),
        Some(Value::Vec3([640.0, 360.0, 0.0]))
    );
    assert_eq!(fx.value(&child, "transform:position"), child_before);
    let moved = fx.world_pos(&comp, &child);
    assert!((moved - child_world - DVec3::new(-320.0, -180.0, 0.0)).length() < 1e-6);
    assert_eq!(
        fx.value(&comp, "renderer"),
        Some(Value::Enum("advanced_3d".into()))
    );
    // One undo step restores everything.
    fx.stage.undo(&fx.author).unwrap();
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!(
        (info.width, info.height, info.name.as_str()),
        (1920, 1080, "Comp 1")
    );
    assert_eq!(
        fx.value(&nul, "transform:position"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
}

#[test]
fn comp_settings_anchor_and_validation() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    // Top-left anchor: layers keep their coordinates.
    fx.run(
        "comp.set_settings",
        json!({"comp": comp, "width": 1000, "height": 500, "anchor": "top_left"}),
    );
    assert_eq!(
        fx.value(&solid, "transform:position"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
    // Bottom-right anchor: they move by the whole change.
    fx.run(
        "comp.set_settings",
        json!({"comp": comp, "width": 1100, "height": 600, "anchor": "bottom_right"}),
    );
    assert_eq!(
        fx.value(&solid, "transform:position"),
        Some(Value::Vec3([1060.0, 640.0, 0.0]))
    );
    assert!(
        fx.try_run("comp.set_settings", json!({"comp": comp, "width": 0}))
            .is_err()
    );
    assert!(
        fx.try_run("comp.set_settings", json!({"comp": comp, "name": "  "}))
            .is_err()
    );
    assert!(
        fx.try_run("comp.set_settings", json!({"comp": comp, "duration": 0}))
            .is_err()
    );
    // Shortening the comp clamps the work area.
    fx.run(
        "comp.set_work_area",
        json!({"comp": comp, "start": 2 * SEC, "end": 8 * SEC}),
    );
    fx.run(
        "comp.set_settings",
        json!({"comp": comp, "duration": 4 * SEC}),
    );
    let info = query::comp_info(&fx.stage, &comp).unwrap();
    assert_eq!(info.work_area.start, Time(2 * SEC));
    assert_eq!(info.work_area.end, Time(4 * SEC));
    // comp.create takes the same settings.
    let out = fx.run(
        "comp.create",
        json!({"width": 640, "height": 480, "fps": 25, "resolution": [2, 2],
               "start_timecode": SEC, "preserve_resolution": true, "drop_frame": false}),
    );
    let c2 = p(out["path"].as_str().unwrap());
    assert_eq!(fx.value(&c2, "resolution"), Some(Value::Vec2([2.0, 2.0])));
    assert_eq!(
        fx.value(&c2, "preserve_resolution"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        fx.value(&c2, "display_start_time"),
        Some(Value::Time(Time(SEC)))
    );
}

#[test]
fn solid_settings_edit_the_solid_and_keep_the_layer_centered() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let layer = fx.layer(
        &comp,
        "solid",
        json!({"name": "Dark Gray Solid 1", "color": [0.2, 0.2, 0.2, 1.0], "pixel_aspect": 2.0}),
    );
    let item = bae_motion::solid::layer_solid(&fx.stage, &layer).unwrap();
    let info = bae_motion::solid::solid_info(&fx.stage, &item).unwrap();
    assert_eq!(
        (info.width, info.height, info.pixel_aspect),
        (1920, 1080, 2.0)
    );
    let out = fx.run(
        "solid.set_settings",
        json!({"target": layer, "name": "Red Solid 1", "width": 500, "height": 300,
               "pixel_aspect": 1.0, "color": [1.0, 0.0, 0.0, 1.0]}),
    );
    // The only user: the item changes in place.
    assert_eq!(out["solid"], json!(item));
    let info = bae_motion::solid::solid_info(&fx.stage, &item).unwrap();
    assert_eq!(info.name, "Red Solid 1");
    assert_eq!(
        (info.width, info.height, info.pixel_aspect),
        (500, 300, 1.0)
    );
    assert_eq!(info.color, [1.0, 0.0, 0.0, 1.0]);
    // The layer showed the solid's name and follows it; its anchor scales
    // with the size (AE), so the solid stays centered in the comp.
    assert_eq!(query::display_name_of(&fx.stage, &layer), "Red Solid 1");
    assert_eq!(
        fx.value(&layer, "transform:anchor_point"),
        Some(Value::Vec3([250.0, 150.0, 0.0]))
    );
    // An off-center anchor scales too (R1: [50,50] on 400x300 → [75,83.33] on 600x500).
    fx.run(
        "attr.set_at",
        json!({"path": layer, "attr": "transform:anchor_point", "time": 0, "value": {"type": "vec3", "value": [50.0, 50.0, 0.0]}}),
    );
    fx.run(
        "solid.set_settings",
        json!({"target": layer, "width": 750, "height": 500}),
    );
    match fx.value(&layer, "transform:anchor_point") {
        Some(Value::Vec3([x, y, _])) => {
            assert!(
                (x - 75.0).abs() < 1e-9 && (y - 50.0 * 500.0 / 300.0).abs() < 1e-9,
                "{x} {y}"
            )
        }
        other => panic!("{other:?}"),
    }
    fx.stage.undo(&fx.author).unwrap();
    fx.stage.undo(&fx.author).unwrap();
    // One undo step.
    fx.stage.undo(&fx.author).unwrap();
    let info = bae_motion::solid::solid_info(&fx.stage, &item).unwrap();
    assert_eq!(
        (info.name.as_str(), info.width),
        ("Dark Gray Solid 1", 1920)
    );
    assert_eq!(
        query::display_name_of(&fx.stage, &layer),
        "Dark Gray Solid 1"
    );
    assert_eq!(
        fx.value(&layer, "transform:anchor_point"),
        Some(Value::Vec3([960.0, 540.0, 0.0]))
    );
}

#[test]
fn solid_settings_fork_or_affect_all_layers_that_use_the_solid() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({"name": "Gray Solid 1"}));
    let item = bae_motion::solid::layer_solid(&fx.stage, &a).unwrap();
    let b = fx.layer(&comp, "solid", json!({"source": item}));
    // A renamed layer keeps its own name.
    fx.run(
        "attr.set",
        json!({"path": b, "attr": "name", "value": {"type": "string", "value": "BG"}}),
    );
    assert_eq!(bae_motion::solid::solid_users(&fx.stage, &item).len(), 2);
    // Unchecked "Affect all layers that use this solid": A gets its own copy.
    let out = fx.run(
        "solid.set_settings",
        json!({"target": a, "name": "Blue Solid 1", "color": [0.0, 0.0, 1.0, 1.0]}),
    );
    let copy = p(out["solid"].as_str().unwrap());
    assert_ne!(copy, item);
    assert_eq!(
        bae_motion::solid::layer_solid(&fx.stage, &a),
        Some(copy.clone())
    );
    assert_eq!(
        bae_motion::solid::layer_solid(&fx.stage, &b),
        Some(item.clone())
    );
    let shared = bae_motion::solid::solid_info(&fx.stage, &item).unwrap();
    assert_eq!(shared.name, "Gray Solid 1");
    let forked = bae_motion::solid::solid_info(&fx.stage, &copy).unwrap();
    assert_eq!(
        (forked.name.as_str(), forked.color),
        ("Blue Solid 1", [0.0, 0.0, 1.0, 1.0])
    );
    assert_eq!((forked.width, forked.height), (shared.width, shared.height));
    fx.stage.undo(&fx.author).unwrap();
    assert!(!fx.stage.has_prim(&copy));
    assert_eq!(
        bae_motion::solid::layer_solid(&fx.stage, &a),
        Some(item.clone())
    );
    // Checked: the shared solid changes for both layers.
    fx.run(
        "solid.set_settings",
        json!({"target": a, "name": "Green Solid 1", "affect_all": true}),
    );
    assert_eq!(
        bae_motion::solid::layer_solid(&fx.stage, &a),
        Some(item.clone())
    );
    assert_eq!(query::display_name_of(&fx.stage, &item), "Green Solid 1");
    assert_eq!(query::display_name_of(&fx.stage, &a), "Green Solid 1");
    assert_eq!(query::display_name_of(&fx.stage, &b), "BG");
    // The solid item itself (Project panel) always affects every layer.
    let out = fx.run("solid.set_settings", json!({"target": item, "width": 100}));
    assert_eq!(out["layers"].as_array().unwrap().len(), 2);
    assert_eq!(out["solid"], json!(item));
}

#[test]
fn solid_settings_validation() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    let shape = fx.layer(&comp, "shape", json!({"shape": "rect"}));
    let adjustment = fx.layer(&comp, "adjustment", json!({}));
    assert!(
        fx.try_run("solid.set_settings", json!({"target": shape}))
            .is_err()
    );
    assert!(
        fx.try_run("solid.set_settings", json!({"target": comp}))
            .is_err()
    );
    for bad in [
        json!({"name": "  "}),
        json!({"width": 0}),
        json!({"height": 30001}),
        json!({"pixel_aspect": 0.0}),
    ] {
        let mut params = bad.clone();
        params["target"] = json!(solid);
        assert!(fx.try_run("solid.set_settings", params).is_err(), "{bad}");
    }
    // Adjustment layers use a solid too (AE: Solid Settings works on them).
    assert!(
        fx.try_run(
            "solid.set_settings",
            json!({"target": adjustment, "width": 640})
        )
        .is_ok()
    );
    assert!(
        fx.try_run(
            "layer.create",
            json!({"comp": comp, "kind": "solid", "pixel_aspect": -1.0})
        )
        .is_err()
    );
}

#[test]
fn new_cameras_and_lights_take_their_settings() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let cam = fx.layer(
        &comp,
        "camera",
        json!({"name": "Camera 1", "camera": {"camera_type": "one_node", "zoom": 1280.0,
               "depth_of_field": true, "focus_distance": 1000.0, "aperture": 50.0}}),
    );
    assert_eq!(
        fx.value(&cam, "camera:type"),
        Some(Value::Enum("one_node".into()))
    );
    assert_eq!(fx.value(&cam, "camera:zoom"), Some(Value::Float(1280.0)));
    assert_eq!(
        fx.value(&cam, "camera:depth_of_field"),
        Some(Value::Bool(true))
    );
    assert_eq!(
        fx.value(&cam, "camera:focus_distance"),
        Some(Value::Float(1000.0))
    );
    // The camera stands `zoom` in front of the comp.
    assert_eq!(
        fx.value(&cam, "transform:position"),
        Some(Value::Vec3([960.0, 540.0, -1280.0]))
    );
    // Without settings: AE's 50mm camera, focus distance = zoom.
    let default_cam = fx.layer(&comp, "camera", json!({}));
    let zoom = 1920.0 * 50.0 / 36.0;
    assert_eq!(
        fx.value(&default_cam, "camera:zoom"),
        Some(Value::Float(zoom))
    );
    assert_eq!(
        fx.value(&default_cam, "camera:focus_distance"),
        Some(Value::Float(zoom))
    );
    let light = fx.layer(
        &comp,
        "light",
        json!({"name": "Point Light 1", "light": {"light_type": "point", "intensity": 150.0,
               "color": [1.0, 0.5, 0.0, 1.0], "casts_shadows": true}}),
    );
    assert_eq!(
        fx.value(&light, "light:type"),
        Some(Value::Enum("point".into()))
    );
    assert_eq!(
        fx.value(&light, "light:intensity"),
        Some(Value::Float(150.0))
    );
    assert_eq!(
        fx.value(&light, "light:casts_shadows"),
        Some(Value::Bool(true))
    );
    assert!(
        fx.try_run(
            "layer.create",
            json!({"comp": comp, "kind": "light", "light": {"light_type": "laser"}})
        )
        .is_err()
    );
}

#[test]
fn camera_and_light_settings_edit_at_the_current_time_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let cam = fx.layer(&comp, "camera", json!({}));
    let light = fx.layer(&comp, "light", json!({}));
    // Zoom is keyframed: the edit adds a keyframe at the time; static values change.
    fx.run(
        "attr.set_animated",
        json!({"path": cam, "attr": "camera:zoom", "animated": true, "time": 0}),
    );
    fx.run(
        "camera.set_settings",
        json!({"layer": cam, "name": "Main Cam", "camera_type": "one_node", "zoom": 800.0,
               "blur_level": 50.0, "time": SEC}),
    );
    assert_eq!(fx.keys(&cam, "camera:zoom"), vec![0, SEC]);
    assert_eq!(
        fx.value(&cam, "camera:blur_level"),
        Some(Value::Float(50.0))
    );
    assert_eq!(query::display_name_of(&fx.stage, &cam), "Main Cam");
    // One-node: no longer aims at the point of interest.
    assert_eq!(
        fx.value(&cam, "auto_orient"),
        Some(Value::Enum("off".into()))
    );
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(fx.keys(&cam, "camera:zoom"), vec![0]);
    assert_eq!(query::display_name_of(&fx.stage, &cam), "Camera 1");
    fx.run(
        "light.set_settings",
        json!({"layer": light, "light_type": "parallel", "cone_angle": 45.0, "falloff": "smooth"}),
    );
    assert_eq!(
        fx.value(&light, "light:type"),
        Some(Value::Enum("parallel".into()))
    );
    assert_eq!(
        fx.value(&light, "light:falloff"),
        Some(Value::Enum("smooth".into()))
    );
    // Wrong kinds and out-of-range values are refused.
    assert!(
        fx.try_run("camera.set_settings", json!({"layer": light}))
            .is_err()
    );
    assert!(
        fx.try_run("light.set_settings", json!({"layer": cam}))
            .is_err()
    );
    assert!(
        fx.try_run(
            "light.set_settings",
            json!({"layer": light, "cone_angle": 270.0})
        )
        .is_err()
    );
    assert!(
        fx.try_run("camera.set_settings", json!({"layer": cam, "zoom": 0.0}))
            .is_err()
    );
    assert!(
        fx.try_run("camera.set_settings", json!({"layer": cam, "name": " "}))
            .is_err()
    );
}

#[test]
fn precompose_moves_layers_into_a_new_comp_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let nul = fx.layer(&comp, "null", json!({}));
    let solid = fx.layer(&comp, "solid", json!({"name": "BG"}));
    let shape = fx.layer(&comp, "shape", json!({"shape": "rect"}));
    let text = fx.layer(&comp, "text", json!({"text": "Hi"}));
    // Stack: text, shape, solid, null. The shape follows the solid (moved
    // along) and the solid follows the null (stays outside).
    fx.run(
        "layer.set_parent",
        json!({"layers": [shape], "parent": solid, "keep_transform": false}),
    );
    fx.run(
        "layer.set_parent",
        json!({"layers": [solid], "parent": nul, "keep_transform": false}),
    );
    let names = |fx: &Fx, c: &PrimPath| -> Vec<String> {
        query::comp_layers(&fx.stage, c)
            .iter()
            .map(|l| query::display_name_of(&fx.stage, l))
            .collect()
    };
    assert_eq!(
        bae_motion::precompose::default_precomp_name(&fx.stage, &[shape.clone(), solid.clone()]),
        "Pre-comp 1"
    );
    let solid_world = fx.world_pos(&comp, &solid);
    let out = fx.run(
        "layer.precompose",
        json!({"layers": [solid, shape], "mode": "move"}),
    );
    let inner = p(out["comp"].as_str().unwrap());
    let pre = p(out["layer"].as_str().unwrap());
    let info = query::comp_info(&fx.stage, &inner).unwrap();
    assert_eq!(info.name, "Pre-comp 1");
    assert_eq!(
        (info.width, info.height, info.duration),
        (1920, 1080, Time::from_seconds(10.0))
    );
    // The new comp holds the two layers in their order; the pre-comp layer
    // takes the topmost one's place.
    assert_eq!(names(&fx, &inner), ["Shape Layer 1", "BG"]);
    assert_eq!(names(&fx, &comp), ["Hi", "Pre-comp 1", "Null 1"]);
    assert_eq!(query::comp_layers(&fx.stage, &comp)[1], pre);
    assert_eq!(
        fx.value(&pre, "source"),
        Some(Value::PrimRef(inner.clone()))
    );
    // The parent inside the new comp follows; the one across comps is gone.
    let moved = query::comp_layers(&fx.stage, &inner);
    assert_eq!(
        fx.value(&moved[0], "parent"),
        Some(Value::PrimRef(moved[1].clone()))
    );
    assert_eq!(fx.value(&moved[1], "parent"), None);
    // … and the layer keeps its place on screen (AE bakes the parent in).
    assert!((fx.world_pos(&inner, &moved[1]) - solid_world).length() < 1e-6);
    let _ = text;
    // One undo step.
    fx.stage.undo(&fx.author).unwrap();
    assert!(!fx.stage.has_prim(&inner));
    assert_eq!(names(&fx, &comp), ["Hi", "Shape Layer 1", "BG", "Null 1"]);
    assert_eq!(
        fx.value(&solid, "parent"),
        Some(Value::PrimRef(nul.clone()))
    );
}

#[test]
fn precompose_adjusts_the_duration_to_the_layers() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({"start": 2 * SEC}));
    let b = fx.layer(&comp, "null", json!({"start": 3 * SEC}));
    fx.run("layer.trim", json!({"layers": [a], "out_point": 5 * SEC}));
    let span_a = query::layer_timing(&fx.stage, &a);
    assert_eq!(
        (span_a.in_point, span_a.out_point),
        (Time(2 * SEC), Time(5 * SEC))
    );
    let out = fx.run(
        "layer.precompose",
        json!({"layers": [a, b], "adjust_duration": true, "name": "Part"}),
    );
    let inner = p(out["comp"].as_str().unwrap());
    let pre = p(out["layer"].as_str().unwrap());
    // From the earliest in point (2 s) to the latest out point (10 s).
    let info = query::comp_info(&fx.stage, &inner).unwrap();
    assert_eq!(info.duration, Time(8 * SEC));
    let moved = query::comp_layers(&fx.stage, &inner);
    let t = query::layer_timing(&fx.stage, &moved[1]);
    assert_eq!((t.in_point, t.out_point), (Time::ZERO, Time(3 * SEC)));
    let t = query::layer_timing(&fx.stage, &pre);
    assert_eq!(
        (t.start_time, t.in_point, t.out_point),
        (Time(2 * SEC), Time(2 * SEC), Time(10 * SEC))
    );
}

#[test]
fn precompose_leaving_the_attributes_on_the_layer() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(
        &comp,
        "solid",
        json!({"name": "Red Solid 1", "size": [400, 300]}),
    );
    let item = bae_motion::solid::layer_solid(&fx.stage, &solid).unwrap();
    fx.run(
        "attr.set_at",
        json!({"path": solid, "attr": "transform:position", "time": 0, "value": {"type": "vec3", "value": [100.0, 200.0, 0.0]}}),
    );
    assert_eq!(
        bae_motion::precompose::default_precomp_name(&fx.stage, std::slice::from_ref(&solid)),
        "Red Solid 1 Comp 1"
    );
    let out = fx.run(
        "layer.precompose",
        json!({"layers": [solid], "mode": "leave"}),
    );
    let inner = p(out["comp"].as_str().unwrap());
    assert_eq!(p(out["layer"].as_str().unwrap()), solid);
    // The new comp has the solid's size and holds the solid.
    let info = query::comp_info(&fx.stage, &inner).unwrap();
    assert_eq!((info.width, info.height), (400, 300));
    let inside = query::comp_layers(&fx.stage, &inner);
    assert_eq!(inside.len(), 1);
    // The inner layer starts at 0 (the layer's timing stays outside).
    assert_eq!(
        query::layer_timing(&fx.stage, &inside[0]).start_time,
        Time::ZERO
    );
    assert_eq!(fx.value(&inside[0], "source"), Some(Value::PrimRef(item)));
    // The layer keeps its place and attributes and now shows the new comp.
    assert_eq!(fx.value(&solid, "source"), Some(Value::PrimRef(inner)));
    assert_eq!(
        fx.value(&solid, "kind"),
        Some(Value::Enum("pre_comp".into()))
    );
    assert_eq!(
        fx.value(&solid, "transform:position"),
        Some(Value::Vec3([100.0, 200.0, 0.0]))
    );
    assert_eq!(
        query::display_name_of(&fx.stage, &solid),
        "Red Solid 1 Comp 1"
    );
    // Text and several layers cannot leave their attributes.
    let text = fx.layer(&comp, "text", json!({"text": "T"}));
    assert!(
        fx.try_run(
            "layer.precompose",
            json!({"layers": [text], "mode": "leave"})
        )
        .is_err()
    );
    assert!(
        fx.try_run(
            "layer.precompose",
            json!({"layers": [text, solid], "mode": "leave"})
        )
        .is_err()
    );
    assert!(
        fx.try_run("layer.precompose", json!({"layers": []}))
            .is_err()
    );
}

#[test]
fn import_creates_footage_items_in_one_step() {
    let dir = std::env::temp_dir().join(format!("bae-import-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("plates")).unwrap();
    let still = dir.join("logo.png");
    let frame = dir.join("plates").join("shot_0001.png");
    std::fs::write(&still, b"not really a png").unwrap();
    std::fs::write(&frame, b"frame").unwrap();
    let mut fx = Fx::new();
    let out = fx.run(
        "item.import",
        json!({"files": [
            {"path": still, "attrs": {
                "width": {"type": "int", "value": 640}, "height": {"type": "int", "value": 360}}},
            {"path": frame, "name": "shot_[0001-0024].png", "folder": "plates", "attrs": {
                "kind": {"type": "enum", "value": "sequence"},
                "sequence:first_frame": {"type": "int", "value": 1}}},
        ]}),
    );
    let items: Vec<PrimPath> = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| p(v.as_str().unwrap()))
        .collect();
    assert_eq!(items[0].parent().unwrap(), p(query::ASSETS_FOLDER));
    assert_eq!(query::display_name_of(&fx.stage, &items[0]), "logo.png");
    assert_eq!(
        fx.value(&items[0], "kind"),
        Some(Value::Enum("footage".into()))
    );
    assert_eq!(fx.value(&items[0], "width"), Some(Value::Int(640)));
    let original = fx.value(&items[0], "file:original").unwrap();
    assert!(
        original.as_str().unwrap().ends_with("logo.png"),
        "{original:?}"
    );
    assert_eq!(fx.value(&items[0], "file:size"), Some(Value::Int(16)));
    // A dropped folder keeps its structure.
    let folder = items[1].parent().unwrap();
    assert_eq!(query::display_name_of(&fx.stage, &folder), "plates");
    assert_eq!(folder.parent().unwrap(), p(query::ASSETS_FOLDER));
    assert_eq!(
        fx.value(&items[1], "kind"),
        Some(Value::Enum("sequence".into()))
    );
    assert_eq!(
        query::display_name_of(&fx.stage, &items[1]),
        "shot_[0001-0024].png"
    );
    // One undo step removes both (and the new folder).
    fx.stage.undo(&fx.author).unwrap();
    assert!(!fx.stage.has_prim(&items[0]) && !fx.stage.has_prim(&folder));
    // Missing files, unknown attributes and empty lists are refused.
    assert!(
        fx.try_run(
            "item.import",
            json!({"files": [{"path": dir.join("nope.png")}]})
        )
        .is_err()
    );
    assert!(
        fx.try_run(
            "item.import",
            json!({"files": [{"path": still, "attrs": {"name": {"type": "string", "value": "x"}}}]})
        )
        .is_err()
    );
    assert!(fx.try_run("item.import", json!({"files": []})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `layer.set_attr`: one undo step for several layers; only switches, quality
/// and label.
#[test]
fn set_layer_attr_sets_switches_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let b = fx.layer(&comp, "null", json!({}));
    let shy = |fx: &Fx, l: &PrimPath| {
        fx.stage
            .prim(l)
            .and_then(|v| query::static_value(&v, "shy"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    fx.run(
        "layer.set_attr",
        json!({"layers": [a, b], "attr": "shy", "value": {"type": "bool", "value": true}}),
    );
    assert!(shy(&fx, &a) && shy(&fx, &b));
    assert_eq!(fx.stage.undo_label(&fx.author), Some("Change Layers"));
    fx.stage.undo(&fx.author).unwrap();
    assert!(!shy(&fx, &a) && !shy(&fx, &b), "one undo step");
    fx.run(
        "layer.set_attr",
        json!({"layers": [a], "attr": "label", "value": {"type": "int", "value": 9}}),
    );
    let label = fx
        .stage
        .prim(&a)
        .and_then(|v| query::static_value(&v, "label"))
        .and_then(|v| v.as_f64());
    assert_eq!(label, Some(9.0));
    assert!(
        fx.try_run(
            "layer.set_attr",
            json!({"layers": [a], "attr": "name", "value": {"type": "string", "value": "x"}}),
        )
        .is_err()
    );
}

/// `layer.split` puts the new layer above the original (AE's default) or,
/// with `above: false` (the preference off), below it.
#[test]
fn split_layer_above_or_below() {
    for above in [true, false] {
        let mut fx = Fx::new();
        let comp = fx.comp();
        let a = fx.layer(&comp, "solid", json!({}));
        let out = fx.run(
            "layer.split",
            json!({"layers": [a], "time": SEC, "above": above}),
        );
        let new = p(out["paths"][0].as_str().unwrap());
        let layers = query::comp_layers(&fx.stage, &comp);
        let (i_new, i_old) = (
            layers.iter().position(|l| *l == new).unwrap(),
            layers.iter().position(|l| *l == a).unwrap(),
        );
        assert_eq!(i_new < i_old, above, "above = {above}: {layers:?}");
        assert_eq!(query::layer_timing(&fx.stage, &new).in_point, Time(SEC));
        assert_eq!(query::layer_timing(&fx.stage, &a).out_point, Time(SEC));
    }
}

/// `layer.paste` (Edit > Paste): copies of the copied layers with their keyframes,
/// above a layer or on top, named like Duplicate, into the same or another comp,
/// in one undo step; the copies work after the originals are deleted (Cut).
#[test]
fn paste_layers_same_and_other_comp() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let b = fx.layer(&comp, "null", json!({}));
    fx.run(
        "attr.set_at",
        json!({"path": a, "attr": "transform:opacity", "time": 0, "value": {"type": "float", "value": 50.0}}),
    );
    fx.run(
        "attr.set_animated",
        json!({"path": a, "attr": "transform:opacity", "animated": true, "time": 0}),
    );
    let copied = bae_motion::copy_layers(&fx.stage, std::slice::from_ref(&a));
    // Same comp, above the null (top): Solid 1 → Solid 2.
    let out = fx.run(
        "layer.paste",
        json!({"comp": comp, "layers": copied, "above": b}),
    );
    let pasted = p(out["paths"][0].as_str().unwrap());
    let layers = query::comp_layers(&fx.stage, &comp);
    assert_eq!(layers[0], pasted, "{layers:?}");
    assert_eq!(query::display_name_of(&fx.stage, &pasted), "Solid 2");
    assert_eq!(
        query::attr_spec(&fx.stage, &pasted, "transform:opacity").map(|s| s.keyframes.len()),
        query::attr_spec(&fx.stage, &a, "transform:opacity").map(|s| s.keyframes.len())
    );
    assert_eq!(fx.stage.undo_label(&fx.author), Some("Paste"));
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        query::comp_layers(&fx.stage, &comp).len(),
        2,
        "one undo step"
    );
    // Cut: the original is gone, the clipboard still pastes, into another comp.
    fx.run("layer.delete", json!({"layers": [a]}));
    let other = fx.comp();
    let out = fx.run("layer.paste", json!({"comp": other, "layers": copied}));
    let pasted = p(out["paths"][0].as_str().unwrap());
    assert_eq!(query::comp_layers(&fx.stage, &other), vec![pasted.clone()]);
    assert_eq!(
        query::layer_kind(&fx.stage, &pasted).as_deref(),
        Some("solid")
    );
}

#[test]
fn markers_add_edit_move_delete() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let solid = fx.layer(&comp, "solid", json!({}));
    // A comp marker and a layer marker with a comment and a duration.
    let m1 = p(
        fx.run("marker.add", json!({"owner": comp, "time": SEC}))["path"]
            .as_str()
            .unwrap(),
    );
    let m2 = p(fx.run(
        "marker.add",
        json!({"owner": solid, "time": 2 * SEC, "comment": "hit", "duration": SEC}),
    )["path"]
        .as_str()
        .unwrap());
    assert_ne!(m1, m2);
    assert_eq!(
        bae_motion::markers::markers_of(&fx.stage, &comp),
        vec![(m1.clone(), Time(SEC))]
    );
    assert_eq!(fx.value(&m2, "comment"), Some(Value::String("hit".into())));
    assert_eq!(fx.value(&m2, "duration"), Some(Value::Time(Time(SEC))));
    // A second comp marker gets its own name; markers_of sorts by time.
    let m3 = p(
        fx.run("marker.add", json!({"owner": comp, "time": 0}))["path"]
            .as_str()
            .unwrap(),
    );
    let comp_markers: Vec<PrimPath> = bae_motion::markers::markers_of(&fx.stage, &comp)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert_eq!(comp_markers, vec![m3.clone(), m1.clone()]);
    // Edit (the Marker dialog), move (drag), delete; each one undo step.
    fx.run(
        "marker.edit",
        json!({"path": m1, "comment": "intro", "label": 3, "time": 3 * SEC}),
    );
    assert_eq!(
        fx.value(&m1, "comment"),
        Some(Value::String("intro".into()))
    );
    assert_eq!(fx.value(&m1, "label"), Some(Value::Int(3)));
    fx.run("marker.move", json!({"markers": [m1, m3], "delta": SEC}));
    assert_eq!(
        bae_motion::markers::marker_time(&fx.stage, &m1),
        Time(4 * SEC)
    );
    assert_eq!(bae_motion::markers::marker_time(&fx.stage, &m3), Time(SEC));
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        bae_motion::markers::marker_time(&fx.stage, &m1),
        Time(3 * SEC)
    );
    fx.run("marker.delete", json!({"markers": [m2]}));
    assert!(fx.stage.prim(&m2).is_none());
    fx.stage.undo(&fx.author).unwrap();
    assert!(fx.stage.prim(&m2).is_some());
    // Not a marker; not a comp or layer.
    assert!(
        fx.try_run("marker.delete", json!({"markers": [solid]}))
            .is_err()
    );
    assert!(
        fx.try_run("marker.add", json!({"owner": m1, "time": 0}))
            .is_err()
    );
    // A locked layer's markers cannot be added or changed (AE).
    fx.run(
        "attr.set",
        json!({"path": solid, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    assert!(
        fx.try_run("marker.add", json!({"owner": solid, "time": 0}))
            .unwrap_err()
            .contains("locked")
    );
    assert!(
        fx.try_run("marker.move", json!({"markers": [m2], "delta": SEC}))
            .is_err()
    );
}

#[test]
fn slip_edit_moves_the_source_inside_fixed_in_and_out_points() {
    let mut fx = Fx::new();
    let main = fx.comp();
    // A 10 s pre-comp in the main comp, trimmed to 2 s .. 5 s.
    let inner = fx.comp();
    let layer = fx.layer(&main, "pre_comp", json!({"source": inner}));
    assert_eq!(
        query::source_duration(&fx.stage, &layer),
        Some(Time(10 * SEC))
    );
    fx.run(
        "layer.trim",
        json!({"layers": [layer], "in_point": 2 * SEC, "out_point": 5 * SEC}),
    );
    // Two keyframes: the one at layer time 3 s is selected and moves with the
    // source, the other keeps its comp time.
    let attr = "transform:rotation";
    fx.run(
        "attr.set_animated",
        json!({"path": layer, "attr": attr, "animated": true, "time": 3 * SEC}),
    );
    fx.run("attr.set_at", json!({"path": layer, "attr": attr, "time": 4 * SEC, "value": {"type": "float", "value": 90.0}}));
    fx.run(
        "layer.slip",
        json!({"layers": [layer], "delta": -SEC, "keys": [key(&layer, attr, 3 * SEC)]}),
    );
    let t = query::layer_timing(&fx.stage, &layer);
    assert_eq!(t.start_time, Time(-SEC));
    assert_eq!((t.in_point, t.out_point), (Time(2 * SEC), Time(5 * SEC)));
    // Kept key: layer time 3 s (comp 2 s); the other: comp 4 s = layer time 5 s.
    assert_eq!(fx.keys(&layer, attr), vec![3 * SEC, 5 * SEC]);
    // Clamped: the source (comp -1 s .. 9 s) cannot start after In (2 s).
    fx.run("layer.slip", json!({"layers": [layer], "delta": 10 * SEC}));
    assert_eq!(
        query::layer_timing(&fx.stage, &layer).start_time,
        Time(2 * SEC)
    );
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        query::layer_timing(&fx.stage, &layer).start_time,
        Time(-SEC)
    );
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        query::layer_timing(&fx.stage, &layer).start_time,
        Time::ZERO
    );
    assert_eq!(fx.keys(&layer, attr), vec![3 * SEC, 4 * SEC]);
    // Layers without a time-based source cannot slip.
    let solid = fx.layer(&main, "solid", json!({}));
    assert!(
        fx.try_run("layer.slip", json!({"layers": [solid], "delta": SEC}))
            .is_err()
    );
}

#[test]
fn set_expressions_edits_several_properties_in_one_step() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let b = fx.layer(&comp, "solid", json!({}));
    let expr = |fx: &Fx, l: &PrimPath, attr: &str| {
        query::attr_spec(&fx.stage, l, attr).and_then(|s| s.expression.clone())
    };
    fx.run(
        "attr.set_expressions",
        json!({"items": [
            {"path": a, "attr": "transform:position", "source": "transform.position"},
            {"path": b, "attr": "transform:opacity", "source": "50", "enabled": false},
        ]}),
    );
    assert_eq!(
        expr(&fx, &a, "transform:position").map(|e| (e.source, e.enabled)),
        Some(("transform.position".to_owned(), true))
    );
    assert_eq!(
        expr(&fx, &b, "transform:opacity").map(|e| (e.source, e.enabled)),
        Some(("50".to_owned(), false))
    );
    // Removing both is one step; undo brings both back.
    fx.run(
        "attr.set_expressions",
        json!({"items": [
            {"path": a, "attr": "transform:position", "source": null},
            {"path": b, "attr": "transform:opacity", "source": null},
        ]}),
    );
    assert!(expr(&fx, &a, "transform:position").is_none());
    assert!(expr(&fx, &b, "transform:opacity").is_none());
    fx.stage.undo(&fx.author).unwrap();
    assert!(expr(&fx, &a, "transform:position").is_some());
    assert!(expr(&fx, &b, "transform:opacity").is_some());
    // A locked layer is left alone (nothing changes).
    fx.run(
        "attr.set",
        json!({"path": b, "attr": "locked", "value": {"type": "bool", "value": true}}),
    );
    assert!(
        fx.try_run(
            "attr.set_expressions",
            json!({"items": [{"path": b, "attr": "transform:opacity", "source": null}]}),
        )
        .is_err()
    );
}

#[test]
fn text_animators_animate_add_selector_and_remove() {
    use bae_motion::text_animators::{animators_of, selectors_of};
    let mut fx = Fx::new();
    let comp = fx.comp();
    let text = fx.layer(&comp, "text", json!({"text": "Hello"}));
    // Animate Text > Position: a new animator with a Range Selector and Position.
    let out = fx.run(
        "text.animate",
        json!({"layers": [text], "property": "position"}),
    );
    let a1 = p(out["animators"][0].as_str().unwrap());
    assert_eq!(animators_of(&fx.stage, &text), vec![a1.clone()]);
    assert_eq!(selectors_of(&fx.stage, &a1).len(), 1);
    assert!(query::attr_spec(&fx.stage, &a1, "animator:position").is_some());
    assert!(
        query::attr_spec(&fx.stage, &a1, "animator:scale").is_none(),
        "only what was added"
    );
    // Again: a second animator. Fill Color comes in red (AE).
    fx.run(
        "text.animate",
        json!({"layers": [text], "property": "fill_rgb"}),
    );
    let all = animators_of(&fx.stage, &text);
    assert_eq!(all.len(), 2);
    assert_eq!(
        fx.value(&all[1], "animator:fill_color"),
        Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    // The first animator's Add menu: Scale; Add > Selector > Range.
    fx.run("text.animate", json!({"animator": a1, "property": "scale"}));
    assert!(query::attr_spec(&fx.stage, &a1, "animator:scale").is_some());
    fx.run("text.add_selector", json!({"animator": a1}));
    assert_eq!(selectors_of(&fx.stage, &a1).len(), 2);
    assert!(
        fx.try_run(
            "text.add_selector",
            json!({"animator": a1, "selector": "wiggly"})
        )
        .is_err()
    );
    // Remove All Text Animators; each command is one undo step.
    fx.run("text.remove_animators", json!({"layers": [text]}));
    assert!(animators_of(&fx.stage, &text).is_empty());
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(animators_of(&fx.stage, &text).len(), 2);
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(selectors_of(&fx.stage, &a1).len(), 1);
    // Not a text layer.
    let solid = fx.layer(&comp, "solid", json!({}));
    assert!(
        fx.try_run(
            "text.animate",
            json!({"layers": [solid], "property": "position"})
        )
        .is_err()
    );
}

#[test]
fn set_track_matte_sets_type_source_and_hides_the_matte() {
    let mut fx = Fx::new();
    let comp = fx.comp();
    let a = fx.layer(&comp, "solid", json!({}));
    let b = fx.layer(&comp, "solid", json!({}));
    fx.run(
        "layer.set_track_matte",
        json!({"layers": [a], "matte": "luma_inverted", "source": b}),
    );
    assert_eq!(
        fx.value(&a, "track_matte"),
        Some(Value::Enum("luma_inverted".into()))
    );
    assert_eq!(
        fx.value(&a, "track_matte_layer"),
        Some(Value::PrimRef(b.clone()))
    );
    assert_eq!(fx.value(&b, "enabled"), Some(Value::Bool(false)));
    // Type only: the source stays.
    fx.run(
        "layer.set_track_matte",
        json!({"layers": [a], "matte": "alpha"}),
    );
    assert_eq!(
        fx.value(&a, "track_matte_layer"),
        Some(Value::PrimRef(b.clone()))
    );
    fx.stage.undo(&fx.author).unwrap();
    assert_eq!(
        fx.value(&a, "track_matte"),
        Some(Value::Enum("luma_inverted".into()))
    );
    // Not itself, not another comp, not an unknown type.
    assert!(
        fx.try_run(
            "layer.set_track_matte",
            json!({"layers": [a], "matte": "alpha", "source": a})
        )
        .is_err()
    );
    let other = fx.comp();
    let c = fx.layer(&other, "solid", json!({}));
    assert!(
        fx.try_run(
            "layer.set_track_matte",
            json!({"layers": [a], "matte": "alpha", "source": c})
        )
        .is_err()
    );
    assert!(
        fx.try_run(
            "layer.set_track_matte",
            json!({"layers": [a], "matte": "bogus"})
        )
        .is_err()
    );
}

#[test]
fn interpret_footage_sets_alpha_rate_aspect_and_loop() {
    let dir = std::env::temp_dir().join(format!("bae-interpret-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("clip.mov");
    std::fs::write(&clip, b"clip").unwrap();
    let mut fx = Fx::new();
    let out = fx.run(
        "item.import",
        json!({"files": [{"path": clip, "attrs": {
            "frame_rate": {"type": "frame_rate", "value": [24, 1]}}}]}),
    );
    let item = p(out["items"][0].as_str().unwrap());
    fx.run(
        "footage.interpret",
        json!({"items": [item], "alpha_mode": "premultiplied", "frame_rate": 30.0,
               "pixel_aspect": 2.0, "loop_count": 3}),
    );
    assert_eq!(
        fx.value(&item, "alpha_mode"),
        Some(Value::Enum("premultiplied".into()))
    );
    assert_eq!(
        fx.value(&item, "frame_rate"),
        Some(Value::FrameRate(FrameRate::FPS_30))
    );
    assert_eq!(fx.value(&item, "pixel_aspect"), Some(Value::Float(2.0)));
    assert_eq!(fx.value(&item, "loop"), Some(Value::Int(3)));
    // Back to the file's rate.
    fx.run(
        "footage.interpret",
        json!({"items": [item], "use_file_frame_rate": true}),
    );
    assert_eq!(
        fx.value(&item, "frame_rate"),
        Some(Value::FrameRate(FrameRate::FPS_24))
    );
    let _ = std::fs::remove_dir_all(dir);
}

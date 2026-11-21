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
    assert_eq!(reg.len(), bae_motion::commands().len());
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
    assert_eq!(
        fx.value(&parent, "transform:opacity"),
        Some(Value::Float(100.0))
    );
    assert_eq!(
        fx.value(&child, "transform:opacity"),
        Some(Value::Float(100.0))
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

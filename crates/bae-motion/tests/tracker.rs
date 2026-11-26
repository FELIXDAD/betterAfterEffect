use bae_model::{AuthorId, CommandRegistry, PrimPath, Stage, Time, Value};
use serde_json::{Value as Json, json};

#[test]
fn corner_pin_keeps_the_layer_of_the_chosen_effect_point() {
    let mut registry = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut registry);
    let author = AuthorId::new("user:tracker").unwrap();
    let mut stage = Stage::new();
    let run = |s: &mut Stage, id, p| registry.execute(s, &author, id, p).unwrap().output;
    let comp = run(
        &mut stage,
        "comp.create",
        json!({"width":128,"height":128,"fps":30}),
    )["path"]
        .clone();
    let source = run(
        &mut stage,
        "layer.create",
        json!({"comp":comp,"kind":"solid"}),
    )["path"]
        .clone();
    let dest = run(
        &mut stage,
        "layer.create",
        json!({"comp":comp,"kind":"solid"}),
    )["path"]
        .clone();
    let dest = PrimPath::new(dest.as_str().unwrap()).unwrap();
    let folder = dest.child(bae_motion::effect::EFFECTS_FOLDER).unwrap();
    let effect = folder.child("Point_Control").unwrap();
    run(
        &mut stage,
        "prim.create",
        json!({"path":folder,"prim_type":"Folder"}),
    );
    run(
        &mut stage,
        "prim.create",
        json!({"path":effect,"prim_type":"Effect"}),
    );
    let tracker = run(
        &mut stage,
        "tracker.create",
        json!({"layer":source,"center":[12,12]}),
    );
    let tracker = PrimPath::new(tracker.as_str().unwrap()).unwrap();
    let point = bae_model::effect::param_attr(1);
    run(
        &mut stage,
        "tracker.configure",
        json!({"tracker":tracker,"values":{
            "target": Value::PrimRef(effect.clone()), "target_property": Value::String(point.clone())
        }}),
    );
    let target = |s: &Stage| {
        let get = |key| bae_motion::tracker::setting(s, &tracker, key).unwrap();
        (get("target"), get("target_property"))
    };
    for mode in ["parallel", "perspective"] {
        run(
            &mut stage,
            "tracker.configure",
            json!({"tracker":tracker,"values":{"track_type":Value::Enum(mode.into())}}),
        );
        assert_eq!(
            target(&stage),
            (Value::PrimRef(dest.clone()), Value::String(String::new()))
        );
        stage.undo(&author).unwrap();
        assert_eq!(
            target(&stage),
            (Value::PrimRef(effect.clone()), Value::String(point.clone()))
        );
    }
}

#[test]
fn stabilize_records_applies_and_undoes_one_operation_at_a_time() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    bae_motion::effect::register_effect_commands(&mut reg, Default::default());
    let author = AuthorId::new("user:tracker").unwrap();
    let mut stage = Stage::new();
    let run =
        |stage: &mut Stage, id: &str, p: Json| reg.execute(stage, &author, id, p).unwrap().output;
    let comp = run(
        &mut stage,
        "comp.create",
        json!({"width":128,"height":128,"fps":30}),
    )["path"]
        .clone();
    let layer = run(
        &mut stage,
        "layer.create",
        json!({"comp":comp,"kind":"solid"}),
    )["path"]
        .clone();
    let tracker = run(
        &mut stage,
        "tracker.create",
        json!({"layer":layer,"center":[32,32],"stabilize":true}),
    );
    let tp = PrimPath::new(tracker.as_str().unwrap()).unwrap();
    let pt = bae_motion::tracker::points(&stage, &tp).remove(0);
    let sample =
        |t, x| json!({"time":t,"points":[{"center":[x,32],"attach":[x,32],"confidence":95}]});
    run(
        &mut stage,
        "tracker.record",
        json!({"tracker":tracker,"frames":[sample(0,32),sample(100,38)]}),
    );
    assert_eq!(
        bae_motion::tracker::vector_at(&stage, &pt, "feature_center", Time(100)),
        Some([38., 32.])
    );
    run(
        &mut stage,
        "tracker.apply",
        json!({"tracker":tracker,"axes":"x"}),
    );
    let lp = PrimPath::new(layer.as_str().unwrap()).unwrap();
    let key = |s: &Stage, t| {
        s.prim(&lp)
            .unwrap()
            .attr("transform:anchor_point")
            .unwrap()
            .keyframes
            .get(&Time(t))
            .map(|k| k.value.clone())
    };
    let (Some(Value::Vec3(a)), Some(Value::Vec3(b))) = (key(&stage, 0), key(&stage, 100)) else {
        panic!("missing anchor animation")
    };
    assert_eq!(b, [a[0] + 6., a[1], a[2]]);
    stage.undo(&author);
    assert_eq!(key(&stage, 100), None);
    run(&mut stage, "tracker.reset", json!({"tracker":tracker}));
    assert!(
        stage
            .prim(&pt)
            .unwrap()
            .attr("confidence")
            .unwrap()
            .keyframes
            .is_empty()
    );
    stage.undo(&author);
    assert_eq!(
        stage
            .prim(&pt)
            .unwrap()
            .attr("confidence")
            .unwrap()
            .keyframes
            .len(),
        2
    );
}

#[test]
fn stabilization_keeps_both_attachment_points_fixed_in_comp_space() {
    let mut registry = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut registry);
    bae_motion::effect::register_effect_commands(&mut registry, Default::default());
    let author = AuthorId::new("user:tracker").unwrap();
    let mut stage = Stage::new();
    let mut run = |id: &str, params: Json| {
        registry
            .execute(&mut stage, &author, id, params)
            .unwrap()
            .output
    };
    let comp = run("comp.create", json!({"width":128,"height":128,"fps":30}))["path"].clone();
    let layer = run("layer.create", json!({"comp":comp,"kind":"solid"}))["path"].clone();
    let tracker = run(
        "tracker.create",
        json!({"layer":layer,"center":[10,10],"stabilize":true}),
    );
    run(
        "tracker.configure",
        json!({"tracker":tracker,"values":{"rotation":Value::Bool(true),"scale":Value::Bool(true)}}),
    );
    let points = [[[10., 10.], [20., 10.]], [[22., 16.], [22., 36.]]];
    let frames: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, pair)| {
            let samples: Vec<_> = pair
                .iter()
                .map(|p| json!({"center":p,"attach":p,"confidence":100}))
                .collect();
            json!({"time":100*i,"points":samples})
        })
        .collect();
    run("tracker.record", json!({"tracker":tracker,"frames":frames}));
    run("tracker.apply", json!({"tracker":tracker}));
    let comp = PrimPath::new(comp.as_str().unwrap()).unwrap();
    let layer = PrimPath::new(layer.as_str().unwrap()).unwrap();
    let in_comp = |i: usize| {
        let frame = bae_anim::evaluate_comp(&stage, &comp, Time(100 * i as i64)).unwrap();
        let matrix = frame.layer(&layer).unwrap().world_matrix;
        points[i].map(|p| matrix.transform_point3(bae_anim::glam::DVec2::from(p).extend(0.0)))
    };
    for (before, after) in in_comp(0).into_iter().zip(in_comp(1)) {
        assert!((after - before).length() < 1e-8, "{before} -> {after}");
    }
}

#[test]
fn property_tracking_adds_independent_points_and_preserves_active_transform_points() {
    let mut reg = CommandRegistry::with_builtins();
    bae_motion::register_commands(&mut reg);
    let author = AuthorId::new("user:tracker").unwrap();
    let mut stage = Stage::new();
    let run = |s: &mut Stage, id: &str, p: Json| reg.execute(s, &author, id, p).unwrap().output;
    let comp = run(
        &mut stage,
        "comp.create",
        json!({"width":128,"height":128,"fps":30}),
    )["path"]
        .clone();
    let src = run(
        &mut stage,
        "layer.create",
        json!({"comp":comp,"kind":"solid"}),
    )["path"]
        .clone();
    let target = run(
        &mut stage,
        "layer.create",
        json!({"comp":comp,"kind":"solid"}),
    )["path"]
        .clone();
    let track = run(
        &mut stage,
        "tracker.create",
        json!({"layer":src,"center":[32,32],"property_target":[target,"transform:rotation"]}),
    );
    let path = PrimPath::new(track.as_str().unwrap()).unwrap();
    let points = bae_motion::tracker::points(&stage, &path);
    assert_eq!(points.len(), 2);
    assert_eq!(
        bae_motion::tracker::setting(&stage, &path, "position"),
        Some(Value::Bool(false))
    );
    let extra = run(&mut stage, "tracker.add_point", json!({"tracker":track}));
    let all = bae_motion::tracker::points(&stage, &path);
    assert_eq!(&all[..2], points.as_slice());
    assert_eq!(all[2].as_str(), extra.as_str().unwrap());
    stage.undo(&author).unwrap();
    assert_eq!(bae_motion::tracker::points(&stage, &path), points);
}

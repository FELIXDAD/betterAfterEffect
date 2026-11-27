//! Expressions through the public API.

use bae_anim::{Evaluator, LayerSource, evaluate_attribute, evaluate_comp};
use bae_model::{
    AttrSpec, AuthorId, Expression, Keyframe, Op, PrimPath, PrimType, Stage, TextDocument, Time,
    Transaction, Value,
};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn secs(s: f64) -> Time {
    Time::from_seconds(s)
}

struct B<'a, 'b>(&'a mut Transaction<'b>);

impl B<'_, '_> {
    fn prim(&mut self, path: &str, t: PrimType) -> &mut Self {
        self.0
            .apply(Op::CreatePrim {
                path: p(path),
                specifier: Default::default(),
                prim_type: Some(t),
                index: None,
            })
            .unwrap();
        self
    }
    fn spec(&mut self, path: &str, attr: &str, spec: AttrSpec) -> &mut Self {
        self.0
            .apply(Op::SetAttr {
                path: p(path),
                attr: attr.into(),
                spec: Some(spec),
            })
            .unwrap();
        self
    }
    fn set(&mut self, path: &str, attr: &str, v: impl Into<Value>) -> &mut Self {
        self.spec(path, attr, AttrSpec::with_value(v.into()))
    }
    /// Static value plus expression.
    fn expr(&mut self, path: &str, attr: &str, v: impl Into<Value>, src: &str) -> &mut Self {
        self.spec(
            path,
            attr,
            AttrSpec {
                value: Some(v.into()),
                expression: Some(Expression::new(src)),
                ..Default::default()
            },
        )
    }
    fn layer(&mut self, comp: &str, name: &str, kind: &str) -> String {
        let path = format!("{comp}/layers/{name}");
        self.prim(&path, PrimType::LAYER)
            .set(&path, "kind", Value::Enum(kind.into()))
            .set(&path, "name", name);
        path
    }
    fn comp(&mut self, path: &str, name: &str) -> &mut Self {
        self.prim(path, PrimType::COMP)
            .set(path, "name", name)
            .set(path, "duration", Value::Time(secs(10.0)))
            .prim(&format!("{path}/layers"), PrimType::FOLDER)
    }
}

fn stage(f: impl FnOnce(&mut B<'_, '_>)) -> Stage {
    let mut stage = Stage::new();
    stage
        .transact(&AuthorId::new("user:test").unwrap(), "build", |tx| {
            let mut b = B(tx);
            b.prim("/comps", PrimType::FOLDER)
                .comp("/comps/main", "Main");
            f(&mut b);
            Ok::<_, bae_model::ModelError>(())
        })
        .unwrap();
    stage
}

const MAIN: &str = "/comps/main";

fn eval(s: &Stage, t: f64) -> bae_anim::EvaluatedComp {
    evaluate_comp(s, &p(MAIN), secs(t)).unwrap()
}

#[test]
fn localized_pick_whip_lookups_read_the_same_property() {
    let s = stage(|b| {
        let target = b.layer(MAIN, "target", "null");
        b.set(&target, "transform:rotation", 31.0);
        let source = b.layer(MAIN, "source", "null");
        b.expr(&source, "transform:rotation", 0.0,
            r#"var peer = thisComp.layer("target");
               peer("Transform")("Rotation") + peer("变换")("旋转") + (peer.name === "target" ? 1 : 0)"#);
    });
    let comp = eval(&s, 0.0);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    assert!(comp.layers.iter().any(|l| l.transform.rotation.z == 63.0));
}

#[test]
fn vector_and_interpolation_examples() {
    let samples = [
        ("add([4,5],[1,2,3])[2]", 3.0),
        ("sub(5,[2,3])[1]", -3.0),
        ("mul([2,3],4)[1]", 12.0),
        ("div([2,3],4)[1]", 0.75),
        ("clamp([1,2],[0],[3,4])[1]", 2.0),
        ("dot([1,2],[3,4,5])", 11.0),
        ("cross([1,2],[3,4])[0]", 0.0),
        ("length([1,2],[4,6])", 5.0),
        ("normalize([3,4])[0]", 0.6),
        ("lookAt([0,0,0],[1,1,1])[2]", 30.0),
        ("linear(0.4,1,0,10,20)", 14.0),
        ("ease(.25,0,1)", 0.15625),
        ("easeIn(.25,0,1)", 0.109375),
        ("easeOut(.25,0,1)", 0.296875),
    ];
    let s = stage(|b| {
        for (i, (source, _)) in samples.iter().enumerate() {
            let layer = b.layer(MAIN, &format!("case{i}"), "null");
            b.expr(&layer, "transform:rotation", 0.0, source);
        }
    });
    let comp = eval(&s, 0.0);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    for (layer, (source, want)) in comp.layers.iter().zip(samples) {
        assert!(
            (layer.transform.rotation.z - want).abs() < 1e-8,
            "{source}: {}",
            layer.transform.rotation.z
        );
    }
}

#[test]
fn vector_arithmetic_on_2d_position() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(
            &l,
            "transform:position",
            [100.0, 200.0, 0.0],
            "value + [10, 20]",
        )
        .expr(&l, "transform:rotation", 0.0, "time * 90")
        .expr(&l, "transform:scale", [100.0, 100.0, 100.0], "value * 2");
    });
    let ec = eval(&s, 0.5);
    let t = &ec.layers[0].transform;
    assert_eq!(t.position.to_array(), [110.0, 220.0, 0.0]);
    assert_eq!(t.rotation.z, 45.0);
    // 2D layer: scale is [x, y] in expressions; z keeps its value.
    assert_eq!(t.scale.to_array(), [200.0, 200.0, 100.0]);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
}

#[test]
fn cross_layer_reads_and_dependencies() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "leader", "null");
        let f = b.layer(MAIN, "follower", "null");
        b.expr(&a, "transform:rotation", 0.0, "time * 10")
            .expr(
                &f,
                "transform:rotation",
                0.0,
                r#"thisComp.layer("leader").transform.rotation * 2"#,
            )
            .expr(
                &f,
                "transform:opacity",
                100.0,
                "thisComp.layer(1).rotation.value + index",
            );
    });
    let ec = eval(&s, 2.0);
    assert_eq!(ec.layers[1].transform.rotation.z, 40.0);
    assert_eq!(ec.layers[1].transform.opacity, 22.0);
    assert_eq!(
        ec.layers[1].depends_on,
        vec![p("/comps/main/layers/leader")]
    );
    assert!(ec.layers[0].depends_on.is_empty());
}

#[test]
fn errors_fall_back_to_the_pre_expression_value() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(&l, "transform:rotation", 15.0, "value +")
            .expr(
                &l,
                "transform:opacity",
                50.0,
                "thisComp.layer(\"nope\").opacity",
            )
            .expr(
                &l,
                "transform:position",
                [1.0, 2.0, 0.0],
                "[value[0], value[1], value[2] + 1]",
            );
    });
    let ec = eval(&s, 0.0);
    let t = &ec.layers[0].transform;
    assert_eq!(t.rotation.z, 15.0);
    assert_eq!(t.opacity, 50.0);
    assert_eq!(t.position.to_array(), [1.0, 2.0, 0.0]);
    assert_eq!(ec.errors.len(), 3, "{:#?}", ec.errors);
    let msg = |attr: &str| {
        ec.errors
            .iter()
            .find(|e| e.attr == attr)
            .map(|e| e.message.clone())
            .unwrap()
    };
    assert!(
        msg("transform:rotation").contains("SyntaxError"),
        "{}",
        msg("transform:rotation")
    );
    assert!(
        msg("transform:opacity").contains("does not exist"),
        "{}",
        msg("transform:opacity")
    );
    // AE's message for value[2] on a 2D position.
    assert!(
        msg("transform:position").contains("Undefined value"),
        "{}",
        msg("transform:position")
    );
}

#[test]
fn runtime_error_reports_the_line() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(&l, "transform:rotation", 0.0, "var x = 1;\nx = y + 1;\nx");
    });
    let ec = eval(&s, 0.0);
    assert!(
        ec.errors[0].message.contains("line 2"),
        "{}",
        ec.errors[0].message
    );
}

#[test]
fn cycles_terminate() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "a", "null");
        let c = b.layer(MAIN, "c", "null");
        b.expr(
            &a,
            "transform:rotation",
            1.0,
            r#"thisComp.layer("c").rotation + 1"#,
        )
        .expr(
            &c,
            "transform:rotation",
            2.0,
            r#"thisComp.layer("a").rotation + 1"#,
        );
    });
    let ec = eval(&s, 0.0);
    // a = (c's expression reading a's pre value 1) + 1 = 3; c reads a's post value.
    assert_eq!(ec.layers[0].transform.rotation.z, 3.0);
    assert!(ec.layers[1].transform.rotation.z.is_finite());
}

#[test]
fn keys_groups_and_loop_segments_use_composition_time() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "Source", "null");
        b.set(&l, "out_point", Value::Time(secs(5.0)));
        let mut pos = AttrSpec::default();
        for (t, x) in [(1.0, 0.0), (3.0, 100.0)] {
            pos.keyframes
                .insert(secs(t), Keyframe::linear(Value::Vec3([x, 10.0, 0.0])));
        }
        b.spec(&l, "transform:position", pos);
        let text = b.layer(MAIN, "Info", "text");
        b.expr(
            &text,
            "text:source",
            doc(""),
            r#"var p=thisComp.layer("Source").position;
            [p.key(1).value.length,p.nearestKey(2.9).index,p.propertyIndex,
             propertyGroup(1).name,p.valueAtTime(2)[0],Math.round(p.velocityAtTime(2)[0]),
             Math.round(p.speedAtTime(2)),Math.round(p.smooth(1,3,2)[0]),
             p.loopInDuration()[0],p.loopOutDuration()[0]].join("|")"#,
        );
    });
    for (t, suffix) in [(0.5, "100|0"), (3.5, "100|0")] {
        let comp = eval(&s, t);
        assert!(comp.errors.is_empty(), "{:?}", comp.errors);
        assert_eq!(
            comp.layers[1].text.as_ref().unwrap().text,
            format!("2|2|2|Text|50|50|50|50|{suffix}")
        );
    }
}

#[test]
fn loops_cover_each_mode_in_both_directions() {
    let modes = [
        ("cycle", 75.0, 25.0),
        ("pingpong", 25.0, 75.0),
        ("offset", -25.0, 125.0),
        ("continue", -25.0, 125.0),
    ];
    for (mode, before, after) in modes {
        for (method, arg, time, expected) in [
            ("loopIn", 1, 0.5, before),
            ("loopInDuration", 2, 0.5, before),
            ("loopOut", 1, 3.5, after),
            ("loopOutDuration", 2, 3.5, after),
        ] {
            let s = stage(|b| {
                let l = b.layer(MAIN, "Animated", "null");
                let mut rotation = AttrSpec::default();
                rotation.keyframes.insert(secs(1.0), Keyframe::linear(0.0));
                rotation
                    .keyframes
                    .insert(secs(3.0), Keyframe::linear(100.0));
                rotation.expression = Some(Expression::new(format!("{method}('{mode}',{arg})")));
                b.spec(&l, "transform:rotation", rotation);
            });
            let comp = eval(&s, time);
            assert!(comp.errors.is_empty(), "{:?}", comp.errors);
            assert!(
                (comp.layers[0].transform.rotation.z - expected).abs() < 0.01,
                "{method} {mode}"
            );
        }
    }
}

#[test]
fn value_at_time_and_loop_out() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        let mut spec = AttrSpec::default();
        spec.keyframes.insert(secs(0.0), Keyframe::linear(0.0));
        spec.keyframes.insert(secs(1.0), Keyframe::linear(100.0));
        spec.expression = Some(Expression::new("loopOut('cycle')"));
        b.spec(&l, "transform:rotation", spec);
        let mut spec = AttrSpec::default();
        spec.keyframes.insert(secs(0.0), Keyframe::linear(0.0));
        spec.keyframes.insert(secs(1.0), Keyframe::linear(100.0));
        spec.expression = Some(Expression::new("valueAtTime(time - 0.25)"));
        b.spec(&l, "transform:opacity", spec);
    });
    let ec = eval(&s, 1.25);
    assert!((ec.layers[0].transform.rotation.z - 25.0).abs() < 1e-9);
    assert!((ec.layers[0].transform.opacity - 100.0).abs() < 1e-9);
    let ec = eval(&s, 0.5);
    assert!((ec.layers[0].transform.opacity - 25.0).abs() < 1e-9);
}

#[test]
fn wiggle_is_deterministic_and_bounded() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(
            &l,
            "transform:position",
            [500.0, 500.0, 0.0],
            "wiggle(3, 20)",
        );
    });
    let mut seen = Vec::new();
    for i in 0..30 {
        let a = eval(&s, i as f64 / 10.0).layers[0].transform.position;
        let b = eval(&s, i as f64 / 10.0).layers[0].transform.position;
        assert_eq!(a, b);
        assert!(
            (a.x - 500.0).abs() <= 40.0 && (a.y - 500.0).abs() <= 40.0,
            "{a}"
        );
        seen.push(a.x);
    }
    // It moves, and x and y differ.
    assert!(seen.iter().any(|x| (x - seen[0]).abs() > 1.0));
}

fn rotation_expression(source: &str, t: f64) -> f64 {
    let s = stage(|b| {
        let l = b.layer(MAIN, "sample", "null");
        b.expr(&l, "transform:rotation", 0.0, source);
    });
    let comp = eval(&s, t);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    comp.layers[0].transform.rotation.z
}

#[test]
fn seed_random_restarts_the_property_stream() {
    let src = "seedRandom(7, true); var x=random(); seedRandom(9); random(); seedRandom(7,true); random()-x";
    assert_eq!(rotation_expression(src, 1.25), 0.0);
    for timeless in [false, true] {
        let src = format!("seedRandom(7,{timeless});random()");
        assert_eq!(
            rotation_expression(&src, 0.0) == rotation_expression(&src, 1.0),
            timeless
        );
    }
    let s = stage(|b| {
        for name in ["a", "b"] {
            let layer = b.layer(MAIN, name, "null");
            b.expr(
                &layer,
                "transform:rotation",
                0.0,
                "seedRandom(7,true);random()",
            );
        }
    });
    let result = eval(&s, 1.0);
    assert!(result.errors.is_empty());
    assert_ne!(
        result.layers[0].transform.rotation.z,
        result.layers[1].transform.rotation.z
    );
}

#[test]
fn random_pads_vector_ranges_and_consumes_one_draw_per_component() {
    let src = "seedRandom(0,true);var v=random([1],[1,8]);seedRandom(0,true);random();var y=random(8); (v[0]===1 && v[1]===y) ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn uniform_draws_follow_the_measured_ae26_recurrence() {
    let src = "var a=Math.round(random()*4294967295);var expected=(Math.imul(a,1664525)+1013904223)>>>0; random()===expected/4294967295 ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn gauss_random_has_the_documented_tails() {
    let src = "var tails=0; for(var i=0;i<2048;i++){var v=gaussRandom([4],[4,1]); if(v[0]!==4) throw Error('range'); if(v[1]<0 || v[1]>1) tails++;} tails/2048";
    assert!((0.07..0.14).contains(&rotation_expression(src, 0.0)));
}

#[test]
fn noise_varies_across_the_plane_and_volume() {
    let src = "var p=noise([0.32,0.27]);var q=noise([0.32,0.27,0.15]); (p!==noise([0.32+31.7,0.27-1]) && q!==p && Math.abs(p-noise([0.32001,0.27]))<0.001 && Math.abs(q)<=1) ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn noise_accepts_one_component_and_color_arrays() {
    let src = "var x=.25; (noise([x])===noise(x) && noise([x,.2,.3,1])===noise([x,.2,.3])) ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn wiggle_uses_the_offset_independently_of_timeless() {
    let src = "seedRandom(2,true);var a=wiggle(2,4);seedRandom(2,false);var b=wiggle(2,4);seedRandom(3,true); (a===b && a!==wiggle(2,4)) ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn wiggle_takes_properties_for_octaves_and_amplitude_multiplier() {
    // wiggle(2, 10, effect("Oct")("Slider")): a property, not a number, reaches the host.
    let src = "var o=transform.opacity, m=transform.opacity; \
               (isFinite(wiggle(2,10,o,m/200)) && isFinite(temporalWiggle(2,0.1,o,m/200))) ? 1 : 0";
    assert_eq!(rotation_expression(src, 0.0), 1.0);
}

#[test]
fn temporal_wiggle_samples_an_animated_property_with_multiple_octaves() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "animated", "null");
        let mut spec = AttrSpec::default();
        spec.keyframes.insert(secs(0.0), Keyframe::linear(0.0));
        spec.keyframes.insert(secs(3.0), Keyframe::linear(300.0));
        spec.expression = Some(Expression::new(
            "temporalWiggle(2,0.25,3,0.5)-temporalWiggle(2,0.25,1)",
        ));
        b.spec(&l, "transform:rotation", spec);
    });
    let comp = eval(&s, 1.25);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    assert!(comp.layers[0].transform.rotation.z.abs() > 0.01);
}

#[test]
fn to_comp_uses_the_parent_chain() {
    let s = stage(|b| {
        let parent = b.layer(MAIN, "parent", "null");
        let child = b.layer(MAIN, "child", "null");
        let probe = b.layer(MAIN, "probe", "null");
        b.set(&parent, "transform:position", [100.0, 100.0, 0.0])
            .set(&parent, "transform:rotation", 90.0)
            .set(&child, "parent", Value::PrimRef(p(&parent)))
            .set(&child, "transform:position", [10.0, 0.0, 0.0])
            .expr(
                &probe,
                "transform:position",
                [0.0, 0.0, 0.0],
                r#"thisComp.layer("child").toComp([0, 0])"#,
            );
    });
    let ec = eval(&s, 0.0);
    let pos = ec.layers[2].transform.position;
    assert!(
        (pos.x - 100.0).abs() < 1e-9 && (pos.y - 110.0).abs() < 1e-9,
        "{pos}"
    );
}

#[test]
fn comp_by_name_and_source() {
    let s = stage(|b| {
        b.comp("/comps/other", "Other");
        let o = b.layer("/comps/other", "spin", "null");
        b.expr(&o, "transform:rotation", 0.0, "time * 30");
        let pre = b.layer(MAIN, "pre", "pre_comp");
        b.set(&pre, "source", Value::PrimRef(p("/comps/other")));
        let l = b.layer(MAIN, "reader", "null");
        b.expr(
            &l,
            "transform:rotation",
            0.0,
            r#"comp("Other").layer("spin").rotation + thisComp.layer("pre").source.numLayers"#,
        );
    });
    let ec = eval(&s, 1.0);
    assert_eq!(ec.layers[1].transform.rotation.z, 31.0);
    // The nested comp's own expressions run too.
    let LayerSource::Comp { nested, .. } = &ec.layers[0].source else {
        panic!()
    };
    assert_eq!(
        nested.as_ref().unwrap().layers[0].transform.rotation.z,
        30.0
    );
}

#[test]
fn time_conversions_follow_the_display_settings_and_rounding_mode() {
    let s = stage(|b| {
        b.prim("/project", PrimType::PROJECT).set(
            "/project",
            "time_display",
            Value::Enum("frames".into()),
        );
        b.set(MAIN, "display_start_time", Value::Time(secs(2.0)))
            .set(MAIN, "drop_frame", true);
        let l = b.layer(MAIN, "Clock", "text");
        b.expr(&l, "text:source", doc(""), r#"posterizeTime(0);
            [time, timeToFrames(undefined,24), framesToTime(6,24), timeToFrames(0.01,24,true),
             timeToTimecode(-0.01,24), timeToFeetAndFrames(-0.11,24,16,true), timeToNTSCTimecode(60.06,true),
             timeToCurrentFormat(0.11,24,true),thisComp.ntscDropFrame].join("|")"#);
    });
    let comp = eval(&s, 0.5);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    assert_eq!(
        comp.layers[0].text.as_ref().unwrap().text,
        "0|48|0.25|1|-00:00:00:01|-0000+03|00;01;00;02|3|true"
    );
}

#[test]
fn project_timecode_and_signed_footage_display() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "Format", "text");
        b.expr(&l,"text:source",doc(""),r#"[timeToCurrentFormat(60,24),timeToTimecode(60,24),timeToFeetAndFrames(-1,30)].join("|")"#);
    });
    let comp = eval(&s, 0.0);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    assert_eq!(
        comp.layers[0].text.as_ref().unwrap().text,
        "0:01:00:00|00:01:00:00|-0001+14"
    );
}

#[test]
fn posterize_time_and_frames() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(&l, "transform:rotation", 0.0, "posterizeTime(2); time")
            .expr(&l, "transform:opacity", 0.0, "timeToFrames()");
    });
    let ec = eval(&s, 1.9);
    assert_eq!(ec.layers[0].transform.rotation.z, 1.5);
    assert_eq!(ec.layers[0].transform.opacity, 57.0);
}

#[test]
fn source_text_and_effect_parameters() {
    let s = stage(|b| {
        let t = b.layer(MAIN, "title", "text");
        b.expr(
            &t,
            "text:source",
            Value::TextDocument(Box::new(TextDocument::new("x", "ArialMT", 50.0))),
            r#""Frame " + timeToFrames()"#,
        );
        let l = b.layer(MAIN, "fx", "null");
        b.prim(&format!("{l}/effects"), PrimType::FOLDER)
            .prim(&format!("{l}/effects/slider"), PrimType::EFFECT)
            .set(&format!("{l}/effects/slider"), "match_name", "ADBE Slider Control")
            .set(&format!("{l}/effects/slider"), "name", "Amount")
            .expr(&format!("{l}/effects/slider"), "param:0001", 0.0, "time * 4")
            .expr(&l, "transform:rotation", 0.0, r#"effect("Amount")(1) + effect("ADBE Slider Control")("ADBE Slider Control-0001")"#);
    });
    let ec = eval(&s, 1.0);
    assert_eq!(ec.layers[0].text.as_ref().unwrap().text, "Frame 30");
    assert_eq!(ec.layers[1].effects[0].param(1), Some(&Value::Float(4.0)));
    assert_eq!(ec.layers[1].transform.rotation.z, 8.0);
}

#[test]
fn evaluate_attribute_runs_expressions() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "a", "null");
        let c = b.layer(MAIN, "c", "null");
        b.set(&a, "transform:rotation", 7.0).expr(
            &c,
            "transform:rotation",
            0.0,
            r#"thisComp.layer("a").rotation * time"#,
        );
    });
    let v = evaluate_attribute(
        &s,
        &p("/comps/main/layers/c"),
        "transform:rotation",
        secs(2.0),
    )
    .unwrap();
    assert_eq!(v, Value::Float(14.0));
}

#[test]
fn disabled_expressions_are_ignored() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.spec(
            &l,
            "transform:rotation",
            AttrSpec {
                value: Some(Value::Float(5.0)),
                expression: Some(Expression {
                    source: "100".into(),
                    enabled: false,
                }),
                ..Default::default()
            },
        );
    });
    assert_eq!(eval(&s, 0.0).layers[0].transform.rotation.z, 5.0);
}

#[test]
fn evaluator_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Evaluator>();
    // Expressions run on any thread (each has its own QuickJS runtime).
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.expr(&l, "transform:rotation", 0.0, "time * 2");
    });
    // `Stage` is Send (not Sync): move it to the worker.
    let h = std::thread::spawn(move || eval(&s, 3.0).layers[0].transform.rotation.z);
    assert_eq!(h.join().unwrap(), 6.0);
}

/// Run `f` on a thread with a 1 MiB stack (the Windows main-thread size).
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(
            std::env::var("BAE_TEST_STACK")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1 << 20),
        )
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn long_dependency_chains_fit_a_small_stack() {
    on_small_stack(long_dependency_chains_in_both_directions);
}

#[test]
fn runaway_recursion_is_an_error_not_a_crash() {
    on_small_stack(|| {
        let s = stage(|b| {
            let l = b.layer(MAIN, "a", "null");
            b.expr(
                &l,
                "transform:rotation",
                3.0,
                "function f(n) { return f(n + 1) + 1; } f(0)",
            )
            .expr(
                &l,
                "transform:opacity",
                50.0,
                "var x = 0; while (true) { x++; if (x > 1e6) break; } x / 1e4",
            );
        });
        let ec = eval(&s, 0.0);
        assert_eq!(ec.layers[0].transform.rotation.z, 3.0);
        assert!(
            ec.errors.iter().any(|e| e.message.contains("stack")),
            "{:?}",
            ec.errors
        );
        assert_eq!(ec.layers[0].transform.opacity, 100.0);
    });
}

#[test]
fn long_dependency_chains_in_both_directions() {
    let n = 150;
    let s = stage(|b| {
        for i in 0..n {
            b.layer(MAIN, &format!("l{i}"), "null");
        }
        for i in 0..n {
            let l = format!("{MAIN}/layers/l{i}");
            // Rotation: each layer reads the one above (index - 1).
            let rot = if i == 0 {
                "1".to_owned()
            } else {
                "thisComp.layer(index - 1).rotation + 1".to_owned()
            };
            // Opacity: each layer reads the one below (index + 1).
            let op = if i == n - 1 {
                "1".to_owned()
            } else {
                "thisComp.layer(index + 1).opacity + 1".to_owned()
            };
            b.expr(&l, "transform:rotation", 0.0, &rot)
                .expr(&l, "transform:opacity", 0.0, &op);
        }
    });
    let ec = eval(&s, 0.0);
    assert!(
        ec.errors.is_empty(),
        "{:?}",
        &ec.errors[..ec.errors.len().min(3)]
    );
    assert_eq!(ec.layers[n - 1].transform.rotation.z, n as f64);
    // Opacity is clamped to 0–100 after the expression, as in AE.
    assert_eq!(ec.layers[n - 50].transform.opacity, 50.0);
    assert_eq!(ec.layers[0].transform.opacity, 100.0);
}

#[test]
fn camera_space_and_world_space_round_trip_property_values() {
    let s = stage(|b| {
        let cam = b.layer(MAIN, "Camera", "camera");
        b.set(&cam, "three_d", true)
            .set(&cam, "transform:position", [960.0, 540.0, -1500.0])
            .set(&cam, "transform:rotation", 35.0);
        let l = b.layer(MAIN, "Card", "null");
        b.set(&l, "three_d", true)
            .set(&l, "transform:position", [800.0, 400.0, 300.0])
            .set(&l, "transform:anchor_point", [20.0, 10.0, 0.0]);
        let t = b.layer(MAIN, "Read", "text");
        b.expr(
            &t,
            "text:source",
            doc(""),
            r#"var s=thisComp.layer("Card"), p=s.anchorPoint;
           var cp=s.toComp(p), wp=s.toWorld(p), cv=s.toCompVec([1,0,0]), wv=s.toWorldVec([1,0,0]);
           [length(s.fromComp(cp),p)<0.00001,length(s.fromWorld(wp),p)<0.00001,
            length(s.fromCompVec(cv),[1,0,0])<0.00001,length(s.fromWorldVec(wv),[1,0,0])<0.00001,
            length(wv,cv)>0.01,s.toWorld([0,0]).length,s.fromCompToSurface(cp).length].join("|")"#,
        );
    });
    let comp = eval(&s, 0.0);
    assert!(comp.errors.is_empty(), "{:?}", comp.errors);
    assert_eq!(
        comp.layers[2].text.as_ref().unwrap().text,
        "true|true|true|true|true|3|2"
    );
}

#[test]
fn to_comp_projects_3d_layers_through_the_default_camera() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.set(&l, "three_d", true)
            .set(&l, "transform:position", [800.0, 400.0, 300.0])
            .expr(&l, "transform:rotation", 0.0, "toComp([0, 0, 0])[0]")
            .expr(&l, "transform:opacity", 0.0, "toWorld([0, 0, 0])[2] / 10");
    });
    let ec = eval(&s, 0.0);
    let zoom = 1920.0 * 50.0 / 36.0;
    let want = 960.0 + (800.0 - 960.0) * zoom / (300.0 + zoom);
    assert!((ec.layers[0].transform.rotation.z - want).abs() < 1e-9);
    assert_eq!(ec.layers[0].transform.opacity, 30.0);
}

/// fromComp on a 3D layer: a point given at depth 0 (or behind the camera)
/// is taken at the comp plane, like a 2D point (no projection from the eye).
#[test]
fn from_comp_with_no_depth_uses_the_comp_plane() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "a", "null");
        b.set(&l, "three_d", true)
            .set(&l, "transform:position", [800.0, 400.0, 300.0])
            .expr(
                &l,
                "transform:rotation",
                0.0,
                "fromComp([10, 20, 0])[0] - fromComp([10, 20])[0]",
            );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(ec.layers[0].transform.rotation.z, 0.0);
}

/// key(index) takes a property for its index (e.g. a slider), as AE.
#[test]
fn key_takes_a_property_for_its_index() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "animated", "null");
        b.set(&l, "transform:opacity", 2.0);
        let mut spec = AttrSpec::default();
        spec.keyframes.insert(secs(0.0), Keyframe::linear(0.0));
        spec.keyframes.insert(secs(3.0), Keyframe::linear(300.0));
        spec.expression = Some(Expression::new("thisProperty.key(transform.opacity).time"));
        b.spec(&l, "transform:rotation", spec);
    });
    let ec = eval(&s, 1.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(ec.layers[0].transform.rotation.z, 3.0);
}

#[test]
fn expression_controls_by_name_index_and_layer_reference() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "controls", "null");
        let target = b.layer(MAIN, "target", "null");
        b.set(&target, "transform:rotation", 10.0);
        b.prim(&format!("{l}/effects"), PrimType::FOLDER);
        for (slug, name, mn, value) in [
            (
                "slider",
                "Slider Control",
                "ADBE Slider Control",
                Some(Value::Float(4.0)),
            ),
            (
                "point",
                "Point Control",
                "ADBE Point Control",
                Some(Value::Vec2([5.0, 6.0])),
            ),
            (
                "point_3d",
                "3D Point Control",
                "ADBE Point3D Control",
                Some(Value::Vec3([1.0, 2.0, 7.0])),
            ),
            (
                "angle",
                "Angle Control",
                "ADBE Angle Control",
                Some(Value::Float(8.0)),
            ),
            (
                "check",
                "Checkbox Control",
                "ADBE Checkbox Control",
                Some(Value::Bool(true)),
            ),
            (
                "color",
                "Color Control",
                "ADBE Color Control",
                Some(Value::Color([0.1, 0.2, 0.5, 1.0])),
            ),
            (
                "layer",
                "Layer Control",
                "ADBE Layer Control",
                Some(Value::PrimRef(p(&target))),
            ),
            ("self_layer", "Self", "ADBE Layer Control", None),
            (
                "menu",
                "Dropdown Menu Control",
                "ADBE Dropdown Control",
                Some(Value::Int(3)),
            ),
        ] {
            let fx = format!("{l}/effects/{slug}");
            b.prim(&fx, PrimType::EFFECT)
                .set(&fx, "match_name", mn)
                .set(&fx, "name", name);
            if let Some(value) = value {
                b.set(&fx, "param:0001", value);
            }
        }
        b.set(&format!("{l}/effects/check"), "enabled", false);
        b.expr(&l, "transform:rotation", 0.0, r#"
            if (effect("Checkbox Control").active || !effect("Checkbox Control")("Checkbox").value) {
                throw Error("wrong checkbox state");
            }
            if (effect("Self")("Layer").index !== thisLayer.index) throw Error("wrong default layer");
            effect("Slider Control")("Slider") + effect(1)(1)
              + effect("Point Control")("点")[1] + effect("3D Point Control")("3D Point")[2]
              + effect("Angle Control").param("Angle") + effect("Color Control")("Color")[2]
              + effect("Dropdown Menu Control")("Pseudo/@@wtM8H841Sg2uh023dgZBCw-0001")
              + effect("Layer Control")(1).rotation
        "#);
    });
    let ec = eval(&s, 0.0);
    assert_eq!(ec.layers[0].transform.rotation.z, 42.5);
}

#[test]
fn property_index_and_length_read_its_value() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "A", "solid");
        b.set(&l, "transform:position", Value::Vec3([10.0, 20.0, 0.0]));
        // A 2D layer's Position has two components in expressions.
        b.expr(
            &l,
            "transform:rotation",
            0.0,
            "transform.position[1] + transform.position.length + (transform.position[2] === undefined ? 100 : 0)",
        );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(ec.layers[0].transform.rotation.z, 122.0);
}

fn doc(text: &str) -> Value {
    Value::TextDocument(Box::new(TextDocument::new(text, "ArialMT", 50.0)))
}

/// `text.sourceText` reads as a string with the property's methods; `value` on
/// Source Text is the text; the AE 17.0+ style API reads and sets the style.
#[test]
fn source_text_strings_and_styles() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "A", "text");
        b.set(&a, "text:source", doc("Hello world"));
        let c = b.layer(MAIN, "C", "text");
        b.expr(
            &c,
            "text:source",
            doc("x"),
            r#"thisComp.layer("A").text.sourceText + "!""#,
        );
        let v = b.layer(MAIN, "V", "text");
        b.expr(&v, "text:source", doc("Hi"), r#"value + " there""#);
        let st = b.layer(MAIN, "S", "text");
        b.expr(
            &st,
            "text:source",
            doc("Styled"),
            "text.sourceText.style.setLineOrientation(\"vertical\").setFontSize(80).setFillColor([1, 0, 0]).setText(\"New\").setFauxBold(true)",
        );
        let n = b.layer(MAIN, "N", "null");
        b.expr(
            &n,
            "transform:rotation",
            0.0,
            r#"var t = thisComp.layer("A").text.sourceText; t.length * 100 + t.split(" ").length * 10 + t.style.fontSize / 10"#,
        );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    let text = |i: usize| ec.layers[i].text.as_ref().unwrap();
    assert_eq!(text(1).text, "Hello world!");
    assert_eq!(text(2).text, "Hi there");
    let styled = text(3);
    assert_eq!(styled.text, "New");
    assert_eq!(
        styled.line_orientation,
        bae_model::LineOrientation::Vertical
    );
    assert_eq!(styled.style.font_size, 80.0);
    assert_eq!(styled.style.fill_color[..3], [1.0, 0.0, 0.0]);
    assert!(styled.style.faux_bold);
    // 11 characters, 2 words, 50 pt.
    assert_eq!(ec.layers[4].transform.rotation.z, 1100.0 + 20.0 + 5.0);
}

#[test]
fn vertical_styles_follow_the_utf16_selection() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "A", "text");
        let mut document = TextDocument::new("中🙂12A", "Arial", 36.);
        document.line_orientation = bae_model::LineOrientation::Vertical;
        document.edit_styles(2..4, |style| {
            style.baseline_direction = bae_model::BaselineDirection::TateChuYoko
        });
        b.set(&a, "text:source", Value::TextDocument(Box::new(document)));
        let r = b.layer(MAIN, "R", "text");
        b.expr(&r,"text:source",doc("中🙂12A"),r#"
            var s = thisComp.layer("A").text.sourceText;
            if (s.style.lineOrientation !== "vertical" || s.getStyleAt(5).baselineDirection !== "default")
                throw Error("wrong source style");
            text.sourceText.style.setLineOrientation(s.style.lineOrientation)
                .setBaselineDirection(s.getStyleAt(3).baselineDirection,3,2)
        "#);
    });
    let ec = eval(&s, 0.);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    let d = ec.layers[1].text.as_ref().unwrap();
    assert_eq!(d.line_orientation, bae_model::LineOrientation::Vertical);
    assert_eq!(
        d.style_at(2).baseline_direction,
        bae_model::BaselineDirection::TateChuYoko
    );
    assert_eq!(
        d.style_at(4).baseline_direction,
        bae_model::BaselineDirection::Default
    );
}

#[test]
fn source_text_ranges_read_utf16_offsets_and_preserve_ordered_changes() {
    let s = stage(|b| {
        let a = b.layer(MAIN, "A", "text");
        let mut text = TextDocument::new("A🙂bc", "Arial", 12.);
        text.edit_styles(2..3, |s| s.font_size = 30.);
        b.set(&a, "text:source", Value::TextDocument(Box::new(text)));
        let target = b.layer(MAIN, "B", "text");
        b.expr(&target,"text:source",doc("X🙂yz"),
            r#"var a=thisComp.layer("A").text.sourceText; text.sourceText.style.setFontSize(a.getStyleAt(3).fontSize,1,3).setFontSize(18,3,1)"#);
    });
    let ec = eval(&s, 0.);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    let text = ec.layers[1].text.as_ref().unwrap();
    assert_eq!(
        (0..4)
            .map(|i| text.style_at(i).font_size)
            .collect::<Vec<_>>(),
        [50., 30., 18., 50.]
    );
}

/// `sourceRectAtTime`: the measured text through the registered measurer, a
/// solid's own size, a null's 100 × 100.
#[test]
fn source_rect_at_time() {
    struct Fixed;
    impl bae_anim::SourceBounds for Fixed {
        fn text(&self, doc: &TextDocument, extents: bool) -> Option<bae_anim::SourceRect> {
            let pad = if extents { 1.0 } else { 0.0 };
            Some([
                -10.0 - pad,
                -20.0 - pad,
                doc.text.len() as f64 * 10.0 + 2.0 * pad,
                30.0,
            ])
        }
        fn shapes(&self, _: &[bae_anim::EvaluatedPrim], _: bool) -> Option<bae_anim::SourceRect> {
            None
        }
    }
    bae_anim::set_source_bounds(std::sync::Arc::new(Fixed));
    let s = stage(|b| {
        let t = b.layer(MAIN, "T", "text");
        b.set(&t, "text:source", doc("Hello"));
        let n = b.layer(MAIN, "N", "null");
        b.expr(
            &n,
            "transform:position",
            Value::Vec3([0.0; 3]),
            r#"var r = thisComp.layer("T").sourceRectAtTime(time, true); [r.left + r.width, r.top + r.height]"#,
        );
        b.expr(&n, "transform:rotation", 0.0, "sourceRectAtTime().width");
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    let p = ec.layers[1].transform.position;
    assert_eq!([p.x, p.y], [-11.0 + 52.0, -21.0 + 30.0]);
    assert_eq!(
        ec.layers[1].transform.rotation.z, 100.0,
        "a null is 100 × 100"
    );
}

/// `marker` / `thisComp.marker`: numKeys, key(index), key(comment), nearestKey and
/// the MarkerKey fields; layer marker times are comp time (AE).
#[test]
fn markers_in_expressions() {
    let s = stage(|b| {
        b.prim(&format!("{MAIN}/markers"), PrimType::FOLDER)
            .prim(&format!("{MAIN}/markers/m1"), PrimType::MARKER)
            .set(
                &format!("{MAIN}/markers/m1"),
                "time",
                Value::Time(secs(2.0)),
            )
            .set(&format!("{MAIN}/markers/m1"), "comment", "intro")
            .set(
                &format!("{MAIN}/markers/m1"),
                "duration",
                Value::Time(secs(0.5)),
            );
        let l = b.layer(MAIN, "A", "null");
        b.set(&l, "start_time", Value::Time(secs(1.0)))
            .prim(&format!("{l}/markers"), PrimType::FOLDER)
            .prim(&format!("{l}/markers/a"), PrimType::MARKER)
            .set(&format!("{l}/markers/a"), "time", Value::Time(secs(3.0)))
            .set(&format!("{l}/markers/a"), "comment", "late")
            .prim(&format!("{l}/markers/b"), PrimType::MARKER)
            .set(&format!("{l}/markers/b"), "time", Value::Time(secs(0.5)))
            .set(&format!("{l}/markers/b"), "chapter", "One")
            .expr(
                &l,
                "transform:rotation",
                0.0,
                r#"marker.numKeys * 1000 + marker.key(1).time * 100 + marker.key("late").time * 10
                   + thisComp.marker.key("intro").duration + marker.nearestKey(4).index / 10"#,
            )
            .expr(
                &l,
                "transform:opacity",
                0.0,
                r#"marker.key(1).chapter == "One" && thisComp.marker.numKeys == 1 ? 50 : 0"#,
            );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    // 2 markers; b at layer 0.5 s = comp 1.5 s; "late" at comp 4 s; intro 0.5 s
    // long; the nearest to 4 s is "late", index 2.
    let r = ec.layers[0].transform.rotation.z;
    assert!(
        (r - (2000.0 + 150.0 + 40.0 + 0.5 + 0.2)).abs() < 1e-9,
        "{r}"
    );
    assert_eq!(ec.layers[0].transform.opacity, 50.0);
}

fn rect_path() -> Value {
    Value::Bezier(bae_model::BezierPath {
        vertices: vec![
            [100.0, 100.0],
            [300.0, 100.0],
            [300.0, 200.0],
            [100.0, 200.0],
        ],
        in_tangents: vec![[0.0; 2]; 4],
        out_tangents: vec![[0.0; 2]; 4],
        closed: true,
        feather: Vec::new(),
    })
}

/// Path properties: points / tangents / isClosed, pointOnPath at an arc-length
/// percentage (AE 26: 174.9986, 100 on this rectangle), and a path expression
/// returning createPath(…).
#[test]
fn path_property_methods_and_create_path() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "A", "solid");
        b.prim(&format!("{l}/masks"), PrimType::FOLDER)
            .prim(&format!("{l}/masks/m1"), PrimType::MASK)
            .set(&format!("{l}/masks/m1"), "name", "Mask 1")
            .set(&format!("{l}/masks/m1"), "path", rect_path())
            .prim(&format!("{l}/masks/m2"), PrimType::MASK)
            .set(&format!("{l}/masks/m2"), "name", "Mask 2")
            .expr(
                &format!("{l}/masks/m2"),
                "path",
                rect_path(),
                r#"var p = mask("Mask 1").maskPath;
                   createPath(p.points().map(function (v) { return [v[0] + 10, v[1]]; }),
                              p.inTangents(), p.outTangents(), false)"#,
            )
            .expr(
                &l,
                "transform:rotation",
                0.0,
                r#"var p = mask("Mask 1").maskPath; p.points()[1][0] + p.isClosed() * 1000 + p.tangentOnPath(0.875)[1]"#,
            )
            .expr(
                &l,
                "transform:position",
                Value::Vec3([0.0; 3]),
                r#"mask(1).maskPath.pointOnPath(0.125) + mask(1).maskPath.normalOnPath(0.125)"#,
            );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    let l = &ec.layers[0];
    assert_eq!(l.transform.rotation.z, 300.0 + 1000.0 - 1.0);
    // (175, 100) plus the normal (0, 1).
    assert!(
        (l.transform.position.x - 175.0).abs() < 0.01,
        "{:?}",
        l.transform.position
    );
    assert!((l.transform.position.y - 101.0).abs() < 0.01);
    let shifted = &l.masks[1].shape;
    assert_eq!(shifted.vertices[0], [110.0, 100.0]);
    assert!(!shifted.closed);
}

/// Shape layer contents: content(name / index) down to a path, a fill and the
/// group's transform, with AE's property names.
#[test]
fn shape_contents_in_expressions() {
    let s = stage(|b| {
        let l = b.layer(MAIN, "S", "shape");
        let g = format!("{l}/contents/g");
        b.prim(&format!("{l}/contents"), PrimType::FOLDER)
            .prim(&g, PrimType::SHAPE_GROUP)
            .set(&g, "name", "Group 1")
            .set(&g, "transform:position", Value::Vec2([40.0, 50.0]))
            .prim(&format!("{g}/contents"), PrimType::FOLDER)
            .prim(&format!("{g}/contents/p"), PrimType::PATH)
            .set(&format!("{g}/contents/p"), "name", "Path 1")
            .set(&format!("{g}/contents/p"), "path", rect_path())
            .prim(&format!("{g}/contents/f"), PrimType::FILL)
            .set(&format!("{g}/contents/f"), "name", "Fill 1")
            .set(&format!("{g}/contents/f"), "color", Value::Color([0.25, 0.5, 1.0, 1.0]))
            .expr(
                &l,
                "transform:rotation",
                0.0,
                r#"var g = content("Group 1");
                   g.content("Path 1").path.points()[1][0] + g.transform.position[1] * 10
                   + content(1).content(2).color[0] * 4 + (g.content(1).path.pointOnPath(0)[0] == 100)"#,
            );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(ec.layers[0].transform.rotation.z, 300.0 + 500.0 + 1.0 + 1.0);
}

/// Camera, light and material options, AE property names and property groups.
#[test]
fn camera_light_material_and_property_names() {
    let s = stage(|b| {
        let cam = b.layer(MAIN, "Cam", "camera");
        b.set(&cam, "camera:zoom", 1500.0)
            .set(&cam, "camera:depth_of_field", true);
        let light = b.layer(MAIN, "Key", "light");
        b.set(&light, "light:intensity", 80.0)
            .set(&light, "light:casts_shadows", true);
        let solid = b.layer(MAIN, "Plate", "solid");
        b.set(&solid, "three_d", true)
            .set(&solid, "material:accepts_lights", false)
            .prim(&format!("{solid}/masks"), PrimType::FOLDER)
            .prim(&format!("{solid}/masks/m"), PrimType::MASK)
            .set(&format!("{solid}/masks/m"), "name", "Mask 1")
            .set(
                &format!("{solid}/masks/m"),
                "path",
                Value::Bezier(bae_model::BezierPath {
                    vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                    in_tangents: vec![[0.0; 2]; 3],
                    out_tangents: vec![[0.0; 2]; 3],
                    closed: true,
                    feather: Vec::new(),
                }),
            );
        let t = b.layer(MAIN, "Names", "text");
        b.expr(
            &t,
            "text:source",
            Value::TextDocument(Box::new(TextDocument::new("x", "ArialMT", 50.0))),
            r#"var c = thisComp.layer("Cam"), k = thisComp.layer("Key"), p = thisComp.layer("Plate");
               [c.zoom.value, c.cameraOption.zoom.value, c.depthOfField.value, c.irisShape.value,
                k.intensity.value, k.castsShadows.value, p.castsShadows.value, p.acceptsLights.value,
                p.transform.position.name, p.transform.rotation.name, p.mask(1).maskPath.name,
                p.transform.position.propertyGroup(1).name, p.transform.position.propertyGroup(2).name,
                p.mask(1).maskPath.propertyGroup(1).name, p.mask(1).maskPath.propertyGroup(2).name].join("|")"#,
        );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(
        ec.layers[3].text.as_ref().unwrap().text,
        "1500|1500|1|1|80|1|0|0|Position|Z Rotation|Mask Path|Transform|Plate|Mask 1|Masks"
    );
}

/// footage(name), a footage layer's source, colorDepth / thisProject, NTSC and
/// feet + frames timecode, and fromCompToSurface on a 2D layer.
#[test]
fn footage_project_and_timecodes() {
    let s = stage(|b| {
        b.prim("/project", PrimType::PROJECT)
            .set("/project", "bit_depth", Value::Enum("32".into()))
            .prim("/assets", PrimType::FOLDER)
            .prim("/assets/clip", PrimType::ASSET)
            .set("/assets/clip", "name", "Clip")
            .set("/assets/clip", "kind", Value::Enum("footage".into()))
            .set("/assets/clip", "width", 640.0)
            .set("/assets/clip", "height", 360.0)
            .set("/assets/clip", "duration", Value::Time(secs(4.0)));
        let l = b.layer(MAIN, "Shot", "footage");
        b.set(&l, "source", Value::AssetRef(p("/assets/clip")));
        let t = b.layer(MAIN, "Info", "text");
        b.expr(
            &t,
            "text:source",
            Value::TextDocument(Box::new(TextDocument::new("x", "ArialMT", 50.0))),
            r#"var s = thisComp.layer("Shot");
               [s.source.width, footage("Clip").duration, footage("Clip").name, colorDepth,
                thisProject.bitsPerChannel, timeToNTSCTimecode(61, true), timeToNTSCTimecode(1),
                timeToFeetAndFrames(1, 30), thisComp.ntscDropFrame,
                fromCompToSurface([10, 20])[0] == fromComp([10, 20])[0]].join("|")"#,
        );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(
        ec.layers[1].text.as_ref().unwrap().text,
        "640|4|Clip|32|32|00;01;01;00|00:00:00:29|0001+14|false|true"
    );
}

/// hasVideo / hasAudio follow the layer's source (a pre-comp has audio when a
/// layer inside it does), and audioLevels exists only on layers with audio.
#[test]
fn audio_levels_and_has_audio() {
    let s = stage(|b| {
        b.prim("/assets", PrimType::FOLDER)
            .prim("/assets/song", PrimType::ASSET)
            .set("/assets/song", "name", "Song")
            .set("/assets/song", "has_video", false)
            .set("/assets/song", "has_audio", true)
            .prim("/assets/clip", PrimType::ASSET)
            .set("/assets/clip", "name", "Clip")
            .comp("/comps/other", "Other");
        let a = b.layer("/comps/other", "Music", "audio");
        b.set(&a, "source", Value::AssetRef(p("/assets/song")));
        let music = b.layer(MAIN, "Music", "audio");
        b.set(&music, "source", Value::AssetRef(p("/assets/song")))
            .set(&music, "audio:levels", Value::Vec2([-6.0, -3.0]));
        let clip = b.layer(MAIN, "Clip", "footage");
        b.set(&clip, "source", Value::AssetRef(p("/assets/clip")));
        let pre = b.layer(MAIN, "Pre", "pre_comp");
        b.set(&pre, "source", Value::PrimRef(p("/comps/other")));
        let t = b.layer(MAIN, "Info", "text");
        b.expr(
            &t,
            "text:source",
            Value::TextDocument(Box::new(TextDocument::new("x", "ArialMT", 50.0))),
            r#"var m = thisComp.layer("Music"), c = thisComp.layer("Clip"), p = thisComp.layer("Pre");
               var e = "";
               try { c.audioLevels; } catch (err) { e = err.message.split(".")[0]; }
               [m.audioLevels.value.join(","), m.hasAudio, m.hasVideo, m.audioActive, c.hasAudio,
                c.hasVideo, p.hasAudio, p.audioLevels.value.join(","), e].join("|")"#,
        );
    });
    let ec = eval(&s, 0.0);
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(
        ec.layers[3].text.as_ref().unwrap().text,
        "-6,-3|true|false|true|false|true|true|0,0|\
         property or method named 'audioLevels' in Class 'Layer' is missing or does not exist"
    );
}

/// evaluate_attribute_traced reports what the value was read from: the
/// attribute itself, same-layer and other-layer properties at the times read,
/// and what nested expressions read; edits to anything else leave it valid.
#[test]
fn traced_attribute_reports_its_reads() {
    use bae_anim::{ExprRead, ReadKind};
    use bae_model::{Change, TimeRange};
    let a = format!("{MAIN}/layers/A");
    let fx = format!("{a}/effects/rate");
    let s = stage(|b| {
        b.layer(MAIN, "A", "null");
        let l = b.layer(MAIN, "B", "null");
        b.expr(
            &l,
            "transform:rotation",
            0.0,
            r#"thisComp.layer("C").transform.scale[0]"#,
        );
        b.layer(MAIN, "C", "null");
        b.prim(&format!("{a}/effects"), PrimType::FOLDER)
            .prim(&fx, PrimType::EFFECT)
            .set(&fx, "match_name", "ADBE Slider Control")
            .set(&fx, "name", "Rate")
            .expr(
                &fx,
                "param:0001",
                0.0,
                r#"var b = thisComp.layer("B");
                   transform.opacity + b.transform.position.valueAtTime(time - 1)[0] + b.transform.rotation
                   + thisComp.layer("C").marker.numKeys"#,
            );
    });
    let mut ev = bae_anim::Evaluator::new();
    let (v, reads) = ev
        .evaluate_attribute_traced(&s, &p(&fx), "param:0001", secs(2.0))
        .unwrap();
    assert_eq!(v, Value::Float(100.0 + 0.0 + 100.0));
    let attr = |prim: &str, attr: &str, t: f64| ExprRead {
        prim: p(prim),
        kind: ReadKind::Attr(attr.into()),
        time: Some(secs(t)),
    };
    let (b, c) = (format!("{MAIN}/layers/B"), format!("{MAIN}/layers/C"));
    for want in [
        attr(&fx, "param:0001", 2.0),
        attr(&a, "transform:opacity", 2.0),
        attr(&b, "transform:position", 1.0),
        attr(&b, "transform:rotation", 2.0),
        attr(&c, "transform:scale", 2.0),
        ExprRead {
            prim: p(&format!("{c}/markers")),
            kind: ReadKind::Subtree,
            time: None,
        },
    ] {
        assert!(reads.contains(&want), "missing {want:?} in {reads:#?}");
    }
    let set = |path: &str, attr: &str| Change::AttrChanged {
        path: p(path),
        attr: attr.into(),
        range: TimeRange::ALL,
    };
    assert!(bae_anim::reads_touched(
        &reads,
        [&set(&c, "transform:scale")]
    ));
    assert!(bae_anim::reads_touched(
        &reads,
        [&set(&a, "transform:opacity")]
    ));
    assert!(!bae_anim::reads_touched(
        &reads,
        [&set(&c, "transform:position")]
    ));
    assert!(!bae_anim::reads_touched(
        &reads,
        [&set(&a, "transform:rotation")]
    ));
    assert!(!bae_anim::reads_touched(
        &reads,
        [&set(&b, "transform:opacity")]
    ));
    // Without an expression: the attribute itself.
    let (_, plain) = ev
        .evaluate_attribute_traced(&s, &p(&c), "transform:scale", secs(2.0))
        .unwrap();
    assert_eq!(plain, vec![attr(&c, "transform:scale", 2.0)]);
}

/// sampleImage reads layer pixels through the shared sample cache: a miss is
/// transparent black and asks for the sample; once the renderer fills it in,
/// the next evaluation reads it.
#[test]
fn sample_image_through_the_sample_cache() {
    let s = stage(|b| {
        b.layer(MAIN, "Plate", "solid");
        let t = b.layer(MAIN, "Info", "text");
        b.expr(
            &t,
            "text:source",
            Value::TextDocument(Box::new(TextDocument::new("x", "ArialMT", 50.0))),
            r#"var c = thisComp.layer("Plate").sampleImage([10, 20], [2, 1], false, 1);
               c.map(function (v) { return Math.round(v * 100) / 100; }).join(",")"#,
        );
    });
    let samples = std::sync::Arc::new(bae_anim::ImageSamples::new());
    let mut ev = bae_anim::Evaluator::new();
    ev.set_image_samples(Some(samples.clone()));
    let ec = ev.evaluate_comp(&s, &p(MAIN), secs(0.0)).unwrap();
    assert!(ec.errors.is_empty(), "{:?}", ec.errors);
    assert_eq!(ec.layers[1].text.as_ref().unwrap().text, "0,0,0,0");
    let taken = samples.take_requests();
    assert_eq!(taken.keys.len(), 1);
    let key = &taken.keys[0];
    assert_eq!(key.layer, p(&format!("{MAIN}/layers/Plate")));
    assert_eq!((key.time, key.post_effect), (secs(1.0), false));
    assert_eq!(key.area(), [8.0, 19.0, 12.0, 21.0]);
    samples.fulfil(taken.generation, key.clone(), [1.0, 0.5, 0.25, 0.75]);
    let ec = ev.evaluate_comp(&s, &p(MAIN), secs(0.0)).unwrap();
    assert_eq!(ec.layers[1].text.as_ref().unwrap().text, "1,0.5,0.25,0.75");
    assert!(!samples.is_waiting());
}

/// A live drag's values (overrides) reach everything that depends on them —
/// children through parenting, expressions reading them — without changing the
/// stage; clearing them shows the committed values again.
#[test]
fn overrides_show_uncommitted_values_everywhere() {
    use bae_anim::{AttrOverride, Evaluator};
    use bae_model::AttrSpec;
    let s = stage(|b| {
        let parent = b.layer(MAIN, "P", "null");
        let child = b.layer(MAIN, "C", "null");
        b.set(&child, "parent", Value::PrimRef(p(&parent))).set(
            &child,
            "transform:position",
            Value::Vec3([10.0, 0.0, 0.0]),
        );
        let reader = b.layer(MAIN, "R", "null");
        b.expr(
            &reader,
            "transform:rotation",
            0.0,
            r#"thisComp.layer("P").transform.position[0]"#,
        );
    });
    let parent = p(&format!("{MAIN}/layers/P"));
    let mut ev = Evaluator::new();
    let world_x = |ev: &mut Evaluator| {
        let ec = ev.evaluate_comp(&s, &p(MAIN), Time::ZERO).unwrap();
        (
            ec.layers[1].world_matrix.w_axis.x,
            ec.layers[2].transform.rotation.z,
        )
    };
    assert_eq!(world_x(&mut ev), (10.0, 0.0));
    ev.set_overrides(vec![AttrOverride {
        prim: parent.clone(),
        attr: "transform:position".into(),
        spec: AttrSpec {
            value: Some(Value::Vec3([100.0, 0.0, 0.0])),
            ..Default::default()
        },
    }]);
    assert_eq!(
        world_x(&mut ev),
        (110.0, 100.0),
        "child and expression follow"
    );
    ev.set_overrides(Vec::new());
    assert_eq!(world_x(&mut ev), (10.0, 0.0), "committed values again");
}

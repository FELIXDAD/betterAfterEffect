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

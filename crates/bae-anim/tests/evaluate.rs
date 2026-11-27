//! `evaluate_comp` / `evaluate_attribute` / `Evaluator` behaviour on small stages.

use bae_anim::glam::DVec3;
use bae_anim::{EvalError, Evaluator, LayerSource, TrackMatte, evaluate_attribute, evaluate_comp};
use bae_model::{
    AttrSpec, AuthorId, BezierPath, FrameRate, Keyframe, Op, PrimPath, PrimType, Stage,
    TextDocument, Time, Transaction, Value,
};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn secs(s: f64) -> Time {
    Time::from_seconds(s)
}

/// Small builder over `Stage::transact` for tests.
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
    fn folder(&mut self, path: &str) -> &mut Self {
        self.prim(path, PrimType::FOLDER)
    }
    fn set(&mut self, path: &str, attr: &str, v: impl Into<Value>) -> &mut Self {
        self.0
            .apply(Op::SetAttr {
                path: p(path),
                attr: attr.into(),
                spec: Some(AttrSpec::with_value(v.into())),
            })
            .unwrap();
        self
    }
    fn keys(&mut self, path: &str, attr: &str, keys: Vec<(f64, Keyframe)>) -> &mut Self {
        let mut spec = AttrSpec::default();
        for (t, k) in keys {
            spec.keyframes.insert(secs(t), k);
        }
        self.0
            .apply(Op::SetAttr {
                path: p(path),
                attr: attr.into(),
                spec: Some(spec),
            })
            .unwrap();
        self
    }
    fn layer(&mut self, comp: &str, name: &str, kind: &str) -> String {
        let path = format!("{comp}/layers/{name}");
        self.prim(&path, PrimType::LAYER)
            .set(&path, "kind", Value::Enum(kind.into()));
        path
    }
}

fn stage(f: impl FnOnce(&mut B<'_, '_>)) -> Stage {
    let mut stage = Stage::new();
    stage
        .transact(&AuthorId::new("user:test").unwrap(), "build", |tx| {
            let mut b = B(tx);
            b.folder("/comps")
                .prim("/comps/main", PrimType::COMP)
                .set("/comps/main", "duration", Value::Time(secs(10.0)))
                .folder("/comps/main/layers");
            f(&mut b);
            Ok::<_, bae_model::ModelError>(())
        })
        .unwrap();
    stage
}

fn close(a: DVec3, b: DVec3) {
    assert!((a - b).length() < 1e-9, "{a} != {b}");
}

#[test]
fn defaults_and_layer_order() {
    let s = stage(|b| {
        b.layer("/comps/main", "top", "solid");
        b.layer("/comps/main", "bottom", "null");
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    assert_eq!((ec.width, ec.height), (1920, 1080));
    assert_eq!(ec.frame_rate, FrameRate::FPS_30);
    assert_eq!(ec.layers.len(), 2);
    assert_eq!(ec.layers[0].path, p("/comps/main/layers/top"));
    let l = &ec.layers[0];
    assert_eq!(l.transform.scale, DVec3::splat(100.0));
    assert_eq!(l.opacity, 1.0);
    assert!(l.visible && l.active);
    // Default out point is the comp end.
    let late = evaluate_comp(&s, &p("/comps/main"), secs(10.0)).unwrap();
    assert!(!late.layers[0].active && !late.layers[0].visible);
}

#[test]
fn parenting_2d() {
    let s = stage(|b| {
        let parent = b.layer("/comps/main", "parent", "null");
        let child = b.layer("/comps/main", "child", "solid");
        b.set(&parent, "transform:position", [100.0, 100.0, 0.0])
            .set(&parent, "transform:rotation", 90.0)
            .set(&child, "transform:position", [10.0, 0.0, 0.0])
            .set(&child, "parent", Value::PrimRef(p(&parent)));
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let child = &ec.layers[1];
    assert_eq!(child.parent, Some(0));
    // Child origin: parent rotates (10, 0) by 90° clockwise to (0, 10), then + (100, 100).
    close(
        child.world_matrix.transform_point3(DVec3::ZERO),
        DVec3::new(100.0, 110.0, 0.0),
    );
}

#[test]
fn two_d_child_of_3d_parent_is_flattened() {
    let s = stage(|b| {
        let parent = b.layer("/comps/main", "parent", "null");
        let child = b.layer("/comps/main", "child", "solid");
        b.set(&parent, "three_d", true)
            .set(&parent, "transform:rotation_y", 60.0)
            .set(&parent, "transform:position", [0.0, 0.0, 500.0])
            .set(&child, "parent", Value::PrimRef(p(&parent)));
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let m = ec.layers[1].world_matrix;
    // AE 26: a 2D layer sees its 3D parent as 2D (X/Y rotation and z ignored).
    close(
        m.transform_point3(DVec3::new(100.0, 0.0, 0.0)),
        DVec3::new(100.0, 0.0, 0.0),
    );
}

#[test]
fn parent_cycles_do_not_hang() {
    let s = stage(|b| {
        let a = b.layer("/comps/main", "a", "null");
        let c = b.layer("/comps/main", "c", "null");
        b.set(&a, "parent", Value::PrimRef(p(&c)))
            .set(&c, "parent", Value::PrimRef(p(&a)));
    });
    evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
}

#[test]
fn solo_and_video_switch() {
    let s = stage(|b| {
        let a = b.layer("/comps/main", "a", "solid");
        let c = b.layer("/comps/main", "c", "solid");
        let d = b.layer("/comps/main", "d", "solid");
        b.set(&a, "solo", true).set(&c, "enabled", false);
        let _ = d;
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let vis: Vec<bool> = ec.layers.iter().map(|l| l.visible).collect();
    assert_eq!(vis, vec![true, false, false]);
}

#[test]
fn track_matte_defaults_to_layer_above() {
    let s = stage(|b| {
        b.layer("/comps/main", "matte", "shape");
        let fill = b.layer("/comps/main", "fill", "solid");
        b.set(&fill, "track_matte", Value::Enum("luma_inverted".into()));
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    assert_eq!(ec.layers[1].track_matte, TrackMatte::LumaInverted);
    assert_eq!(ec.layers[1].track_matte_layer, Some(0));
    assert_eq!(ec.layers[0].track_matte_layer, None);
}

#[test]
fn layer_time_and_precomp_source_time() {
    let s = stage(|b| {
        b.prim("/comps/inner", PrimType::COMP)
            .set(
                "/comps/inner",
                "frame_rate",
                Value::FrameRate(FrameRate::new(12, 1)),
            )
            .set("/comps/inner", "preserve_frame_rate", true)
            .set("/comps/inner", "duration", Value::Time(secs(10.0)))
            .folder("/comps/inner/layers");
        let inner = b.layer("/comps/inner", "spin", "null");
        b.keys(
            &inner,
            "transform:rotation",
            vec![(0.0, Keyframe::linear(0.0)), (1.0, Keyframe::linear(100.0))],
        );
        let l = b.layer("/comps/main", "pre", "pre_comp");
        b.set(&l, "source", Value::PrimRef(p("/comps/inner")))
            .set(&l, "start_time", Value::Time(secs(1.0)))
            .set(&l, "stretch", 200.0)
            // Keyframes in layer time: 0 → 10 over one layer second.
            .keys(&l, "transform:rotation", vec![
                (0.0, Keyframe::linear(0.0)),
                (1.0, Keyframe::linear(10.0)),
            ]);
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), secs(2.0)).unwrap();
    let l = &ec.layers[0];
    // Comp 2 s → layer (2 - 1) / 2 = 0.5 s.
    assert_eq!(l.layer_time, secs(0.5));
    assert!((l.transform.rotation.z - 5.0).abs() < 1e-12);
    // Inner comp time snapped to 12 fps (0.5 s is on a frame; 0.52 s is not).
    let LayerSource::Comp { comp, time, nested } = &l.source else {
        panic!("{:?}", l.source)
    };
    assert_eq!(comp, &p("/comps/inner"));
    assert_eq!(*time, secs(0.5));
    // The pre-comp is evaluated recursively at the mapped time.
    let nested = nested.as_ref().expect("nested comp evaluated");
    assert_eq!(nested.path, p("/comps/inner"));
    assert_eq!(nested.time, secs(0.5));
    assert!((nested.layers[0].transform.rotation.z - 50.0).abs() < 1e-9);
    let ec = evaluate_comp(&s, &p("/comps/main"), secs(2.04)).unwrap();
    let LayerSource::Comp { time, .. } = ec.layers[0].source else {
        panic!()
    };
    assert_eq!(time, secs(0.5));
}

#[test]
fn time_remap_drives_source_time() {
    let s = stage(|b| {
        b.prim("/assets", PrimType::FOLDER)
            .prim("/assets/clip", PrimType::ASSET);
        let l = b.layer("/comps/main", "clip", "footage");
        b.set(&l, "source", Value::PrimRef(p("/assets/clip")))
            .set(&l, "time_remap_enabled", true)
            .keys(
                &l,
                "time_remap",
                vec![
                    (0.0, Keyframe::linear(Value::Time(secs(4.0)))),
                    (2.0, Keyframe::linear(Value::Time(secs(0.0)))),
                ],
            );
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), secs(0.5)).unwrap();
    let LayerSource::Asset { asset, time, info } = &ec.layers[0].source else {
        panic!("{:?}", ec.layers[0].source)
    };
    assert_eq!((asset, *time), (&p("/assets/clip"), secs(3.0)));
    assert_eq!(info.kind, "footage");
    assert_eq!(info.solid_color, None);
}

#[test]
fn solid_source_info() {
    let s = stage(|b| {
        b.prim("/assets", PrimType::FOLDER)
            .prim("/assets/red", PrimType::ASSET)
            .set("/assets/red", "kind", Value::Enum("solid".into()))
            .set("/assets/red", "width", Value::Int(640))
            .set("/assets/red", "height", Value::Int(360))
            .set(
                "/assets/red",
                "solid_color",
                Value::Color([1.0, 0.0, 0.0, 1.0]),
            );
        let l = b.layer("/comps/main", "red", "solid");
        b.set(&l, "source", Value::PrimRef(p("/assets/red"))).set(
            &l,
            "preserve_transparency",
            true,
        );
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let l = &ec.layers[0];
    assert!(l.switches.preserve_transparency);
    let LayerSource::Asset { info, .. } = &l.source else {
        panic!()
    };
    assert_eq!((info.width, info.height), (640, 360));
    assert_eq!(info.solid_color, Some([1.0, 0.0, 0.0, 1.0]));
}

#[test]
fn precomp_cycles_are_cut() {
    let s = stage(|b| {
        let l = b.layer("/comps/main", "self", "pre_comp");
        b.set(&l, "source", Value::PrimRef(p("/comps/main")));
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let LayerSource::Comp { nested, .. } = &ec.layers[0].source else {
        panic!()
    };
    assert!(nested.is_none());
}

#[test]
fn motion_blur_sample_times() {
    let s = stage(|b| {
        b.set("/comps/main", "motion_blur", true).set(
            "/comps/main",
            "motion_blur_samples",
            Value::Int(4),
        );
        let a = b.layer("/comps/main", "blurred", "solid");
        b.layer("/comps/main", "sharp", "solid");
        b.set(&a, "motion_blur", true);
    });
    let t = FrameRate::FPS_30.frame_to_time(3);
    let ec = evaluate_comp(&s, &p("/comps/main"), t).unwrap();
    let times = &ec.layers[0].motion_blur_times;
    assert_eq!(times.len(), 4);
    // Default shutter 180° / -90°: centred on the frame, spanning half a frame.
    let span = times[3] - times[0];
    let frame = FrameRate::FPS_30.frame_duration();
    assert!((span.0 - frame.0 * 3 / 8).abs() <= 1, "{span:?}");
    assert!(times[0] < t && times[3] > t);
    assert!(ec.layers[1].motion_blur_times.is_empty());
}

#[test]
fn motion_blur_adaptive_limit() {
    let s = stage(|_| {});
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    assert_eq!(ec.motion_blur.adaptive_limit, 128, "AE default");
    for (set, want) in [(40, 40), (4, 16), (999, 256)] {
        let s = stage(|b| {
            b.set("/comps/main", "motion_blur_adaptive_limit", Value::Int(set));
        });
        let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
        assert_eq!(ec.motion_blur.adaptive_limit, want, "{set}");
    }
}

#[test]
fn text_source_document() {
    let s = stage(|b| {
        let l = b.layer("/comps/main", "title", "text");
        b.keys(
            &l,
            "text:source",
            vec![
                (
                    0.0,
                    Keyframe::linear(Value::TextDocument(Box::new(TextDocument::new(
                        "Hello", "ArialMT", 72.0,
                    )))),
                ),
                (
                    1.0,
                    Keyframe::linear(Value::TextDocument(Box::new(TextDocument::new(
                        "World", "ArialMT", 72.0,
                    )))),
                ),
            ],
        );
    });
    let at = |t: f64| {
        evaluate_comp(&s, &p("/comps/main"), secs(t))
            .unwrap()
            .layers[0]
            .text
            .as_ref()
            .unwrap()
            .text
            .clone()
    };
    // Text documents only hold.
    assert_eq!(at(0.5), "Hello");
    assert_eq!(at(1.0), "World");
}

#[test]
fn masks_effects_and_contents() {
    let s = stage(|b| {
        let l = b.layer("/comps/main", "shape", "shape");
        let square = BezierPath {
            vertices: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            in_tangents: vec![[0.0; 2]; 3],
            out_tangents: vec![[0.0; 2]; 3],
            closed: true,
            feather: Vec::new(),
        };
        let mut big = square.clone();
        big.vertices = vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]];
        b.folder(&format!("{l}/masks"))
            .prim(&format!("{l}/masks/m1"), PrimType::MASK)
            .set(
                &format!("{l}/masks/m1"),
                "mode",
                Value::Enum("subtract".into()),
            )
            .keys(
                &format!("{l}/masks/m1"),
                "path",
                vec![
                    (0.0, Keyframe::linear(Value::Bezier(square))),
                    (1.0, Keyframe::linear(Value::Bezier(big))),
                ],
            )
            .folder(&format!("{l}/effects"))
            .prim(&format!("{l}/effects/blur"), PrimType::EFFECT)
            .set(
                &format!("{l}/effects/blur"),
                "match_name",
                "ADBE Gaussian Blur 2",
            )
            .keys(
                &format!("{l}/effects/blur"),
                "param:0001",
                vec![(0.0, Keyframe::linear(0.0)), (1.0, Keyframe::linear(20.0))],
            )
            .folder(&format!("{l}/contents"))
            .prim(&format!("{l}/contents/rect"), PrimType::RECT)
            .set(&format!("{l}/contents/rect"), "size", [50.0, 60.0]);
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), secs(0.5)).unwrap();
    let l = &ec.layers[0];
    assert_eq!(l.masks.len(), 1);
    assert_eq!(l.masks[0].mode, "subtract");
    assert_eq!(l.masks[0].shape.vertices[2], [15.0, 15.0]);
    assert_eq!(l.masks[0].opacity, 1.0);
    assert_eq!(l.effects[0].match_name, "ADBE Gaussian Blur 2");
    assert_eq!(l.effects[0].param(1), Some(&Value::Float(10.0)));
    let rect = &l.contents[0];
    assert_eq!(rect.get("size"), Some(&Value::Vec2([50.0, 60.0])));
    // Animatable schema attributes without an opinion are filled with defaults.
    assert_eq!(rect.get("roundness"), Some(&Value::Float(0.0)));
}

#[test]
fn two_node_camera_aims_at_point_of_interest() {
    let s = stage(|b| {
        let cam = b.layer("/comps/main", "cam", "camera");
        b.set(&cam, "transform:position", [960.0, 540.0, -1000.0])
            .set(
                &cam,
                "transform:point_of_interest",
                [1960.0, 540.0, -1000.0],
            );
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    assert_eq!(ec.camera, Some(0));
    let cam = ec.layers[0].camera.unwrap();
    assert!(cam.two_node);
    // Looking along +x: local +z maps to comp +x.
    let fwd = cam.world_matrix.transform_vector3(DVec3::Z);
    close(fwd, DVec3::X);
}

#[test]
fn evaluate_attribute_uses_layer_time_and_defaults() {
    let s = stage(|b| {
        let l = b.layer("/comps/main", "l", "null");
        b.set(&l, "start_time", Value::Time(secs(1.0))).keys(
            &l,
            "transform:opacity",
            vec![(0.0, Keyframe::linear(0.0)), (1.0, Keyframe::linear(100.0))],
        );
    });
    let l = p("/comps/main/layers/l");
    let v = evaluate_attribute(&s, &l, "transform:opacity", secs(1.5)).unwrap();
    assert_eq!(v, Value::Float(50.0));
    let v = evaluate_attribute(&s, &l, "transform:scale", secs(1.5)).unwrap();
    assert_eq!(v, Value::Vec3([100.0; 3]));
    assert!(matches!(
        evaluate_attribute(&s, &l, "no_such_attr", Time::ZERO),
        Err(EvalError::NoValue { .. })
    ));
    assert!(matches!(
        evaluate_comp(&s, &l, Time::ZERO),
        Err(EvalError::NotAComp(_))
    ));
}

#[test]
fn evaluator_cache_invalidates_on_events() {
    let mut s = stage(|b| {
        b.layer("/comps/main", "l", "null");
    });
    let mut ev = Evaluator::new();
    let comp = p("/comps/main");
    assert_eq!(
        ev.evaluate_comp(&s, &comp, Time::ZERO).unwrap().layers[0]
            .transform
            .rotation
            .z,
        0.0
    );
    let (_, event) = s
        .transact(&AuthorId::new("user:test").unwrap(), "rotate", |tx| {
            tx.apply(Op::SetAttr {
                path: p("/comps/main/layers/l"),
                attr: "transform:rotation".into(),
                spec: Some(AttrSpec::with_value(45.0)),
            })
        })
        .unwrap();
    // Stale until invalidated.
    assert_eq!(
        ev.evaluate_comp(&s, &comp, Time::ZERO).unwrap().layers[0]
            .transform
            .rotation
            .z,
        0.0
    );
    ev.invalidate(&event);
    assert_eq!(
        ev.evaluate_comp(&s, &comp, Time::ZERO).unwrap().layers[0]
            .transform
            .rotation
            .z,
        45.0
    );
}

#[test]
fn evaluated_comp_serializes_to_json() {
    let s = stage(|b| {
        b.layer("/comps/main", "l", "solid");
    });
    let ec = evaluate_comp(&s, &p("/comps/main"), Time::ZERO).unwrap();
    let json = serde_json::to_value(&ec).unwrap();
    assert_eq!(json["layers"][0]["kind"], "solid");
    assert_eq!(json["layers"][0]["source"]["kind"], "none");
}

#[test]
fn text_animator_samples_only_authored_properties_and_samples_nested_selector() {
    let mut stage = stage(|b| {
        b.layer("/comps/main", "text", "text");
    });
    let registry = bae_model::CommandRegistry::with_builtins();
    let layer = "/comps/main/layers/text";
    let group = format!("{layer}/animators/title");
    let mut command = |id, params| {
        registry
            .execute(&mut stage, &AuthorId::new("user:test").unwrap(), id, params)
            .unwrap();
    };
    use serde_json::json;
    command(
        "text.create_animator",
        json!({"layer":layer, "name":"title"}),
    );
    for (sec, opacity) in [(0.0, 20.0), (1.0, 80.0)] {
        command(
            "key.set",
            json!({"path":group,"attr":"animator:opacity","time":secs(sec),"keyframe":Keyframe::linear(opacity)}),
        );
    }
    command(
        "attr.set_expression",
        json!({"path":format!("{group}/range"),"attr":"end","source":"time * 100"}),
    );
    let result = evaluate_comp(&stage, &p("/comps/main"), secs(0.5)).unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let animator = &result.layers[0].text_animators[0];
    assert_eq!(animator.f64("animator:opacity"), Some(50.0));
    assert!(animator.get("animator:fill_color").is_none());
    assert_eq!(animator.children[0].f64("end"), Some(50.0));
}

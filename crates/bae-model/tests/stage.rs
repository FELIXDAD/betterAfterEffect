use std::sync::{Arc, Mutex};
use std::time::Instant;

use bae_model::{
    AuthorId, Change, CommandError, CommandRegistry, EventOrigin, Keyframe, ModelError, Op,
    PrimPath, PrimType, Specifier, Stage, StageEvent, SublayerId, Time, TimeRange, Value,
};
use serde_json::json;

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn sl(s: &str) -> SublayerId {
    SublayerId::new(s).unwrap()
}

fn author(s: &str) -> AuthorId {
    AuthorId::new(s).unwrap()
}

fn run(
    reg: &CommandRegistry,
    stage: &mut Stage,
    who: &str,
    id: &str,
    params: serde_json::Value,
) -> StageEvent {
    reg.execute(stage, &author(who), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .event
}

/// /comps (Folder) / main (Comp) / layers (Folder) / title (Layer, opacity 100)
fn base_stage(reg: &CommandRegistry) -> Stage {
    let mut stage = Stage::new();
    for (path, t) in [
        ("/comps", "Folder"),
        ("/comps/main", "Comp"),
        ("/comps/main/layers", "Folder"),
        ("/comps/main/layers/title", "Layer"),
    ] {
        run(
            reg,
            &mut stage,
            "user:a",
            "prim.create",
            json!({"path": path, "prim_type": t}),
        );
    }
    run(
        reg,
        &mut stage,
        "user:a",
        "attr.set",
        json!({"path": "/comps/main/layers/title", "attr": "opacity", "value": {"type": "float", "value": 100.0}}),
    );
    stage
}

fn float(stage: &Stage, path: &str, attr: &str) -> Option<f64> {
    stage.prim(&p(path))?.value(attr)?.as_f64()
}

fn trees_json(stage: &Stage, id: &SublayerId) -> serde_json::Value {
    serde_json::to_value(stage.sublayer(id).unwrap().to_trees()).unwrap()
}

#[test]
fn stronger_sublayer_wins_and_clear_reveals_weaker() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    run(
        &reg,
        &mut stage,
        "user:a",
        "sublayer.create",
        json!({"id": "anim/shot010"}),
    );
    assert_eq!(
        stage.layer_stack(),
        &[sl("anim/shot010"), SublayerId::root()]
    );
    run(
        &reg,
        &mut stage,
        "user:a",
        "sublayer.set_edit_target",
        json!({"id": "anim/shot010"}),
    );

    let title = "/comps/main/layers/title";
    run(
        &reg,
        &mut stage,
        "user:a",
        "attr.set",
        json!({"path": title, "attr": "opacity", "value": {"type": "float", "value": 50.0}}),
    );
    assert_eq!(float(&stage, title, "opacity"), Some(50.0));
    assert_eq!(
        stage.prim(&p(title)).unwrap().attr_source("opacity"),
        Some(&sl("anim/shot010"))
    );
    // The weak layer is untouched; the strong one got `over` specs for the ancestors.
    let root = stage.sublayer(&SublayerId::root()).unwrap();
    assert_eq!(
        root.attr(&p(title), "opacity").unwrap().value,
        Some(Value::Float(100.0))
    );
    let anim = stage.sublayer(&sl("anim/shot010")).unwrap();
    for a in ["/comps", "/comps/main", "/comps/main/layers", title] {
        assert_eq!(anim.spec(&p(a)).unwrap().specifier, Specifier::Over, "{a}");
    }
    // Type comes from the weak `def`.
    assert_eq!(
        stage.prim(&p(title)).unwrap().prim_type(),
        Some(&PrimType::LAYER)
    );

    run(
        &reg,
        &mut stage,
        "user:a",
        "attr.clear",
        json!({"path": title, "attr": "opacity"}),
    );
    assert_eq!(float(&stage, title, "opacity"), Some(100.0));
    assert_eq!(
        stage.prim(&p(title)).unwrap().attr_source("opacity"),
        Some(&SublayerId::root())
    );

    // Moving the sublayer below root flips the winner.
    run(
        &reg,
        &mut stage,
        "user:a",
        "attr.set",
        json!({"path": title, "attr": "opacity", "value": {"type": "float", "value": 25.0}}),
    );
    stage.move_sublayer(&sl("anim/shot010"), 1).unwrap();
    assert_eq!(float(&stage, title, "opacity"), Some(100.0));
}

#[test]
fn over_only_prims_are_not_on_the_stage() {
    let mut stage = Stage::new();
    stage
        .apply_edits(
            &AuthorId::system(),
            "over",
            vec![bae_model::Edit::new(
                SublayerId::root(),
                Op::CreatePrim {
                    path: p("/ghost"),
                    specifier: Specifier::Over,
                    prim_type: None,
                    index: None,
                },
            )],
        )
        .unwrap();
    assert!(!stage.has_prim(&p("/ghost")));
    assert!(stage.traverse().is_empty());
}

#[test]
fn undo_is_per_author() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    let title = "/comps/main/layers/title";
    let set =
        |v: f64| json!({"path": title, "attr": "opacity", "value": {"type": "float", "value": v}});
    run(&reg, &mut stage, "user:alice", "attr.set", set(10.0));
    run(
        &reg,
        &mut stage,
        "user:bob",
        "attr.set",
        json!({"path": title, "attr": "scale", "value": {"type": "vec2", "value": [50.0, 50.0]}}),
    );
    assert_eq!(stage.undo_label(&author("user:alice")), Some("Set Value"));

    let ev = stage.undo(&author("user:alice")).unwrap();
    assert_eq!(ev.origin, EventOrigin::Undo);
    assert_eq!(float(&stage, title, "opacity"), Some(100.0));
    // Bob's change survives Alice's undo.
    assert_eq!(
        stage.prim(&p(title)).unwrap().value("scale"),
        Some(&Value::Vec2([50.0, 50.0]))
    );

    stage.redo(&author("user:alice")).unwrap();
    assert_eq!(float(&stage, title, "opacity"), Some(10.0));
    assert!(!stage.can_redo(&author("user:alice")));

    // A new edit clears the redo stack.
    stage.undo(&author("user:alice")).unwrap();
    run(&reg, &mut stage, "user:alice", "attr.set", set(20.0));
    assert!(!stage.can_redo(&author("user:alice")));
    assert!(stage.undo(&author("user:nobody")).is_none());
}

#[test]
fn delete_and_undo_restore_the_whole_subtree() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    let title = "/comps/main/layers/title";
    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.create",
        json!({"path": "/comps/main/layers/bg", "prim_type": "Layer"}),
    );
    for (t, v) in [(0, 0.0), (705_600_000, 100.0)] {
        run(
            &reg,
            &mut stage,
            "user:a",
            "key.set",
            json!({"path": title, "attr": "opacity", "time": t, "keyframe": {"value": {"type": "float", "value": v}}}),
        );
    }
    run(
        &reg,
        &mut stage,
        "user:a",
        "attr.set_expression",
        json!({"path": title, "attr": "rotation", "source": "time * 36"}),
    );
    let before = trees_json(&stage, &SublayerId::root());

    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.delete",
        json!({"path": "/comps/main"}),
    );
    assert!(!stage.has_prim(&p(title)));
    assert_eq!(stage.traverse(), vec![p("/comps")]);

    stage.undo(&author("user:a")).unwrap();
    assert_eq!(trees_json(&stage, &SublayerId::root()), before);
    assert_eq!(
        stage.children(&p("/comps/main/layers")),
        vec![p(title), p("/comps/main/layers/bg")]
    );
    stage.redo(&author("user:a")).unwrap();
    assert!(!stage.has_prim(&p("/comps/main")));
}

#[test]
fn rename_moves_overrides_in_every_sublayer() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    run(
        &reg,
        &mut stage,
        "user:a",
        "sublayer.create",
        json!({"id": "anim"}),
    );
    run(
        &reg,
        &mut stage,
        "user:a",
        "sublayer.set_edit_target",
        json!({"id": "anim"}),
    );
    run(
        &reg,
        &mut stage,
        "user:a",
        "attr.set",
        json!({"path": "/comps/main/layers/title", "attr": "opacity", "value": {"type": "float", "value": 5.0}}),
    );

    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.rename",
        json!({"path": "/comps/main", "name": "hero"}),
    );
    assert!(!stage.has_prim(&p("/comps/main")));
    assert_eq!(
        float(&stage, "/comps/hero/layers/title", "opacity"),
        Some(5.0)
    );
    assert!(
        stage
            .sublayer(&sl("anim"))
            .unwrap()
            .has_spec(&p("/comps/hero/layers/title"))
    );

    stage.undo(&author("user:a")).unwrap();
    assert_eq!(
        float(&stage, "/comps/main/layers/title", "opacity"),
        Some(5.0)
    );
    assert!(!stage.has_prim(&p("/comps/hero")));
}

#[test]
fn reorder_and_move_between_parents() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    for n in ["a", "b", "c"] {
        run(
            &reg,
            &mut stage,
            "user:a",
            "prim.create",
            json!({"path": format!("/comps/main/layers/{n}"), "prim_type": "Layer"}),
        );
    }
    let layers = |s: &Stage| -> Vec<String> {
        s.children(&p("/comps/main/layers"))
            .iter()
            .map(|c| c.name().to_owned())
            .collect()
    };
    assert_eq!(layers(&stage), ["title", "a", "b", "c"]);
    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.move",
        json!({"from": "/comps/main/layers/c", "to": "/comps/main/layers/c", "index": 0}),
    );
    assert_eq!(layers(&stage), ["c", "title", "a", "b"]);
    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.move",
        json!({"from": "/comps/main/layers/title", "to": "/comps/main/layers/title", "index": 3}),
    );
    assert_eq!(layers(&stage), ["c", "a", "b", "title"]);
    stage.undo(&author("user:a")).unwrap();
    assert_eq!(layers(&stage), ["c", "title", "a", "b"]);

    run(
        &reg,
        &mut stage,
        "user:a",
        "prim.move",
        json!({"from": "/comps/main/layers/a", "to": "/comps/a"}),
    );
    assert_eq!(layers(&stage), ["c", "title", "b"]);
    assert_eq!(
        stage.children(&p("/comps")),
        vec![p("/comps/main"), p("/comps/a")]
    );

    let err = reg
        .execute(
            &mut stage,
            &author("user:a"),
            "prim.move",
            json!({"from": "/comps", "to": "/comps/main/x"}),
        )
        .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::CyclicMove { .. })),
        "{err}"
    );
}

#[test]
fn failed_transaction_rolls_back_without_event_or_undo() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    let events = Arc::new(Mutex::new(0));
    let counter = events.clone();
    stage.subscribe(move |_| *counter.lock().unwrap() += 1);
    let before = trees_json(&stage, &SublayerId::root());
    let undo_before = stage.undo_label(&author("user:a")).map(str::to_owned);

    let res = stage.transact(&author("user:a"), "broken", |tx| {
        tx.apply(Op::CreatePrim {
            path: p("/comps/tmp"),
            specifier: Specifier::Def,
            prim_type: Some(PrimType::COMP),
            index: None,
        })?;
        tx.apply(Op::SetValue {
            path: p("/comps/tmp"),
            attr: "opacity".into(),
            value: Some(Value::Float(1.0)),
        })?;
        // Fails: no such prim.
        tx.apply(Op::DeletePrim { path: p("/nope") })
    });
    assert!(res.is_err());
    assert_eq!(trees_json(&stage, &SublayerId::root()), before);
    assert_eq!(*events.lock().unwrap(), 0);
    assert_eq!(
        stage.undo_label(&author("user:a")).map(str::to_owned),
        undo_before
    );
}

#[test]
fn guarded_transaction_never_writes_denied_sublayers() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    stage.create_sublayer(sl("agent/x"), 0).unwrap();
    stage.set_edit_target(&sl("agent/x")).unwrap();
    let root_version = stage.sublayer(&SublayerId::root()).unwrap().version();
    let only_agent = |id: &SublayerId| id.as_str().starts_with("agent/");
    let delete = reg.get("prim.delete").unwrap().clone();

    // A namespace edit would touch root: refused before anything is written.
    let res = stage.transact_guarded(&author("agent:x"), "delete", &only_agent, |tx| {
        delete.run_json(tx, json!({"path": "/comps/main/layers/title"}))
    });
    assert!(
        matches!(res, Err(CommandError::Model(ModelError::WriteDenied(ref id))) if *id == SublayerId::root()),
        "{res:?}"
    );
    assert_eq!(
        stage.sublayer(&SublayerId::root()).unwrap().version(),
        root_version
    );
    assert!(stage.has_prim(&p("/comps/main/layers/title")));

    // Stack and edit-target changes are guarded too.
    let res = stage.transact_guarded(&author("agent:x"), "t", &only_agent, |tx| {
        tx.set_edit_target(&SublayerId::root())
    });
    assert!(matches!(res, Err(ModelError::WriteDenied(_))));
    let res = stage.transact_guarded(&author("agent:x"), "t", &only_agent, |tx| {
        tx.create_sublayer(sl("anim/y"), 0)
    });
    assert!(matches!(res, Err(ModelError::WriteDenied(_))));
    assert_eq!(stage.layer_stack(), &[sl("agent/x"), SublayerId::root()]);

    // Allowed writes go through, with the usual undo.
    let set = reg.get("attr.set").unwrap().clone();
    stage
        .transact_guarded(&author("agent:x"), "set", &only_agent, |tx| {
            assert!(tx.may_write(&sl("agent/x")) && !tx.may_write(&SublayerId::root()));
            set.run_json(
                tx,
                json!({"path": "/comps/main/layers/title", "attr": "opacity", "value": {"type": "float", "value": 50.0}}),
            )
        })
        .unwrap();
    assert_eq!(
        float(&stage, "/comps/main/layers/title", "opacity"),
        Some(50.0)
    );
    assert!(stage.undo(&author("agent:x")).is_some());
    assert_eq!(
        float(&stage, "/comps/main/layers/title", "opacity"),
        Some(100.0)
    );
}

#[test]
fn events_report_paths_and_keyframe_ranges() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = base_stage(&reg);
    let seen: Arc<Mutex<Vec<StageEvent>>> = Arc::default();
    let sink = seen.clone();
    let sub = stage.subscribe(move |e| sink.lock().unwrap().push(e.clone()));
    let title = "/comps/main/layers/title";
    for t in [10, 20, 30] {
        run(
            &reg,
            &mut stage,
            "user:a",
            "key.set",
            json!({"path": title, "attr": "opacity", "time": t, "keyframe": {"value": {"type": "float", "value": 1.0}}}),
        );
    }
    let ev = run(
        &reg,
        &mut stage,
        "user:a",
        "key.set",
        json!({"path": title, "attr": "opacity", "time": 20, "keyframe": {"value": {"type": "float", "value": 2.0}}}),
    );
    assert_eq!(
        ev.changes,
        vec![Change::AttrChanged {
            path: p(title),
            attr: "opacity".into(),
            range: TimeRange::new(Time(10), Time(31)),
        }]
    );
    assert!(ev.touches(&p("/comps/main")));
    assert!(!ev.touches(&p("/comps/other")));
    assert_eq!(seen.lock().unwrap().len(), 4);
    assert!(stage.unsubscribe(sub));
    run(
        &reg,
        &mut stage,
        "user:a",
        "key.remove",
        json!({"path": title, "attr": "opacity", "time": 30}),
    );
    assert_eq!(seen.lock().unwrap().len(), 4);
}

#[test]
fn two_stages_editing_one_sublayer_converge() {
    let reg = CommandRegistry::with_builtins();
    let mut a = base_stage(&reg);
    let root = SublayerId::root();
    let snapshot = a.sublayer(&root).unwrap().export_snapshot().unwrap();
    let mut b = Stage::from_sublayers(vec![
        bae_model::Sublayer::from_snapshot(root.clone(), &snapshot).unwrap(),
    ])
    .unwrap();
    assert_eq!(trees_json(&a, &root), trees_json(&b, &root));
    let va = a.sublayer(&root).unwrap().version();
    let vb = b.sublayer(&root).unwrap().version();

    let title = "/comps/main/layers/title";
    // Concurrent edits: different attributes of the same prim, a keyframe, two new
    // layers with the same name, and a rename on one side.
    run(
        &reg,
        &mut a,
        "user:a",
        "attr.set",
        json!({"path": title, "attr": "scale", "value": {"type": "vec2", "value": [1.0, 2.0]}}),
    );
    run(
        &reg,
        &mut a,
        "user:a",
        "prim.create",
        json!({"path": "/comps/main/layers/dup", "prim_type": "Layer"}),
    );
    run(
        &reg,
        &mut b,
        "user:b",
        "key.set",
        json!({"path": title, "attr": "position", "time": 0, "keyframe": {"value": {"type": "vec2", "value": [0.0, 0.0]}}}),
    );
    run(
        &reg,
        &mut b,
        "user:b",
        "prim.create",
        json!({"path": "/comps/main/layers/dup", "prim_type": "Layer"}),
    );
    run(
        &reg,
        &mut b,
        "user:b",
        "prim.rename",
        json!({"path": "/comps/main/layers/title", "name": "headline"}),
    );

    let a_updates = a.sublayer(&root).unwrap().export_updates(&va).unwrap();
    let b_updates = b.sublayer(&root).unwrap().export_updates(&vb).unwrap();
    let ev_b = b.import_updates(&root, &a_updates).unwrap();
    let ev_a = a.import_updates(&root, &b_updates).unwrap();
    assert_eq!(ev_a.origin, EventOrigin::Remote);
    assert!(
        ev_a.changes
            .iter()
            .any(|c| matches!(c, Change::PrimMoved { to, .. } if to.name() == "headline"))
    );
    assert!(!ev_b.is_empty());

    assert_eq!(trees_json(&a, &root), trees_json(&b, &root));
    let headline = "/comps/main/layers/headline";
    for s in [&a, &b] {
        let prim = s.prim(&p(headline)).expect("rename merged");
        assert_eq!(prim.value("scale"), Some(&Value::Vec2([1.0, 2.0])));
        assert_eq!(prim.attr("position").unwrap().keyframes.len(), 1);
        assert_eq!(prim.value("opacity"), Some(&Value::Float(100.0)));
        // Both same-named layers survive under deterministic, distinct paths.
        let dups = s
            .children(&p("/comps/main/layers"))
            .into_iter()
            .filter(|c| c.name().starts_with("dup"))
            .count();
        assert_eq!(dups, 2);
    }
    // Local undo still works after a merge and only touches local changes.
    b.undo(&author("user:b")).unwrap();
    assert!(b.has_prim(&p(title)));
    assert_eq!(
        b.prim(&p(title)).unwrap().value("scale"),
        Some(&Value::Vec2([1.0, 2.0]))
    );
}

#[test]
fn registry_metadata_and_param_validation() {
    let reg = CommandRegistry::with_builtins();
    assert!(reg.len() >= 12);
    let info = reg.get("key.set").unwrap().info();
    let schema = &info.params_schema;
    for field in ["path", "attr", "time", "keyframe"] {
        assert!(
            schema["properties"].get(field).is_some(),
            "{field} missing in {schema}"
        );
    }
    assert!(serde_json::to_string(schema).unwrap().contains("flicks"));

    let mut stage = Stage::new();
    let err = reg
        .execute(
            &mut stage,
            &author("user:a"),
            "prim.create",
            json!({"path": "no-slash", "prim_type": "Comp"}),
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::InvalidParams { .. }), "{err}");
    let err = reg
        .execute(
            &mut stage,
            &author("user:a"),
            "prim.create",
            json!({"path": "/a/b", "prim_type": "Comp"}),
        )
        .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::NoSuchPrim(_))),
        "{err}"
    );
    assert!(matches!(
        reg.execute(&mut stage, &author("user:a"), "nope", json!({})),
        Err(CommandError::Unknown(_))
    ));
}

#[test]
fn plugin_commands_can_be_unregistered_by_owner() {
    struct Hello;
    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct HelloParams {}
    impl bae_model::Command for Hello {
        type Params = HelloParams;
        type Output = &'static str;
        const ID: &'static str = "com.example.hello";
        const TITLE: &'static str = "Hello";
        const DESCRIPTION: &'static str = "Says hello.";
        fn run(
            &self,
            _: &mut bae_model::Transaction<'_>,
            _: HelloParams,
        ) -> Result<&'static str, CommandError> {
            Ok("hello")
        }
    }
    let mut reg = CommandRegistry::with_builtins();
    let n = reg.len();
    reg.register_owned(Hello, Some("com.example")).unwrap();
    assert!(matches!(
        reg.register(Hello),
        Err(CommandError::Duplicate(_))
    ));
    let out = reg
        .execute(
            &mut Stage::new(),
            &author("user:a"),
            "com.example.hello",
            json!({}),
        )
        .unwrap();
    assert_eq!(out.output, json!("hello"));
    assert!(out.event.is_empty());
    assert_eq!(reg.unregister_owner("com.example"), 1);
    assert_eq!(reg.len(), n);
}

/// Acceptance: 10k prims open in < 500 ms; one attribute edit to event < 1 ms.
/// Timing asserts only run in optimized builds (`cargo test --release`).
#[test]
fn performance_budget() {
    let mut stage = Stage::new();
    let root = SublayerId::root();
    let n = 10_000;
    stage
        .transact(&AuthorId::system(), "build", |tx| {
            tx.apply(Op::CreatePrim {
                path: p("/comp"),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::COMP),
                index: None,
            })?;
            for i in 0..n {
                let path = p(&format!("/comp/l{i}"));
                tx.apply(Op::CreatePrim {
                    path: path.clone(),
                    specifier: Specifier::Def,
                    prim_type: Some(PrimType::LAYER),
                    index: None,
                })?;
                tx.apply(Op::SetValue {
                    path: path.clone(),
                    attr: "opacity".into(),
                    value: Some(Value::Float(100.0)),
                })?;
                tx.apply(Op::SetKeyframe {
                    path,
                    attr: "transform:position".into(),
                    time: Time(0),
                    keyframe: Keyframe::linear([0.0, 0.0]),
                })?;
            }
            Ok::<_, ModelError>(())
        })
        .unwrap();
    let snapshot = stage.sublayer(&root).unwrap().export_snapshot().unwrap();

    let t0 = Instant::now();
    let layer = bae_model::Sublayer::from_snapshot(root.clone(), &snapshot).unwrap();
    let open = t0.elapsed();
    assert_eq!(layer.len(), n + 1);
    let mut opened = Stage::from_sublayers(vec![layer]).unwrap();
    let fired = Arc::new(Mutex::new(None));
    let sink = fired.clone();
    opened.subscribe(move |_| *sink.lock().unwrap() = Some(Instant::now()));

    let rounds = 200;
    let mut total = std::time::Duration::ZERO;
    for i in 0..rounds {
        let t = Instant::now();
        opened
            .apply_edits(
                &AuthorId::system(),
                "edit",
                vec![bae_model::Edit::new(
                    root.clone(),
                    Op::SetValue {
                        path: p("/comp/l5000"),
                        attr: "opacity".into(),
                        value: Some(Value::Float(i as f64)),
                    },
                )],
            )
            .unwrap();
        total += fired.lock().unwrap().unwrap() - t;
    }
    let per_edit = total / rounds;
    eprintln!(
        "open {n} prims: {open:?} ({} KiB snapshot); edit→event: {per_edit:?}",
        snapshot.len() / 1024
    );
    if !cfg!(debug_assertions) {
        assert!(open.as_millis() < 500, "open took {open:?}");
        assert!(per_edit.as_micros() < 1000, "edit→event took {per_edit:?}");
    }
}

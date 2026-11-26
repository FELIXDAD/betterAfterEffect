//! A locked layer (`locked = true`) refuses every edit in every transaction,
//! whoever runs it (commands, agents, scripts): AE-PRJ-360.

use bae_model::{
    AuthorId, CommandError, CommandRegistry, ModelError, Op, PrimPath, Specifier, Stage,
    SublayerId, Value,
};
use serde_json::{Value as Json, json};

const LAYER: &str = "/comps/main/layers/title";

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn user() -> AuthorId {
    AuthorId::new("user:a").unwrap()
}

fn run(
    reg: &CommandRegistry,
    stage: &mut Stage,
    id: &str,
    params: Json,
) -> Result<(), CommandError> {
    reg.execute(stage, &user(), id, params).map(|_| ())
}

fn ok(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) {
    run(reg, stage, id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
}

fn set_locked(reg: &CommandRegistry, stage: &mut Stage, locked: bool) {
    ok(
        reg,
        stage,
        "attr.set",
        json!({"path": LAYER, "attr": "locked", "value": {"type": "bool", "value": locked}}),
    );
}

/// main (Comp) with two layers, `title` (named “标题”, with an effect) and
/// `bg`; `title` is locked.
fn locked_stage(reg: &CommandRegistry) -> Stage {
    let mut stage = Stage::new();
    for (path, t) in [
        ("/comps", "Folder"),
        ("/comps/main", "Comp"),
        ("/comps/main/layers", "Folder"),
        (LAYER, "Layer"),
        ("/comps/main/layers/title/effects", "Folder"),
        ("/comps/main/layers/title/effects/blur", "Effect"),
        ("/comps/main/layers/bg", "Layer"),
    ] {
        ok(
            reg,
            &mut stage,
            "prim.create",
            json!({"path": path, "prim_type": t}),
        );
    }
    ok(
        reg,
        &mut stage,
        "attr.set",
        json!({"path": LAYER, "attr": "name", "value": {"type": "string", "value": "标题"}}),
    );
    set_locked(reg, &mut stage, true);
    stage
}

fn assert_locked(result: Result<(), CommandError>, what: &str) {
    match result {
        Err(CommandError::Model(ModelError::LayerLocked { layer, name })) => {
            assert_eq!(layer, p(LAYER), "{what}");
            assert_eq!(name, "标题", "{what}");
        }
        other => panic!("{what}: expected LayerLocked, got {other:?}"),
    }
}

#[test]
fn every_edit_of_a_locked_layer_is_refused() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = locked_stage(&reg);
    let before =
        serde_json::to_value(stage.sublayer(&SublayerId::root()).unwrap().to_trees()).unwrap();
    let float = json!({"type": "float", "value": 50.0});
    let attempts = [
        (
            "attr.set opacity",
            "attr.set",
            json!({"path": LAYER, "attr": "opacity", "value": float}),
        ),
        (
            "attr.set name",
            "attr.set",
            json!({"path": LAYER, "attr": "name", "value": {"type": "string", "value": "改了"}}),
        ),
        (
            "attr.clear name",
            "attr.clear",
            json!({"path": LAYER, "attr": "name"}),
        ),
        (
            "key.set",
            "key.set",
            json!({"path": LAYER, "attr": "opacity", "time": 0, "keyframe": {"value": float}}),
        ),
        (
            "attr.set_expression",
            "attr.set_expression",
            json!({"path": LAYER, "attr": "opacity", "source": "50"}),
        ),
        (
            "effect property",
            "attr.set",
            json!({"path": "/comps/main/layers/title/effects/blur", "attr": "radius", "value": float}),
        ),
        (
            "new child",
            "prim.create",
            json!({"path": "/comps/main/layers/title/effects/glow", "prim_type": "Effect"}),
        ),
        (
            "delete effect",
            "prim.delete",
            json!({"path": "/comps/main/layers/title/effects/blur"}),
        ),
        ("prim.delete", "prim.delete", json!({"path": LAYER})),
        (
            "prim.rename",
            "prim.rename",
            json!({"path": LAYER, "name": "renamed"}),
        ),
        (
            "prim.move",
            "prim.move",
            json!({"from": LAYER, "to": "/comps/title"}),
        ),
    ];
    for (what, id, params) in attempts {
        assert_locked(run(&reg, &mut stage, id, params), what);
    }
    assert_eq!(
        serde_json::to_value(stage.sublayer(&SublayerId::root()).unwrap().to_trees()).unwrap(),
        before,
        "a refused edit changed the project"
    );
    let message = run(
        &reg,
        &mut stage,
        "attr.set",
        json!({"path": LAYER, "attr": "opacity", "value": float}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        message.contains("标题") && message.contains("locked; unlock it first"),
        "{message}"
    );
}

#[test]
fn the_lock_switch_other_layers_and_the_comp_stay_editable() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = locked_stage(&reg);
    for attr in ["enabled", "solo", "audio_enabled", "shy", "motion_blur"] {
        for value in [false, true] {
            ok(
                &reg,
                &mut stage,
                "attr.set",
                json!({"path": LAYER, "attr": attr, "value": {"type":"bool", "value":value}}),
            );
            assert!(stage.is_layer_locked(&p(LAYER)));
            assert_eq!(
                stage
                    .prim(&p(LAYER))
                    .unwrap()
                    .value(attr)
                    .and_then(bae_model::Value::as_bool),
                Some(value)
            );
        }
    }
    assert_locked(
        run(
            &reg,
            &mut stage,
            "attr.set",
            json!({"path": "/comps/main/layers/title/effects/blur", "attr":"enabled", "value":{"type":"bool","value":false}}),
        ),
        "child effect's switch",
    );
    let float = json!({"type": "float", "value": 50.0});
    ok(
        &reg,
        &mut stage,
        "attr.set",
        json!({"path": "/comps/main/layers/bg", "attr": "opacity", "value": float}),
    );
    ok(
        &reg,
        &mut stage,
        "attr.set",
        json!({"path": "/comps/main", "attr": "width", "value": {"type": "int", "value": 640}}),
    );
    // Another layer moves past it in the stack.
    ok(
        &reg,
        &mut stage,
        "prim.move",
        json!({"from": "/comps/main/layers/bg", "to": "/comps/main/layers/bg", "index": 0}),
    );
    assert_eq!(
        stage.children(&p("/comps/main/layers"))[1],
        p(LAYER),
        "reorder"
    );
    // Unlock, edit, lock again.
    set_locked(&reg, &mut stage, false);
    ok(
        &reg,
        &mut stage,
        "attr.set",
        json!({"path": LAYER, "attr": "opacity", "value": float}),
    );
    set_locked(&reg, &mut stage, true);
    assert!(stage.is_layer_locked(&p(LAYER)));
    // Deleting the whole comp takes its locked layers along (AE).
    ok(
        &reg,
        &mut stage,
        "prim.delete",
        json!({"path": "/comps/main"}),
    );
    assert!(!stage.has_prim(&p(LAYER)));
}

#[test]
fn the_lock_state_counts_from_the_first_touch_in_a_transaction() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = locked_stage(&reg);
    let opacity = |v: f64| Op::SetValue {
        path: p(LAYER),
        attr: "opacity".into(),
        value: Some(Value::Float(v)),
    };
    let locked = |v: bool| Op::SetValue {
        path: p(LAYER),
        attr: "locked".into(),
        value: Some(Value::Bool(v)),
    };
    // Unlock, edit, lock again: one step.
    stage
        .transact(&user(), "unlock and edit", |tx| {
            tx.apply(locked(false))?;
            tx.apply(opacity(10.0))?;
            tx.apply(locked(true))
        })
        .unwrap();
    // Locked when first touched: refused even after a lock-switch round trip.
    let err = stage
        .transact(&user(), "edit", |tx| {
            tx.apply(locked(true))?;
            tx.apply(opacity(20.0))
        })
        .unwrap_err();
    assert!(matches!(err, ModelError::LayerLocked { .. }), "{err}");
    // Unlocked when first touched: a command may edit and then lock it.
    set_locked(&reg, &mut stage, false);
    stage
        .transact(&user(), "edit and lock", |tx| {
            tx.apply(opacity(30.0))?;
            tx.apply(locked(true))?;
            tx.apply(opacity(40.0))
        })
        .unwrap();
    assert_eq!(
        stage.prim(&p(LAYER)).unwrap().value("opacity"),
        Some(&Value::Float(40.0))
    );
}

#[test]
fn copies_of_a_locked_layer_and_allowed_consequences() {
    let reg = CommandRegistry::with_builtins();
    let mut stage = locked_stage(&reg);
    let copy = p("/comps/main/layers/title_2");
    // Duplicate: the copy arrives locked and is set up in the same step.
    let tree = stage
        .sublayer(&SublayerId::root())
        .unwrap()
        .subtree(&p(LAYER))
        .expect("layer tree");
    stage
        .transact(&user(), "duplicate", |tx| {
            tx.apply(Op::RestorePrim {
                path: copy.clone(),
                tree,
                index: None,
            })?;
            tx.apply(Op::SetValue {
                path: copy.clone(),
                attr: "name".into(),
                value: Some(Value::String("标题 2".into())),
            })
        })
        .unwrap();
    assert!(stage.is_layer_locked(&copy));
    // An `over` makes room for the lock switch in a stronger sublayer.
    ok(&reg, &mut stage, "sublayer.create", json!({"id": "anim"}));
    ok(
        &reg,
        &mut stage,
        "sublayer.set_edit_target",
        json!({"id": "anim"}),
    );
    set_locked(&reg, &mut stage, false);
    set_locked(&reg, &mut stage, true);
    // `allow_locked` lets consequences through (unparenting after the
    // parent is deleted) and ends with its closure.
    stage
        .transact(&user(), "unparent", |tx| {
            tx.allow_locked(|tx| {
                tx.ensure_spec(&p(LAYER))?;
                tx.apply(Op::SetValue {
                    path: p(LAYER),
                    attr: "parent".into(),
                    value: None,
                })
            })?;
            assert!(!tx.locks_allowed());
            let refused = tx.apply(Op::CreatePrim {
                path: p("/comps/main/layers/title/masks"),
                specifier: Specifier::Def,
                prim_type: None,
                index: None,
            });
            assert!(matches!(refused, Err(ModelError::LayerLocked { .. })));
            Ok::<_, ModelError>(())
        })
        .unwrap();
    // Undo replays history: it can take back the lock and edits before it.
    stage.undo(&user()).unwrap();
    stage.undo(&user()).unwrap();
    assert!(!stage.is_layer_locked(&p(LAYER)));
}

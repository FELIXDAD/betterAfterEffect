use std::sync::{Arc, Mutex};

use bae_model::{
    AuthorId, Change, CommandError, CommandRegistry, ModelError, PrimPath, Stage, StageEvent,
    SublayerId, Value,
};
use serde_json::{Value as Json, json};

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn sl(s: &str) -> SublayerId {
    SublayerId::new(s).unwrap()
}

fn who() -> AuthorId {
    AuthorId::new("user:a").unwrap()
}

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: Json) -> StageEvent {
    reg.execute(stage, &who(), id, params)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
        .event
}

fn create(reg: &CommandRegistry, stage: &mut Stage, path: &str, t: &str) {
    run(
        reg,
        stage,
        "prim.create",
        json!({"path": path, "prim_type": t}),
    );
}

fn set(reg: &CommandRegistry, stage: &mut Stage, path: &str, attr: &str, v: Json) {
    run(
        reg,
        stage,
        "attr.set",
        json!({"path": path, "attr": attr, "value": v}),
    );
}

fn float(v: f64) -> Json {
    json!({"type": "float", "value": v})
}

fn val(stage: &Stage, path: &str, attr: &str) -> Option<Value> {
    stage.prim(&p(path))?.value(attr).cloned()
}

/// /assets/char (color red) / arm (opacity 100) / hand; /shots/s1/hero references /assets/char.
fn scene(reg: &CommandRegistry) -> Stage {
    let mut s = Stage::new();
    for (path, t) in [
        ("/assets", "Folder"),
        ("/assets/char", "Comp"),
        ("/assets/char/arm", "Layer"),
        ("/assets/char/arm/hand", "Layer"),
        ("/shots", "Folder"),
        ("/shots/s1", "Comp"),
        ("/shots/s1/hero", "Layer"),
    ] {
        create(reg, &mut s, path, t);
    }
    set(
        reg,
        &mut s,
        "/assets/char",
        "color",
        json!({"type": "color", "value": [1.0, 0.0, 0.0, 1.0]}),
    );
    set(reg, &mut s, "/assets/char/arm", "opacity", float(100.0));
    run(
        reg,
        &mut s,
        "prim.set_references",
        json!({"path": "/shots/s1/hero", "references": [{"prim": "/assets/char"}]}),
    );
    s
}

#[test]
fn referenced_subtree_composes_under_the_referencing_prim() {
    let reg = CommandRegistry::with_builtins();
    let mut s = scene(&reg);
    assert!(s.has_prim(&p("/shots/s1/hero/arm/hand")));
    assert_eq!(
        s.children(&p("/shots/s1/hero")),
        vec![p("/shots/s1/hero/arm")]
    );
    assert_eq!(
        val(&s, "/shots/s1/hero", "color"),
        Some(Value::Color([1.0, 0.0, 0.0, 1.0]))
    );
    let hero = s.prim(&p("/shots/s1/hero")).unwrap();
    let origin = hero.attr_origin("color").unwrap();
    assert_eq!(origin.path, p("/assets/char"));
    assert!(origin.is_referenced(&p("/shots/s1/hero")));
    // Local type opinion (Layer) is stronger than the referenced one (Comp).
    assert_eq!(hero.prim_type().unwrap().as_str(), "Layer");

    // A local override on referenced content wins and leaves the asset alone.
    set(&reg, &mut s, "/shots/s1/hero/arm", "opacity", float(50.0));
    assert_eq!(
        val(&s, "/shots/s1/hero/arm", "opacity"),
        Some(Value::Float(50.0))
    );
    assert_eq!(
        val(&s, "/assets/char/arm", "opacity"),
        Some(Value::Float(100.0))
    );
    let arm = s.prim(&p("/shots/s1/hero/arm")).unwrap();
    assert!(
        !arm.attr_origin("opacity")
            .unwrap()
            .is_referenced(&p("/shots/s1/hero/arm"))
    );

    // Referenced prims cannot be deleted or renamed through the referencing path.
    let err = reg
        .execute(
            &mut s,
            &who(),
            "prim.delete",
            json!({"path": "/shots/s1/hero/arm"}),
        )
        .unwrap_err();
    assert!(
        matches!(err, CommandError::Model(ModelError::ProvidedByReference(_))),
        "{err}"
    );

    // Flattened output resolves the reference and keeps the override.
    let flat = serde_json::to_value(s.flatten()).unwrap();
    let hero_tree = &flat[1]["children"][0]["children"][0];
    assert_eq!(hero_tree["name"], "hero");
    assert!(hero_tree.get("references").is_none());
    assert_eq!(hero_tree["attrs"]["color"]["value"]["type"], "color");
    assert_eq!(
        hero_tree["children"][0]["attrs"]["opacity"]["value"]["value"],
        50.0
    );
    assert_eq!(hero_tree["children"][0]["children"][0]["name"], "hand");

    // Dropping the reference removes the referenced children again.
    let ev = run(
        &reg,
        &mut s,
        "prim.set_references",
        json!({"path": "/shots/s1/hero", "references": []}),
    );
    assert!(ev.changes.contains(&Change::Resynced {
        path: p("/shots/s1/hero")
    }));
    assert!(!s.has_prim(&p("/shots/s1/hero/arm")));
    assert!(s.unresolved_references().is_empty());
}

#[test]
fn edits_to_the_target_are_reported_at_referencing_paths() {
    let reg = CommandRegistry::with_builtins();
    let mut s = scene(&reg);
    let seen: Arc<Mutex<Vec<StageEvent>>> = Arc::default();
    let sink = seen.clone();
    s.subscribe(move |e| sink.lock().unwrap().push(e.clone()));

    let ev = run(
        &reg,
        &mut s,
        "key.set",
        json!({"path": "/assets/char/arm", "attr": "rotation", "time": 0, "keyframe": {"value": float(0.0)}}),
    );
    let paths: Vec<&str> = ev.changes.iter().map(|c| c.path().as_str()).collect();
    assert!(paths.contains(&"/assets/char/arm"), "{paths:?}");
    assert!(paths.contains(&"/shots/s1/hero/arm"), "{paths:?}");
    assert_eq!(seen.lock().unwrap().last().unwrap(), &ev);

    // Deleting the target itself resyncs the referencing prim.
    let ev = run(&reg, &mut s, "prim.delete", json!({"path": "/assets/char"}));
    assert!(ev.changes.contains(&Change::Resynced {
        path: p("/shots/s1/hero")
    }));
    assert_eq!(
        s.unresolved_references(),
        vec![(
            p("/shots/s1/hero"),
            serde_json::from_value(json!({"prim": "/assets/char"})).unwrap()
        )]
    );
}

#[test]
fn nested_references_compose_and_cycles_terminate() {
    let reg = CommandRegistry::with_builtins();
    let mut s = scene(&reg);
    create(&reg, &mut s, "/rigs", "Folder");
    create(&reg, &mut s, "/rigs/arm", "ShapeGroup");
    create(&reg, &mut s, "/rigs/arm/bone", "Path");
    set(&reg, &mut s, "/rigs/arm/bone", "width", float(4.0));
    run(
        &reg,
        &mut s,
        "prim.set_references",
        json!({"path": "/assets/char/arm", "references": [{"prim": "/rigs/arm"}]}),
    );
    // Two levels deep: hero -> char, char/arm -> rigs/arm.
    assert_eq!(
        val(&s, "/shots/s1/hero/arm/bone", "width"),
        Some(Value::Float(4.0))
    );
    assert_eq!(
        s.children(&p("/shots/s1/hero/arm")),
        vec![p("/shots/s1/hero/arm/bone"), p("/shots/s1/hero/arm/hand")]
    );

    // A reference cycle: /rigs/arm -> /assets/char, which contains arm -> /rigs/arm.
    run(
        &reg,
        &mut s,
        "prim.set_references",
        json!({"path": "/rigs/arm", "references": [{"prim": "/assets/char"}]}),
    );
    assert!(s.has_prim(&p("/shots/s1/hero/arm/bone")));
    let _ = s.traverse().len().min(10_000);
    let _ = s.flatten();
}

#[test]
fn cross_project_references_resolve_against_attached_projects() {
    let reg = CommandRegistry::with_builtins();
    let mut lib = Stage::new();
    create(&reg, &mut lib, "/chars", "Folder");
    create(&reg, &mut lib, "/chars/robot", "Comp");
    create(&reg, &mut lib, "/chars/robot/head", "Layer");
    set(
        &reg,
        &mut lib,
        "/chars/robot/head",
        "scale",
        json!({"type": "vec2", "value": [2.0, 2.0]}),
    );

    let mut s = Stage::new();
    create(&reg, &mut s, "/shots", "Folder");
    create(&reg, &mut s, "/shots/robot", "Layer");
    run(
        &reg,
        &mut s,
        "prim.set_references",
        json!({"path": "/shots/robot", "references": [{"project": "../library.bae", "prim": "/chars/robot"}]}),
    );
    assert!(!s.has_prim(&p("/shots/robot/head")));
    assert_eq!(s.unresolved_references().len(), 1);

    let ev = s.attach_project("../library.bae", lib);
    assert!(ev.changes.contains(&Change::Resynced {
        path: PrimPath::root()
    }));
    assert_eq!(
        val(&s, "/shots/robot/head", "scale"),
        Some(Value::Vec2([2.0, 2.0]))
    );
    let head = s.prim(&p("/shots/robot/head")).unwrap();
    assert_eq!(
        head.attr_origin("scale").unwrap().project,
        Some("../library.bae")
    );
    assert!(s.unresolved_references().is_empty());

    // Overriding referenced content from another project works like any override.
    set(
        &reg,
        &mut s,
        "/shots/robot/head",
        "scale",
        json!({"type": "vec2", "value": [1.0, 1.0]}),
    );
    assert_eq!(
        val(&s, "/shots/robot/head", "scale"),
        Some(Value::Vec2([1.0, 1.0]))
    );
    assert_eq!(
        val(
            s.attached_project("../library.bae").unwrap(),
            "/chars/robot/head",
            "scale"
        ),
        Some(Value::Vec2([2.0, 2.0]))
    );
    s.detach_project("../library.bae").unwrap();
    assert!(!s.has_prim(&p("/shots/robot/head")));
}

#[test]
fn push_down_moves_overrides_into_a_weaker_sublayer() {
    let reg = CommandRegistry::with_builtins();
    let mut s = scene(&reg);
    run(&reg, &mut s, "sublayer.create", json!({"id": "anim"}));
    run(
        &reg,
        &mut s,
        "sublayer.set_edit_target",
        json!({"id": "anim"}),
    );
    set(&reg, &mut s, "/assets/char/arm", "opacity", float(30.0));
    set(&reg, &mut s, "/assets/char/arm", "rotation", float(45.0));
    let before = serde_json::to_value(s.flatten()).unwrap();

    run(
        &reg,
        &mut s,
        "opinion.move",
        json!({"path": "/assets/char/arm", "attr": "opacity", "from": "anim", "to": "root"}),
    );
    let arm = s.prim(&p("/assets/char/arm")).unwrap();
    assert_eq!(arm.attr_source("opacity"), Some(&SublayerId::root()));
    assert_eq!(arm.attr_source("rotation"), Some(&sl("anim")));
    assert_eq!(serde_json::to_value(s.flatten()).unwrap(), before);

    // Moving the rest empties the anim spec, and empty `over` ancestors are pruned.
    run(
        &reg,
        &mut s,
        "opinion.move",
        json!({"path": "/assets/char/arm", "from": "anim", "to": "root"}),
    );
    assert!(s.sublayer(&sl("anim")).unwrap().is_empty());
    assert_eq!(serde_json::to_value(s.flatten()).unwrap(), before);

    s.undo(&who()).unwrap();
    let anim = s.sublayer(&sl("anim")).unwrap();
    assert!(anim.attr(&p("/assets/char/arm"), "rotation").is_some());
    assert_eq!(serde_json::to_value(s.flatten()).unwrap(), before);
}

#[test]
fn merging_an_agent_sublayer_accepts_its_proposal() {
    let reg = CommandRegistry::with_builtins();
    let mut s = scene(&reg);
    run(
        &reg,
        &mut s,
        "sublayer.create",
        json!({"id": "agent/claude-01"}),
    );
    run(
        &reg,
        &mut s,
        "sublayer.set_edit_target",
        json!({"id": "agent/claude-01"}),
    );
    create(&reg, &mut s, "/shots/s1/title", "Layer");
    set(&reg, &mut s, "/shots/s1/title", "opacity", float(80.0));
    set(&reg, &mut s, "/assets/char/arm", "opacity", float(10.0));
    let proposal = serde_json::to_value(s.flatten()).unwrap();

    run(
        &reg,
        &mut s,
        "sublayer.merge",
        json!({"from": "agent/claude-01", "into": "root"}),
    );
    assert!(s.sublayer(&sl("agent/claude-01")).unwrap().is_empty());
    assert_eq!(serde_json::to_value(s.flatten()).unwrap(), proposal);
    let root = s.sublayer(&SublayerId::root()).unwrap();
    assert!(root.spec(&p("/shots/s1/title")).is_some());
    assert_eq!(
        root.attr(&p("/assets/char/arm"), "opacity").unwrap().value,
        Some(Value::Float(10.0))
    );

    // One undo step restores the proposal layer.
    s.undo(&who()).unwrap();
    assert!(!s.sublayer(&sl("agent/claude-01")).unwrap().is_empty());
    assert_eq!(
        s.sublayer(&SublayerId::root())
            .unwrap()
            .attr(&p("/assets/char/arm"), "opacity")
            .unwrap()
            .value,
        Some(Value::Float(100.0))
    );
    assert_eq!(serde_json::to_value(s.flatten()).unwrap(), proposal);
}

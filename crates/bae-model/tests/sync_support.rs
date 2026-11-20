//! Interfaces bae-sync relies on (requests from workstream 09).

use std::sync::{Arc, Mutex};

use bae_model::{
    AuthorId, Change, CommandRegistry, EventOrigin, ModelError, PrimPath, Stage, StageEvent,
    Sublayer, SublayerId,
};
use serde_json::json;

fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

fn run(reg: &CommandRegistry, stage: &mut Stage, id: &str, params: serde_json::Value) {
    reg.execute(stage, &AuthorId::new("user:a").unwrap(), id, params)
        .unwrap();
}

#[test]
fn batch_import_rebuilds_once_and_emits_one_event() {
    let reg = CommandRegistry::with_builtins();
    let root = SublayerId::root();
    let mut a = Stage::new();
    run(
        &reg,
        &mut a,
        "prim.create",
        json!({"path": "/comps", "prim_type": "Folder"}),
    );
    let base = a.sublayer(&root).unwrap().export_snapshot().unwrap();
    let mut b =
        Stage::from_sublayers(vec![Sublayer::from_snapshot(root.clone(), &base).unwrap()]).unwrap();

    // Three separate small updates from A, as a hub would deliver them.
    let mut blobs = Vec::new();
    for name in ["one", "two", "three"] {
        let v = a.sublayer(&root).unwrap().version();
        run(
            &reg,
            &mut a,
            "prim.create",
            json!({"path": format!("/comps/{name}"), "prim_type": "Comp"}),
        );
        blobs.push(a.sublayer(&root).unwrap().export_updates(&v).unwrap());
    }
    let events: Arc<Mutex<Vec<StageEvent>>> = Arc::default();
    let sink = events.clone();
    b.subscribe(move |e| sink.lock().unwrap().push(e.clone()));

    let ev = b.import_updates_batch(&root, &blobs).unwrap();
    assert_eq!(ev.origin, EventOrigin::Remote);
    assert_eq!(events.lock().unwrap().len(), 1);
    let created: Vec<&str> = ev
        .changes
        .iter()
        .filter_map(|c| match c {
            Change::PrimCreated { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 3, "{created:?}");
    assert_eq!(b.children(&p("/comps")).len(), 3);
}

#[test]
fn tree_nodes_map_to_paths_and_commits_carry_timestamps() {
    let reg = CommandRegistry::with_builtins();
    let mut s = Stage::new();
    run(
        &reg,
        &mut s,
        "prim.create",
        json!({"path": "/comps", "prim_type": "Folder"}),
    );
    run(
        &reg,
        &mut s,
        "prim.create",
        json!({"path": "/comps/main", "prim_type": "Comp"}),
    );
    let layer = s.sublayer(&SublayerId::root()).unwrap();
    let id = layer.node_id(&p("/comps/main")).unwrap();
    assert_eq!(layer.node_path(id), Some(&p("/comps/main")));

    run(
        &reg,
        &mut s,
        "prim.rename",
        json!({"path": "/comps/main", "name": "hero"}),
    );
    let layer = s.sublayer(&SublayerId::root()).unwrap();
    assert_eq!(
        layer.node_path(id),
        Some(&p("/comps/hero")),
        "node id is stable across renames"
    );

    let doc = layer.doc();
    let last = doc.oplog_frontiers().iter().next().expect("has history");
    let change = doc.get_change(last).expect("change meta");
    assert!(change.timestamp > 0, "commit timestamps are recorded");
}

#[test]
fn sublayer_ids_differing_only_in_case_are_rejected() {
    let mut s = Stage::new();
    s.create_sublayer(SublayerId::new("anim").unwrap(), 0)
        .unwrap();
    let err = s
        .create_sublayer(SublayerId::new("Anim").unwrap(), 0)
        .unwrap_err();
    assert!(matches!(err, ModelError::SublayerExists(_)), "{err}");
    let err = Stage::from_sublayers(vec![
        Sublayer::new(SublayerId::new("Root").unwrap()),
        Sublayer::new(SublayerId::root()),
    ])
    .unwrap_err();
    assert!(matches!(err, ModelError::SublayerExists(_)), "{err}");
}

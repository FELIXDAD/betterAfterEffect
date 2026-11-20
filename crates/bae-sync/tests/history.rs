//! Named versions, read-only views and per-prim differences.

mod common;

use std::time::Duration;

use bae_model::project::Project;
use bae_model::{
    AuthorId, Keyframe, Op, PrimType, Specifier, Stage, Sublayer, SublayerId, Time, Value,
};
use bae_sync::history::{diff, view_at};
use bae_sync::{
    ClientId, DirConfig, DirTransport, DocKey, DocSet, History, PrimChange, ProjectDocs,
    ProjectVersion, Relay, SessionConfig, SyncError, SyncSession,
};
use common::*;

fn edit(stage: &mut Stage, who: &AuthorId, layer: &SublayerId, op: Op) {
    stage.set_edit_target(layer).unwrap();
    stage
        .transact(who, "edit", |tx| tx.apply(op))
        .unwrap_or_else(|e| panic!("{e}"));
}

fn create(path: &str) -> Op {
    Op::CreatePrim {
        path: p(path),
        specifier: Specifier::Def,
        prim_type: Some(PrimType::new("Layer")),
        index: None,
    }
}

fn opacity(path: &str, v: f64) -> Op {
    Op::SetValue {
        path: p(path),
        attr: "opacity".into(),
        value: Some(Value::Float(v)),
    }
}

/// alice builds v1, bob changes things for v2.
fn two_versions() -> (Stage, History, AuthorId, AuthorId) {
    let mut stage = client_stage(&base_snapshots());
    let mut history = History::new();
    let (alice, bob) = (author("user:alice"), author("user:bob"));
    let root = SublayerId::root();
    edit(&mut stage, &alice, &root, create("/comps/main/layers/bg"));
    edit(
        &mut stage,
        &alice,
        &root,
        opacity("/comps/main/layers/bg", 50.0),
    );
    edit(&mut stage, &alice, &root, create("/comps/main/layers/old"));
    history
        .create_checkpoint(&stage, &alice, "v1", "first client review")
        .unwrap();

    edit(
        &mut stage,
        &bob,
        &root,
        opacity("/comps/main/layers/bg", 80.0),
    );
    edit(&mut stage, &bob, &root, create("/comps/main/layers/logo"));
    edit(
        &mut stage,
        &bob,
        &root,
        Op::DeletePrim {
            path: p("/comps/main/layers/old"),
        },
    );
    edit(
        &mut stage,
        &bob,
        &root,
        Op::MovePrim {
            from: p("/comps/main/layers/title"),
            to: p("/comps/main/layers/headline"),
            index: None,
        },
    );
    edit(
        &mut stage,
        &alice,
        &sl("anim/a"),
        Op::SetKeyframe {
            path: p("/comps/main"),
            attr: "zoom".into(),
            time: Time::ONE_SECOND,
            keyframe: Keyframe::linear(Value::Float(2.0)),
        },
    );
    std::thread::sleep(Duration::from_millis(2));
    history.create_checkpoint(&stage, &bob, "v2", "").unwrap();
    (stage, history, alice, bob)
}

#[test]
fn checkpoints_are_listed_renamed_saved_and_deleted() {
    let (stage, mut history, alice, bob) = two_versions();
    let cps = history.checkpoints();
    assert_eq!(
        cps.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["v1", "v2"]
    );
    assert_eq!(cps[0].author, alice);
    assert_eq!(cps[0].note, "first client review");
    assert_eq!(cps[0].layer_stack, stage.layer_stack());
    assert_eq!(cps[1].version, ProjectVersion::current(&stage));
    assert!(cps[0].created_ms > 0 && cps[0].created_ms <= cps[1].created_ms);

    history
        .rename_checkpoint(&cps[0].id, &bob, "给客户的第 1 版", Some("ok"))
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    history.save(tmp.path()).unwrap();
    let mut loaded = History::load(tmp.path()).unwrap();
    let v1 = loaded.checkpoint(&cps[0].id).unwrap();
    assert_eq!(
        (v1.name.as_str(), v1.note.as_str()),
        ("给客户的第 1 版", "ok")
    );
    assert_eq!(v1.version, cps[0].version);

    assert!(loaded.delete_checkpoint(&cps[1].id, &bob).unwrap());
    assert!(!loaded.delete_checkpoint(&cps[1].id, &bob).unwrap());
    assert_eq!(loaded.checkpoints().len(), 1);
    assert!(
        History::load(tmp.path().join("nowhere"))
            .unwrap()
            .checkpoints()
            .is_empty()
    );
}

#[test]
fn a_checkpoint_view_shows_the_project_as_it_was() {
    let (stage, history, ..) = two_versions();
    let v1 = &history.checkpoints()[0];
    let view = history.view(&stage, &v1.id).unwrap();
    let old = view.stage();
    assert!(old.prim(&p("/comps/main/layers/old")).is_some());
    assert!(old.prim(&p("/comps/main/layers/title")).is_some());
    assert!(old.prim(&p("/comps/main/layers/logo")).is_none());
    assert_eq!(
        old.prim(&p("/comps/main/layers/bg"))
            .unwrap()
            .value("opacity"),
        Some(&Value::Float(50.0))
    );
    assert!(view.missing().is_empty());
    // The live stage is untouched.
    assert_eq!(
        stage
            .prim(&p("/comps/main/layers/bg"))
            .unwrap()
            .value("opacity"),
        Some(&Value::Float(80.0))
    );
    // Any version works, not only named ones: the empty project.
    let empty = view_at(&stage, &ProjectVersion::default(), None).unwrap();
    assert!(empty.stage().prim(&p("/comps")).is_none());
}

#[test]
fn diff_lists_prims_with_authors() {
    let (stage, history, alice, bob) = two_versions();
    let cps = history.checkpoints();
    let d = history.diff(&stage, &cps[0].id, Some(&cps[1].id)).unwrap();
    let find = |path: &str| {
        d.iter()
            .find(|x| x.path == p(path))
            .unwrap_or_else(|| panic!("{path} not in {d:#?}"))
    };
    let bg = find("/comps/main/layers/bg");
    assert_eq!(bg.change, PrimChange::Modified);
    assert_eq!(bg.attrs, ["opacity"]);
    assert_eq!(bg.authors, std::slice::from_ref(&bob));
    assert!(bg.last_changed > 0, "commit timestamps are recorded");

    assert_eq!(find("/comps/main/layers/logo").change, PrimChange::Added);
    let old = find("/comps/main/layers/old");
    assert_eq!(old.change, PrimChange::Removed);
    assert_eq!(old.authors, std::slice::from_ref(&bob));
    let headline = find("/comps/main/layers/headline");
    assert_eq!(
        headline.change,
        PrimChange::Moved {
            from: p("/comps/main/layers/title")
        }
    );
    assert_eq!(headline.authors, std::slice::from_ref(&bob));

    let comp = d
        .iter()
        .find(|x| x.sublayer == sl("anim/a") && x.path == p("/comps/main"))
        .unwrap();
    assert_eq!(comp.attrs, ["zoom"]);
    assert_eq!(comp.authors, std::slice::from_ref(&alice));
    // Untouched prims are not listed.
    assert!(d.iter().all(|x| x.path != p("/comps")));
    // Strongest sublayer first.
    assert_eq!(d[0].sublayer, sl("anim/a"));

    // To the current stage, and backwards.
    let mut stage = stage;
    edit(
        &mut stage,
        &alice,
        &SublayerId::root(),
        opacity("/comps/main/layers/logo", 10.0),
    );
    let now = history.diff(&stage, &cps[1].id, None).unwrap();
    assert_eq!(now.len(), 1);
    assert_eq!(now[0].authors, std::slice::from_ref(&alice));
    let back = diff(&stage, &cps[1].version, &cps[0].version).unwrap();
    assert_eq!(
        back.iter()
            .find(|x| x.path == p("/comps/main/layers/logo"))
            .unwrap()
            .change,
        PrimChange::Removed
    );
}

#[test]
fn checkpoints_sync_with_the_project() {
    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let mut stages = [client_stage(&base), client_stage(&base)];
    let mut histories = [History::new(), History::new()];
    let mut sessions = [0, 1]
        .map(|i| SyncSession::new(SessionConfig::new(ClientId::new(&format!("h{i}")).unwrap())));
    let mut links: Vec<_> = (0..2)
        .map(|_| {
            let (a, b) = bae_sync::MemoryTransport::pair();
            relay.connect(Box::new(b));
            a
        })
        .collect();
    let alice = author("user:alice");
    edit(
        &mut stages[0],
        &alice,
        &SublayerId::root(),
        create("/comps/main/layers/a"),
    );
    let cp = histories[0]
        .create_checkpoint(&stages[0], &alice, "A", "")
        .unwrap();

    // Before syncing, the other side cannot see a version it does not have.
    let mut early = History::from_snapshot(&histories[0].export_snapshot().unwrap()).unwrap();
    assert!(matches!(
        early.view(&stages[1], &cp.id),
        Err(SyncError::VersionNotAvailable(_))
    ));
    early.delete_checkpoint(&cp.id, &alice).unwrap();

    for _ in 0..4 {
        for i in 0..2 {
            let mut docs = ProjectDocs::new(&mut stages[i], &mut histories[i]);
            sessions[i].pump(&mut docs, &mut links[i]).unwrap();
        }
        relay.pump();
    }
    let seen = histories[1].checkpoint(&cp.id).expect("synced");
    assert_eq!(seen.name, "A");
    let view = histories[1].view(&stages[1], &cp.id).unwrap();
    assert!(view.stage().prim(&p("/comps/main/layers/a")).is_some());
    assert!(relay.docs().version(&DocKey::History).is_some());
}

#[test]
fn checkpoints_travel_through_the_shared_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    Project::from_stage(client_stage(&base_snapshots()))
        .save_as(&dir)
        .unwrap();
    let config = DirConfig {
        poll_interval: Duration::ZERO,
        watch: false,
        ..DirConfig::default()
    };
    let open = |id: &str| {
        let mut sc = SessionConfig::new(ClientId::new(id).unwrap());
        sc.hello_interval = None;
        (
            Project::open(&dir).unwrap().stage,
            History::load(&dir).unwrap(),
            SyncSession::new(sc),
            DirTransport::open(&dir, ClientId::new(id).unwrap(), config.clone()).unwrap(),
        )
    };
    let (mut sa, mut ha, mut a, mut ta) = open("a");
    let (mut sb, mut hb, mut b, mut tb) = open("b");
    let alice = author("user:alice");
    let pump_all = |sa: &mut Stage,
                    ha: &mut History,
                    a: &mut SyncSession,
                    ta: &mut DirTransport,
                    sb: &mut Stage,
                    hb: &mut History,
                    b: &mut SyncSession,
                    tb: &mut DirTransport| {
        for _ in 0..3 {
            a.pump(&mut ProjectDocs::new(sa, ha), ta).unwrap();
            b.pump(&mut ProjectDocs::new(sb, hb), tb).unwrap();
        }
    };
    pump_all(
        &mut sa, &mut ha, &mut a, &mut ta, &mut sb, &mut hb, &mut b, &mut tb,
    );
    edit(
        &mut sa,
        &alice,
        &SublayerId::root(),
        create("/comps/main/layers/x"),
    );
    let cp = ha.create_checkpoint(&sa, &alice, "folder", "").unwrap();
    pump_all(
        &mut sa, &mut ha, &mut a, &mut ta, &mut sb, &mut hb, &mut b, &mut tb,
    );
    assert!(dir.join("updates/@history/a.log").is_file());
    assert_eq!(hb.checkpoint(&cp.id).unwrap().name, "folder");
    assert!(
        hb.view(&sb, &cp.id)
            .unwrap()
            .stage()
            .prim(&p("/comps/main/layers/x"))
            .is_some()
    );

    // A saved history.loro is picked up by a client that joins later.
    ha.save(&dir).unwrap();
    drop((ta, tb));
    let (mut sc, mut hc, mut c, mut tc) = open("c");
    for _ in 0..3 {
        c.pump(&mut ProjectDocs::new(&mut sc, &mut hc), &mut tc)
            .unwrap();
    }
    assert!(hc.checkpoint(&cp.id).is_some());
    // An unrelated sublayer added later does not disturb views.
    sc.insert_sublayer(Sublayer::new(sl("extra")), 0).unwrap();
    let view = hc.view(&sc, &cp.id).unwrap();
    assert_eq!(view.stage().layer_stack().len(), 2);
}

/// Every spec of every sublayer, by path (order-independent).
fn specs(stage: &Stage) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for layer in stage.sublayers() {
        for path in layer.paths() {
            out.push((
                layer.id().to_string(),
                path.to_string(),
                serde_json::to_string(layer.spec(path).unwrap()).unwrap(),
            ));
        }
    }
    out.sort();
    out
}

#[test]
fn revert_to_a_named_version_is_a_new_undoable_step() {
    let (mut stage, history, alice, bob) = two_versions();
    let cps = history.checkpoints();
    let then = specs(history.view(&stage, &cps[0].id).unwrap().stage());
    let before = specs(&stage);
    let version_before = ProjectVersion::current(&stage);
    let bob_undo = stage.undo_label(&bob).map(str::to_owned);

    let ev = history
        .revert_to(
            &mut stage,
            &alice,
            &cps[0].id,
            &bae_sync::RevertScope::Project,
        )
        .unwrap();
    assert!(!ev.is_empty());
    assert_eq!(specs(&stage), then, "content is back to v1");
    assert_eq!(
        fingerprint(&stage),
        fingerprint(history.view(&stage, &cps[0].id).unwrap().stage()),
        "child order too"
    );
    assert!(history.diff(&stage, &cps[0].id, None).unwrap().is_empty());
    // History is not rewritten: the rollback is new ops on top, attributed to alice.
    let now = ProjectVersion::current(&stage);
    for (id, vv) in &version_before.sublayers {
        assert!(now.get(id).includes_vv(vv));
    }
    let d = history.diff(&stage, &cps[1].id, None).unwrap();
    assert!(d.iter().all(|x| x.authors == std::slice::from_ref(&alice)));
    assert_eq!(stage.undo_label(&alice), Some("revert to v1"));

    // One undo brings v2's content back; bob's undo stack was never touched.
    stage.undo(&alice).unwrap();
    assert_eq!(specs(&stage), before);
    assert_eq!(stage.undo_label(&bob).map(str::to_owned), bob_undo);
    assert!(bob_undo.is_some());
}

#[test]
fn revert_can_be_limited_to_one_prim() {
    let (mut stage, history, alice, _) = two_versions();
    let v1 = history.checkpoints()[0].id.clone();
    history
        .revert_to(
            &mut stage,
            &alice,
            &v1,
            &bae_sync::RevertScope::Prim(p("/comps/main/layers/bg")),
        )
        .unwrap();
    assert_eq!(
        stage
            .prim(&p("/comps/main/layers/bg"))
            .unwrap()
            .value("opacity"),
        Some(&Value::Float(50.0))
    );
    // Everything else keeps v2's changes.
    assert!(stage.prim(&p("/comps/main/layers/logo")).is_some());
    assert!(stage.prim(&p("/comps/main/layers/headline")).is_some());

    // A whole sublayer: only anim/a goes back.
    history
        .revert_to(
            &mut stage,
            &alice,
            &v1,
            &bae_sync::RevertScope::Sublayer(sl("anim/a")),
        )
        .unwrap();
    assert!(
        stage
            .prim(&p("/comps/main"))
            .unwrap()
            .attr("zoom")
            .is_none()
    );
    assert!(stage.prim(&p("/comps/main/layers/logo")).is_some());
}

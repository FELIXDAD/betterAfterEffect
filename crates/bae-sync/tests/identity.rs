//! History differences and rollback match prims by Loro tree node, not by mirror
//! path (`docs/spec/sync.md` §5.3, §5.4).
//!
//! Mirror paths are not stable: after concurrent creation of the same name the
//! `_<peer>_<counter>` suffix is only applied when a mirror is rebuilt, while
//! local structural edits patch it in place. In the reproduction used here, `a`
//! and `b` both create `…/shared`, exchange everything (both show `shared` and
//! `shared_<peer>_<counter>`), then `a` deletes `…/shared` and `b` imports it:
//! the version vectors are equal, but `a` still shows the other prim as
//! `shared_<peer>_<counter>` while `b` shows it as `shared`.

mod common;

use bae_model::loro::TreeID;
use bae_model::{AuthorId, Op, PrimPath, PrimSpec, PrimType, Specifier, Stage, SublayerId, Value};
use bae_sync::history::{diff, revert, view_at};
use bae_sync::{PrimChange, PrimDiff, ProjectVersion, RevertScope};
use common::*;

fn root() -> SublayerId {
    SublayerId::root()
}

fn layer(name: &str) -> PrimPath {
    p(&format!("{LAYERS}/{name}"))
}

fn apply(stage: &mut Stage, who: &AuthorId, op: Op) {
    stage.set_edit_target(&root()).unwrap();
    stage
        .transact(who, "edit", |tx| tx.apply(op))
        .unwrap_or_else(|e| panic!("{e}"));
}

fn create_typed(stage: &mut Stage, who: &AuthorId, path: PrimPath, t: &'static str) {
    apply(
        stage,
        who,
        Op::CreatePrim {
            path,
            specifier: Specifier::Def,
            prim_type: Some(PrimType::new(t)),
            index: None,
        },
    );
}

fn create(stage: &mut Stage, who: &AuthorId, name: &str) {
    create_typed(stage, who, layer(name), "Layer");
}

fn set_opacity(stage: &mut Stage, who: &AuthorId, path: PrimPath, v: f64) {
    apply(
        stage,
        who,
        Op::SetValue {
            path,
            attr: "opacity".into(),
            value: Some(Value::Float(v)),
        },
    );
}

fn rename(stage: &mut Stage, who: &AuthorId, from: PrimPath, to: PrimPath) {
    apply(
        stage,
        who,
        Op::MovePrim {
            from,
            to,
            index: None,
        },
    );
}

fn root_layer(stage: &Stage) -> &bae_model::Sublayer {
    stage.sublayer(&root()).unwrap()
}

fn node(stage: &Stage, path: &PrimPath) -> TreeID {
    root_layer(stage)
        .node_id(path)
        .unwrap_or_else(|| panic!("no prim at {path}"))
}

/// Children of `…/layers` in the root sublayer, as this client's mirror shows them.
fn names(stage: &Stage) -> Vec<String> {
    root_layer(stage)
        .children(&p(LAYERS))
        .iter()
        .map(|c| c.name().to_owned())
        .collect()
}

/// Spec of a tree node, wherever this client's mirror shows it.
fn spec_of(stage: &Stage, n: TreeID) -> Option<PrimSpec> {
    let l = root_layer(stage);
    l.spec(l.node_path(n)?).cloned()
}

fn opacity_of(spec: &PrimSpec) -> Option<&Value> {
    spec.attrs.get("opacity")?.value.as_ref()
}

/// A stage with the same documents and a freshly rebuilt mirror.
fn clone_stage(stage: &Stage) -> Stage {
    let snaps: Vec<_> = stage
        .layer_stack()
        .iter()
        .map(|id| {
            let s = stage.sublayer(id).unwrap();
            (id.clone(), s.export_snapshot().unwrap())
        })
        .collect();
    client_stage(&snaps)
}

/// What the documents hold, independent of how stale a mirror is.
fn content(stage: &Stage) -> String {
    fingerprint(&clone_stage(stage))
}

/// Every stage imports the ops it lacks from every other. Importing rebuilds
/// the importer's mirror; a client that receives nothing keeps its own.
fn sync(stages: &mut [&mut Stage]) {
    let ids = stages[0].layer_stack().to_vec();
    for i in 0..stages.len() {
        for j in 0..stages.len() {
            if i == j {
                continue;
            }
            for id in &ids {
                let have = stages[i].sublayer(id).unwrap().version();
                let theirs = stages[j].sublayer(id).unwrap();
                if !have.includes_vv(&theirs.version()) {
                    let bytes = theirs.export_updates(&have).unwrap();
                    stages[i].import_updates(id, &bytes).unwrap();
                }
            }
        }
    }
}

struct Collision {
    a: Stage,
    b: Stage,
    wa: AuthorId,
    wb: AuthorId,
    /// Both `shared` prims exist.
    v1: ProjectVersion,
    /// The prim both mirrors showed as `shared` at v1 (`a` deletes it).
    x: TreeID,
    /// The other one (`shared_<peer>_<counter>` at v1).
    y: TreeID,
}

/// `a` and `b` both create `…/shared` (opacity 10 and 20) and exchange
/// everything; both mirrors are rebuilt the same way.
fn collision() -> Collision {
    let base = base_snapshots();
    let (wa, wb) = (author("user:a"), author("user:b"));
    let (mut a, mut b) = (client_stage(&base), client_stage(&base));
    create(&mut a, &wa, "shared");
    set_opacity(&mut a, &wa, layer("shared"), 10.0);
    create(&mut b, &wb, "shared");
    set_opacity(&mut b, &wb, layer("shared"), 20.0);
    sync(&mut [&mut a, &mut b]);
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert_eq!(names(&a).len(), 3, "{:?}", names(&a));
    let x = node(&a, &layer("shared"));
    let y = node(&a, &layer(&names(&a)[2]));
    assert_ne!(x, y);
    let v1 = ProjectVersion::current(&a);
    Collision {
        a,
        b,
        wa,
        wb,
        v1,
        x,
        y,
    }
}

/// The reproduction: `a` deletes `…/shared` (x) and `b` imports it. Both hold
/// the same versions; `a` may still show y as `shared_<peer>_<counter>`.
fn diverged() -> Collision {
    let mut c = collision();
    apply(
        &mut c.a,
        &c.wa,
        Op::DeletePrim {
            path: layer("shared"),
        },
    );
    sync(&mut [&mut c.a, &mut c.b]);
    assert_eq!(ProjectVersion::current(&c.a), ProjectVersion::current(&c.b));
    assert_eq!(names(&c.b), ["title", "shared"]);
    assert_eq!(node(&c.b, &layer("shared")), c.y);
    // Not asserted, since it is a bae-model defect that may get fixed: `a`
    // still shows ["title", "shared_<peer>_<counter>"] here.
    eprintln!("a shows {:?}, b shows {:?}", names(&c.a), names(&c.b));
    c
}

fn entry(path: PrimPath, change: PrimChange, attrs: &[&str], authors: &[&AuthorId]) -> PrimDiff {
    PrimDiff {
        sublayer: root(),
        path,
        change,
        attrs: attrs.iter().map(|s| (*s).to_owned()).collect(),
        fields: Vec::new(),
        authors: authors.iter().map(|a| (*a).clone()).collect(),
        last_changed: 0,
    }
}

/// The diff without commit times (compared separately where it matters).
fn timeless(mut d: Vec<PrimDiff>) -> Vec<PrimDiff> {
    for x in &mut d {
        x.last_changed = 0;
    }
    d
}

#[test]
fn diff_ignores_collision_suffixes_between_clients() {
    let c = diverged();
    let now = ProjectVersion::current(&c.b);
    for (who, stage) in [("b", &c.b), ("a", &c.a)] {
        // Only x is gone; y is the same prim although its path differs from v1's
        // (and, on `a`, from `b`'s).
        let d = diff(stage, &c.v1, &now).unwrap();
        assert_eq!(
            timeless(d.clone()),
            [entry(
                layer("shared"),
                PrimChange::Removed,
                &["opacity"],
                &[&c.wa]
            )],
            "on {who}"
        );
        assert!(d[0].last_changed > 0);
        assert_eq!(
            timeless(diff(stage, &now, &c.v1).unwrap()),
            [entry(
                layer("shared"),
                PrimChange::Added,
                &["opacity"],
                &[&c.wa]
            )],
            "backwards on {who}"
        );
    }
}

#[test]
fn diff_ignores_collision_suffixes_between_versions() {
    let mut c = diverged();
    // y changes; it was `shared_<peer>_<counter>` at v1 and is `shared` at v3.
    set_opacity(&mut c.b, &c.wb, layer("shared"), 30.0);
    let v3 = ProjectVersion::current(&c.b);
    create(&mut c.b, &c.wb, "later");
    let d = timeless(diff(&c.b, &c.v1, &v3).unwrap());
    assert_eq!(
        d,
        [
            entry(
                layer("shared"),
                PrimChange::Modified,
                &["opacity"],
                &[&c.wb]
            ),
            entry(layer("shared"), PrimChange::Removed, &["opacity"], &[&c.wa]),
        ]
    );
}

#[test]
fn revert_restores_the_deleted_prim_and_leaves_its_namesake_alone() {
    for reverter in ["b", "a (stale mirror)"] {
        let mut c = diverged();
        let x_spec = spec_of(&clone_stage(&c.a), c.x);
        assert!(x_spec.is_none(), "x is deleted");
        let x_then = view_at(&c.a, &c.v1, None).unwrap();
        let x_then = spec_of(x_then.stage(), c.x).unwrap();
        let (stage, who) = if reverter == "b" {
            (&mut c.b, &c.wb)
        } else {
            (&mut c.a, &c.wa)
        };
        let y_before = spec_of(stage, c.y).unwrap();
        let y_path = root_layer(stage).node_path(c.y).unwrap().clone();
        revert(stage, who, "revert", &c.v1, &RevertScope::Project).unwrap();

        // y: same node, same path, same spec.
        assert_eq!(spec_of(stage, c.y).as_ref(), Some(&y_before), "{reverter}");
        assert_eq!(node(stage, &y_path), c.y, "{reverter}");
        // x: re-created (a new node) with its own content.
        let added: Vec<String> = names(stage)
            .into_iter()
            .filter(|n| n != "title" && node(stage, &layer(n)) != c.y)
            .collect();
        assert_eq!(added.len(), 1, "{reverter}: {:?}", names(stage));
        let spec = root_layer(stage).spec(&layer(&added[0])).unwrap();
        assert_eq!(opacity_of(spec), opacity_of(&x_then), "{reverter}");
        assert_eq!(spec, &x_then, "{reverter}");
        // Under a name no remaining sibling has (y's raw name is `shared`).
        assert_eq!(added[0], "shared_2", "{reverter}");
        // Nothing else differs from v1.
        let d = diff(stage, &c.v1, &ProjectVersion::current(stage)).unwrap();
        let summary: Vec<_> = d.iter().map(|x| (x.path.name(), &x.change)).collect();
        assert_eq!(
            summary,
            [
                ("shared", &PrimChange::Removed),
                ("shared_2", &PrimChange::Added)
            ],
            "{reverter}"
        );
    }
}

#[test]
fn revert_gives_the_same_result_on_every_client() {
    // One state, two worlds: in the first the client with the stale mirror
    // reverts, in the second the other one. Fresh copies stand in for the
    // client that only receives. (The stale mirror itself is only rebuilt by a
    // later import, so its content is compared through a fresh copy.)
    let c = diverged();
    let y_then = spec_of(&c.b, c.y).map(|s| serde_json::to_string(&s).unwrap());
    let x_then = spec_of(view_at(&c.b, &c.v1, None).unwrap().stage(), c.x);
    assert!(x_then.is_some());
    let mut results = Vec::new();
    let (mut a, mut b) = (c.a, c.b);
    let (mut a2, mut b2) = (clone_stage(&a), clone_stage(&b));
    let who = author("user:reverter");
    // The same content, and y (which both worlds share) is the same prim.
    let result = |stage: &Stage, mirror: String| {
        let fresh = clone_stage(stage);
        let y = spec_of(&fresh, c.y).map(|s| serde_json::to_string(&s).unwrap());
        (content(stage), mirror, y)
    };

    revert(&mut a, &who, "revert", &c.v1, &RevertScope::Project).unwrap();
    sync(&mut [&mut a, &mut b2]);
    results.push(("stale a reverts: a", result(&a, content(&a))));
    results.push(("stale a reverts: b", result(&b2, fingerprint(&b2))));

    revert(&mut b, &who, "revert", &c.v1, &RevertScope::Project).unwrap();
    sync(&mut [&mut b, &mut a2]);
    results.push(("b reverts: b", result(&b, fingerprint(&b))));
    results.push(("b reverts: a", result(&a2, fingerprint(&a2))));

    for (what, r) in &results {
        assert_eq!(r, &results[0].1, "{what} differs from {}", results[0].0);
    }
    assert_eq!(results[0].1.2, y_then, "y is left alone");
    // x is back next to it.
    let x_now = root_layer(&a2).spec(&layer("shared_2")).cloned();
    assert_eq!(x_now, x_then);
}

#[test]
fn undo_of_delete_still_diffs_as_modified() {
    let mut stage = client_stage(&base_snapshots());
    let w = author("user:a");
    create_typed(&mut stage, &w, layer("grp"), "Folder");
    create_typed(&mut stage, &w, layer("grp/x"), "Layer");
    set_opacity(&mut stage, &w, layer("grp/x"), 10.0);
    let v1 = ProjectVersion::current(&stage);
    let x = node(&stage, &layer("grp/x"));

    set_opacity(&mut stage, &w, layer("grp/x"), 20.0);
    apply(
        &mut stage,
        &w,
        Op::DeletePrim {
            path: layer("grp/x"),
        },
    );
    stage.undo(&w).unwrap();
    // Undo re-created the prim: a new tree node.
    assert_ne!(node(&stage, &layer("grp/x")), x);
    let now = ProjectVersion::current(&stage);
    assert_eq!(
        timeless(diff(&stage, &v1, &now).unwrap()),
        [entry(
            layer("grp/x"),
            PrimChange::Modified,
            &["opacity"],
            &[&w]
        )]
    );

    // Still the same prim when its path no longer matches (the folder was
    // renamed since).
    rename(&mut stage, &w, layer("grp"), layer("grp2"));
    let v2 = ProjectVersion::current(&stage);
    assert_eq!(
        timeless(diff(&stage, &v1, &v2).unwrap()),
        [
            entry(
                layer("grp2"),
                PrimChange::Moved { from: layer("grp") },
                &[],
                &[&w]
            ),
            entry(layer("grp2/x"), PrimChange::Modified, &["opacity"], &[&w]),
        ]
    );
    let back = timeless(diff(&stage, &v2, &v1).unwrap());
    assert_eq!(
        back.iter()
            .map(|d| (&d.path, &d.change))
            .collect::<Vec<_>>(),
        [
            (
                &layer("grp"),
                &PrimChange::Moved {
                    from: layer("grp2")
                }
            ),
            (&layer("grp/x"), &PrimChange::Modified),
        ]
    );

    // Rolling back puts the folder back; nothing differs from v1 afterwards.
    revert(&mut stage, &w, "revert", &v1, &RevertScope::Project).unwrap();
    assert!(
        diff(&stage, &v1, &ProjectVersion::current(&stage))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn real_moves_and_renames_are_still_reported() {
    let mut stage = client_stage(&base_snapshots());
    let w = author("user:a");
    for n in ["a", "b", "c", "d"] {
        create(&mut stage, &w, n);
    }
    create_typed(&mut stage, &w, layer("grp"), "Folder");
    create(&mut stage, &w, "grp/child");
    let v1 = ProjectVersion::current(&stage);

    // Rename a and reuse its name.
    rename(&mut stage, &w, layer("a"), layer("a_old"));
    create(&mut stage, &w, "a");
    // Move b into the folder (renamed below) and reuse its path.
    rename(&mut stage, &w, layer("b"), layer("grp/b"));
    create(&mut stage, &w, "b");
    // Swap c and d.
    rename(&mut stage, &w, layer("c"), layer("tmp"));
    rename(&mut stage, &w, layer("d"), layer("c"));
    rename(&mut stage, &w, layer("tmp"), layer("d"));
    // Rename the folder: its unchanged child is not reported.
    rename(&mut stage, &w, layer("grp"), layer("grp2"));
    let v2 = ProjectVersion::current(&stage);

    let d = timeless(diff(&stage, &v1, &v2).unwrap());
    let moved = |from: &str| PrimChange::Moved { from: layer(from) };
    assert_eq!(
        d,
        [
            entry(layer("a"), PrimChange::Added, &[], &[&w]),
            entry(layer("a_old"), moved("a"), &[], &[&w]),
            entry(layer("b"), PrimChange::Added, &[], &[&w]),
            entry(layer("c"), moved("d"), &[], &[&w]),
            entry(layer("d"), moved("c"), &[], &[&w]),
            entry(layer("grp2"), moved("grp"), &[], &[&w]),
            entry(layer("grp2/b"), moved("b"), &[], &[&w]),
        ]
    );

    // Rolling back restores names and places.
    revert(&mut stage, &w, "revert", &v1, &RevertScope::Project).unwrap();
    assert_eq!(
        fingerprint(&stage),
        fingerprint(view_at(&stage, &v1, None).unwrap().stage())
    );
    assert!(
        diff(&stage, &v1, &ProjectVersion::current(&stage))
            .unwrap()
            .is_empty()
    );
}

// ---- property test: random edits with colliding names ----

/// One random edit under `…/layers` of the root sublayer. Names come from a small
/// pool so concurrent creates and renames collide. Edits that no longer apply
/// are ignored.
fn random_edit(stage: &mut Stage, who: &AuthorId, rng: &mut Rng, n: &mut u32) {
    const POOL: [&str; 4] = ["shared", "shared", "shared", "other"];
    stage.set_edit_target(&root()).unwrap();
    let children = root_layer(stage).children(&p(LAYERS));
    let pick = |rng: &mut Rng| children[rng.below(children.len())].clone();
    *n += 1;
    let roll = rng.below(100);
    let op = if roll < 45 || children.is_empty() {
        Op::CreatePrim {
            path: layer(POOL[rng.below(POOL.len())]),
            specifier: Specifier::Def,
            prim_type: Some(PrimType::new("Layer")),
            index: None,
        }
    } else if roll < 65 {
        let to = if rng.chance(80) {
            POOL[rng.below(POOL.len())].to_owned()
        } else {
            format!("r{n}")
        };
        Op::MovePrim {
            from: pick(rng),
            to: layer(&to),
            index: None,
        }
    } else if roll < 80 {
        Op::DeletePrim { path: pick(rng) }
    } else if roll < 90 {
        Op::SetValue {
            path: pick(rng),
            attr: "opacity".into(),
            value: Some(Value::Float(rng.below(101) as f64)),
        }
    } else {
        let _ = stage.undo(who);
        return;
    };
    let _ = stage.transact(who, "edit", |tx| tx.apply(op));
}

/// A diff in terms of tree nodes instead of paths, sorted, so that clients whose
/// mirrors show different paths can be compared.
fn by_node(
    stage: &Stage,
    from: &ProjectVersion,
    to: &ProjectVersion,
    d: &[PrimDiff],
) -> Vec<String> {
    // Like `diff`: the live mirror for a sublayer at its current version.
    let views = [from, to].map(|v| (v, std::cell::OnceCell::new()));
    let node_at = |side: usize, id: &SublayerId, path: &PrimPath| {
        let (v, view) = &views[side];
        let live = stage.sublayer(id).unwrap();
        let layer = if live.version() == v.get(id) {
            live
        } else {
            view.get_or_init(|| view_at(stage, v, None).unwrap())
                .stage()
                .sublayer(id)
                .unwrap()
        };
        layer
            .node_id(path)
            .unwrap_or_else(|| panic!("{path} not in version {side}"))
    };
    let mut out: Vec<String> = d
        .iter()
        .map(|x| {
            let (kind, side, from) = match &x.change {
                PrimChange::Added => ("added", 1, None),
                PrimChange::Removed => ("removed", 0, None),
                PrimChange::Modified => ("modified", 1, None),
                PrimChange::Moved { from } => ("moved", 1, Some(node_at(0, &x.sublayer, from))),
            };
            format!(
                "{} {kind} {:?} from {from:?} attrs {:?} fields {:?} by {:?} at {}",
                x.sublayer,
                node_at(side, &x.sublayer, &x.path),
                x.attrs,
                x.fields,
                x.authors,
                x.last_changed
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn diff_is_the_same_on_every_client_with_colliding_names() {
    let seeds: u64 = std::env::var("BAE_IDENTITY_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let (mut rounds, mut stale_rounds) = (0, 0);
    for seed in 1..=seeds {
        let base = base_snapshots();
        let who = [author("user:a"), author("user:b")];
        let mut s = [client_stage(&base), client_stage(&base)];
        let mut rng = Rng::new(seed);
        let mut n = 0;
        let mut versions = vec![ProjectVersion::current(&s[0])];
        for round in 0..12 {
            // Both edit (concurrent creates collide), or only one (its mirror
            // is not rebuilt by the exchange that follows).
            let editors: &[usize] = match rng.below(4) {
                0 | 1 => &[0, 1],
                2 => &[0],
                _ => &[1],
            };
            for &i in editors {
                for _ in 0..=rng.below(2) {
                    random_edit(&mut s[i], &who[i], &mut rng, &mut n);
                }
            }
            let [s0, s1] = &mut s;
            sync(&mut [s0, s1]);
            let now = ProjectVersion::current(&s[0]);
            assert_eq!(now, ProjectVersion::current(&s[1]), "seed {seed}");
            rounds += 1;
            if fingerprint(&s[0]) != fingerprint(&s[1]) {
                stale_rounds += 1;
            }
            // A third client that loads everything from scratch (rebuilt mirror).
            let fresh = clone_stage(&s[0]);
            let earlier = versions.len().saturating_sub(3)..versions.len();
            for k in std::iter::once(0).chain(earlier) {
                let v = &versions[k];
                for (from, to) in [(v, &now), (&now, v)] {
                    let reference = by_node(&fresh, from, to, &diff(&fresh, from, to).unwrap());
                    for (i, stage) in s.iter().enumerate() {
                        let d = diff(stage, from, to).unwrap();
                        assert_eq!(
                            by_node(stage, from, to, &d),
                            reference,
                            "seed {seed}, round {round}, version {k}, client {i}: {d:#?}"
                        );
                    }
                }
            }
            versions.push(now);
        }
    }
    eprintln!("{stale_rounds} of {rounds} rounds ended with different mirrors on the two clients");
}

/// Every prim of the root sublayer that existed before a rollback, by tree node:
/// name without its own collision suffix, parent node and spec.
fn kept_prims(stage: &Stage, before: &[TreeID]) -> Vec<String> {
    let l = root_layer(stage);
    let mut out: Vec<String> = before
        .iter()
        .filter_map(|n| {
            let path = l.node_path(*n)?;
            let suffix = format!("_{}_{}", n.peer, n.counter);
            let name = path.name().strip_suffix(&suffix).unwrap_or(path.name());
            let parent = path.parent().and_then(|p| l.node_id(&p));
            let spec = serde_json::to_string(l.spec(path).unwrap()).unwrap();
            Some(format!("{n:?} {name} in {parent:?}: {spec}"))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn revert_is_the_same_on_every_client_with_colliding_names() {
    let seeds: u64 = std::env::var("BAE_IDENTITY_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let mut stale = 0;
    for seed in 1..=seeds {
        let base = base_snapshots();
        let who = [author("user:a"), author("user:b")];
        let mut s = [client_stage(&base), client_stage(&base)];
        let mut rng = Rng::new(seed);
        let mut n = 0;
        let mut versions = vec![ProjectVersion::current(&s[0])];
        // Random edits until the two mirrors differ (or 12 rounds).
        for _ in 0..12 {
            let editors: &[usize] = match rng.below(4) {
                0 | 1 => &[0, 1],
                2 => &[0],
                _ => &[1],
            };
            for &i in editors {
                for _ in 0..=rng.below(2) {
                    random_edit(&mut s[i], &who[i], &mut rng, &mut n);
                }
            }
            let [s0, s1] = &mut s;
            sync(&mut [s0, s1]);
            versions.push(ProjectVersion::current(&s[0]));
            if fingerprint(&s[0]) != fingerprint(&s[1]) {
                stale += 1;
                break;
            }
        }
        // Each client rolls back on its own; both must end up the same.
        let to = versions[rng.below(versions.len() - 1)].clone();
        let before: Vec<TreeID> = {
            let l = root_layer(&s[0]);
            l.paths().filter_map(|p| l.node_id(p)).collect()
        };
        for (i, stage) in s.iter_mut().enumerate() {
            revert(stage, &who[i], "revert", &to, &RevertScope::Project)
                .unwrap_or_else(|e| panic!("seed {seed}, client {i}: {e}"));
        }
        assert_eq!(content(&s[0]), content(&s[1]), "seed {seed}");
        assert_eq!(
            kept_prims(&s[0], &before),
            kept_prims(&s[1], &before),
            "seed {seed}"
        );
        // Every prim of that version is back (by content, ignoring renames to
        // `_2` where a remaining namesake kept the name).
        let then = view_at(&s[0], &to, None).unwrap();
        let count = |st: &Stage| root_layer(st).len();
        assert_eq!(count(&s[0]), count(then.stage()), "seed {seed}");
    }
    eprintln!("{stale} of {seeds} runs rolled back with different mirrors on the two clients");
}

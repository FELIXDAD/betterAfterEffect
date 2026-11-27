//! Conflict notices and the offline merge summary (`docs/spec/sync.md` §6.4).
//!
//! Times are explicit (`observe_at`, `pump_at`), so nothing depends on how fast
//! the machine is. Which of two concurrent ops wins is Loro's order (higher
//! Lamport, then peer); `Client::bump` makes one side's next op win for sure.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bae_model::project::Project;
use bae_model::{
    AuthorId, Expression, Keyframe, ModelError, Op, PrimPath, PrimType, Specifier, Stage,
    StageEvent, SublayerId, Time, Value,
};
use bae_sync::{
    ClientId, ConcurrentEdit, ConflictConfig, ConflictOutcome, ConflictTracker, DirConfig,
    DirTransport, Element, ElementValue, MemoryTransport, OfflinePrimChange, PeerHandle,
    PrimChange, Relay, SessionConfig, SyncSession, Transport,
};
use common::*;

const S: Duration = Duration::from_secs(1);

fn layer(name: &str) -> PrimPath {
    p(&format!("{LAYERS}/{name}"))
}

fn title() -> PrimPath {
    layer("title")
}

fn set(path: &PrimPath, attr: &str, v: f64) -> Op {
    Op::SetValue {
        path: path.clone(),
        attr: attr.into(),
        value: Some(Value::Float(v)),
    }
}

fn sec(s: i64) -> Time {
    Time::from_flicks(s * Time::FLICKS_PER_SECOND)
}

fn key(path: &PrimPath, attr: &str, s: i64, v: f64) -> Op {
    Op::SetKeyframe {
        path: path.clone(),
        attr: attr.into(),
        time: sec(s),
        keyframe: Keyframe::linear(v),
    }
}

fn expr(path: &PrimPath, attr: &str, source: &str) -> Op {
    Op::SetExpression {
        path: path.clone(),
        attr: attr.into(),
        expression: Some(Expression::new(source)),
    }
}

fn create(path: &PrimPath) -> Op {
    Op::CreatePrim {
        path: path.clone(),
        specifier: Specifier::Def,
        prim_type: Some(PrimType::new("Layer")),
        index: None,
    }
}

fn delete(path: &PrimPath) -> Op {
    Op::DeletePrim { path: path.clone() }
}

fn value(v: f64) -> Option<ElementValue> {
    Some(ElementValue::Value(Value::Float(v)))
}

fn keyframe(v: f64) -> Option<ElementValue> {
    Some(ElementValue::Keyframe(Keyframe::linear(v)))
}

fn expression(source: &str) -> Option<ElementValue> {
    Some(ElementValue::Expression(Expression::new(source)))
}

/// One collaborator: a stage, its session and link, and its tracker.
struct Client {
    name: String,
    stage: Stage,
    session: SyncSession,
    link: Option<Box<dyn Transport>>,
    tracker: ConflictTracker,
    who: AuthorId,
    notices: Vec<ConcurrentEdit>,
}

impl Client {
    fn new(stage: Stage, name: &str) -> Self {
        let mut config = SessionConfig::new(ClientId::new(name).unwrap());
        config.hello_interval = None;
        Client {
            name: name.to_owned(),
            tracker: ConflictTracker::new(&stage, ConflictConfig::default()),
            stage,
            session: SyncSession::new(config),
            link: None,
            who: author(&format!("user:{name}")),
            notices: Vec::new(),
        }
    }

    fn observe(&mut self, event: &StageEvent, now: Instant) {
        let n = self.tracker.observe_at(&self.stage, event, now);
        self.notices.extend(n);
    }

    fn apply(&mut self, now: Instant, op: Op) {
        let (_, event) = self
            .stage
            .transact(&self.who, "edit", |tx| tx.apply(op))
            .unwrap_or_else(|e: ModelError| panic!("{e}"));
        self.observe(&event, now);
    }

    /// `n` edits nobody else makes, so that this client's next op wins by Loro
    /// order (a higher Lamport) over a concurrent one of the other side.
    fn bump(&mut self, now: Instant, n: usize) {
        let attr = format!("bump_{}", self.name);
        for i in 0..n {
            self.apply(now, set(&p("/comps/main"), &attr, i as f64));
        }
    }

    fn undo(&mut self, now: Instant) {
        let event = self.stage.undo(&self.who).expect("something to undo");
        self.observe(&event, now);
    }

    fn redo(&mut self, now: Instant) {
        let event = self.stage.redo(&self.who).expect("something to redo");
        self.observe(&event, now);
    }

    /// Messages moved.
    fn pump(&mut self, now: Instant) -> usize {
        let Some(link) = &mut self.link else {
            return 0;
        };
        let out = self.session.pump_at(&mut self.stage, link, now).unwrap();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        for event in &out.events {
            self.observe(event, now);
        }
        // Imports where our value won change nothing visible, so no event.
        let n = self.tracker.check_at(&self.stage, now);
        self.notices.extend(n);
        out.received + out.sent
    }

    fn take_notices(&mut self) -> Vec<ConcurrentEdit> {
        std::mem::take(&mut self.notices)
    }

    fn root(&self) -> &bae_model::Sublayer {
        self.stage.sublayer(&SublayerId::root()).unwrap()
    }

    fn value(&self, path: &PrimPath, attr: &str) -> Option<Value> {
        self.root().attr(path, attr)?.value.clone()
    }
}

fn hub() -> Relay {
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    relay.set_hello_interval(None);
    relay
}

/// (Re)connect `c` to the hub over a fresh in-memory link.
fn connect(relay: &mut Relay, c: &mut Client) -> PeerHandle {
    let (here, there) = MemoryTransport::pair();
    c.link = Some(Box::new(here));
    c.session.reset();
    relay.connect(Box::new(there))
}

/// Pump everyone until no message moves.
fn settle(relay: &mut Relay, clients: &mut [&mut Client], now: Instant) {
    for _ in 0..50 {
        let mut moved = 0;
        for c in clients.iter_mut() {
            moved += c.pump(now);
        }
        let out = relay.pump_at(now);
        assert!(out.errors.is_empty() && out.disconnected.is_empty());
        moved += out.received + out.sent;
        if moved == 0 {
            return;
        }
    }
    panic!("the clients never settle");
}

/// Alice and Bob on one hub, in sync.
fn online() -> (Relay, Client, Client, Instant) {
    let base = base_snapshots();
    let mut relay = hub();
    let mut a = Client::new(client_stage(&base), "alice");
    let mut b = Client::new(client_stage(&base), "bob");
    connect(&mut relay, &mut a);
    connect(&mut relay, &mut b);
    let t0 = Instant::now();
    settle(&mut relay, &mut [&mut a, &mut b], t0);
    (relay, a, b, t0)
}

// ---- concurrent-edit notices ----

#[test]
fn the_overridden_side_is_told_and_the_winner_gets_a_hint() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, set(&title(), "opacity", 50.0));
    b.bump(t0 + S, 3);
    b.apply(t0 + 2 * S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 3 * S);
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
    assert_eq!(b.value(&title(), "opacity"), Some(Value::Float(80.0)));

    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    let n = &na[0];
    assert_eq!(n.outcome, ConflictOutcome::Overridden);
    assert_eq!(
        (&n.sublayer, &n.path, n.attr.as_str(), n.element),
        (&SublayerId::root(), &title(), "opacity", Element::Value)
    );
    assert_eq!(Some(n.node), a.root().node_id(&title()));
    assert_eq!(n.ours, value(50.0));
    assert_eq!(n.result, value(80.0));
    assert_eq!(n.theirs, value(80.0));
    assert_eq!(n.winner, Some(b.who.clone()));
    assert_eq!(n.others, vec![b.who.clone()]);
    assert!(n.concurrent);
    assert_eq!((n.edited_at, n.at), (t0 + S, t0 + 3 * S));

    let nb = b.take_notices();
    assert_eq!(nb.len(), 1, "{nb:#?}");
    let n = &nb[0];
    assert_eq!(n.outcome, ConflictOutcome::Kept);
    assert_eq!(n.ours, value(80.0));
    assert_eq!(n.result, value(80.0));
    assert_eq!(n.theirs, value(50.0));
    assert_eq!(n.winner, Some(b.who.clone()));
    assert_eq!(n.others, vec![a.who.clone()]);
    assert!(n.concurrent);
    // Overridden edits are forgotten; kept ones stay until the window ends.
    assert_eq!(a.tracker.remembered(), 0);
}

#[test]
fn a_change_made_after_seeing_ours_is_not_concurrent() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, set(&title(), "opacity", 50.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + S);
    b.apply(t0 + 4 * S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 5 * S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(na[0].winner, Some(b.who.clone()));
    assert!(!na[0].concurrent);
    assert!(b.take_notices().is_empty());
}

#[test]
fn nothing_is_reported_outside_the_window() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0, set(&title(), "opacity", 50.0));
    b.bump(t0, 3);
    b.apply(t0, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 11 * S);
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());

    // The window is a setting.
    let (mut relay, mut a, mut b, t0) = online();
    a.tracker.config_mut().window = 30 * S;
    a.apply(t0, set(&title(), "opacity", 50.0));
    b.bump(t0, 3);
    b.apply(t0, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 11 * S);
    assert_eq!(a.take_notices().len(), 1);
    assert!(b.take_notices().is_empty());
}

#[test]
fn keyframes_conflict_only_at_the_same_time() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, key(&title(), "position", 1, 10.0));
    b.bump(t0 + S, 3);
    b.apply(t0 + S, key(&title(), "position", 2, 20.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());
    let keys = &a.root().attr(&title(), "position").unwrap().keyframes;
    assert_eq!(keys.keys().copied().collect::<Vec<_>>(), [sec(1), sec(2)]);

    a.apply(t0 + 3 * S, key(&title(), "position", 3, 30.0));
    b.bump(t0 + 3 * S, 3);
    b.apply(t0 + 3 * S, key(&title(), "position", 3, 40.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 4 * S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(na[0].element, Element::Keyframe(sec(3)));
    assert_eq!(na[0].ours, keyframe(30.0));
    assert_eq!(na[0].result, keyframe(40.0));
    let nb = b.take_notices();
    assert_eq!(nb.len(), 1, "{nb:#?}");
    assert_eq!(nb[0].outcome, ConflictOutcome::Kept);
    assert_eq!(nb[0].theirs, keyframe(30.0));
}

#[test]
fn value_and_expression_are_separate_elements() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, set(&title(), "opacity", 50.0));
    b.bump(t0 + S, 3);
    b.apply(t0 + S, expr(&title(), "opacity", "wiggle(2, 10)"));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());
    let attr = a.root().attr(&title(), "opacity").unwrap().clone();
    assert_eq!(attr.value, Some(Value::Float(50.0)));
    assert_eq!(attr.expression, Some(Expression::new("wiggle(2, 10)")));

    a.apply(t0 + 3 * S, expr(&title(), "opacity", "time * 10"));
    b.bump(t0 + 3 * S, 3);
    b.apply(t0 + 3 * S, expr(&title(), "opacity", "value / 2"));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 4 * S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].element, Element::Expression);
    assert_eq!(na[0].ours, expression("time * 10"));
    assert_eq!(na[0].result, expression("value / 2"));
}

#[test]
fn undo_and_redo_count_as_our_edits() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, set(&title(), "opacity", 50.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + S);
    a.apply(t0 + 2 * S, set(&title(), "opacity", 60.0));
    a.undo(t0 + 3 * S);
    b.bump(t0 + 3 * S, 3);
    b.apply(t0 + 3 * S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 4 * S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
    // What the undo set, not the 60 it took back.
    assert_eq!(
        (na[0].ours.clone(), na[0].edited_at),
        (value(50.0), t0 + 3 * S)
    );
    assert_eq!(na[0].result, value(80.0));

    a.redo(t0 + 5 * S);
    b.bump(t0 + 5 * S, 3);
    b.apply(t0 + 5 * S, set(&title(), "opacity", 90.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 6 * S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].ours, value(60.0));
    assert_eq!(na[0].result, value(90.0));
}

#[test]
fn remote_changes_of_what_we_did_not_touch_are_quiet() {
    let (mut relay, mut a, mut b, t0) = online();
    // Same prim, another attribute.
    a.apply(t0 + S, set(&title(), "scale", 2.0));
    b.apply(t0 + S, set(&title(), "opacity", 80.0));
    b.apply(t0 + S, key(&title(), "position", 1, 5.0));
    b.apply(t0 + S, create(&layer("logo")));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());
    assert_eq!(a.tracker.stats().elements_checked, 0);
    assert_eq!(a.tracker.stats().remote_ops, 0);
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
}

#[test]
fn removing_a_prim_someone_is_editing_is_reported() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0, create(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t0);
    a.apply(t0 + S, set(&layer("x"), "opacity", 50.0));
    a.apply(t0 + S, key(&layer("x"), "position", 0, 1.0));
    b.apply(t0 + 2 * S, delete(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 3 * S);
    assert!(a.root().node_id(&layer("x")).is_none());
    let na = a.take_notices();
    assert_eq!(na.len(), 2, "{na:#?}");
    for n in &na {
        assert_eq!(n.outcome, ConflictOutcome::PrimRemoved);
        assert_eq!(n.path, layer("x"));
        assert_eq!(n.result, None);
        assert_eq!(n.winner, Some(b.who.clone()));
        assert!(n.concurrent);
    }
    assert_eq!(
        (na[0].attr.as_str(), na[1].attr.as_str()),
        ("opacity", "position")
    );
    assert_eq!(na[0].ours, value(50.0));
    assert!(b.take_notices().is_empty());
}

#[test]
fn a_large_import_is_only_read_where_we_edited() {
    const N: usize = 1000;
    let mut seed = client_stage(&base_snapshots());
    seed.transact(&AuthorId::system(), "layers", |tx| {
        for i in 0..N {
            tx.apply(create(&layer(&format!("l{i}"))))?;
        }
        Ok::<_, ModelError>(())
    })
    .unwrap();
    let base: Vec<(SublayerId, Vec<u8>)> = seed
        .sublayers()
        .map(|s| (s.id().clone(), s.export_snapshot().unwrap()))
        .collect();
    let mut a = Client::new(client_stage(&base), "alice");
    let mut b = Client::new(client_stage(&base), "bob");
    let mut c = Client::new(client_stage(&base), "carol");
    let t0 = Instant::now();
    a.apply(t0, set(&layer("l0"), "opacity", 50.0));
    // Bob changes every layer, one step each (equal labels: Loro merges them
    // into one change), `l0` last.
    for i in (0..N).rev() {
        b.apply(t0, set(&layer(&format!("l{i}")), "opacity", 80.0));
    }
    let root = SublayerId::root();
    let import = |to: &mut Client| {
        let have = to.root().version();
        let bytes = b.root().export_updates(&have).unwrap();
        let event = to.stage.import_updates(&root, &bytes).unwrap();
        assert!(event.changes.len() >= N);
        to.observe(&event, t0 + S);
    };

    import(&mut a);
    let stats = a.tracker.stats();
    assert_eq!(stats.elements_checked, 1);
    // About 3 ops per layer arrived; only the ones around `l0`'s were decoded.
    assert!(stats.remote_ops <= 64, "{stats:?}");
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].path, layer("l0"));
    assert_eq!(na[0].result, value(80.0));

    // Without own edits an import costs no decoding at all.
    import(&mut c);
    assert_eq!(c.tracker.stats().elements_checked, 0);
    assert_eq!(c.tracker.stats().remote_ops, 0);
    assert!(c.take_notices().is_empty());
}

// ---- offline merge summary ----

/// Alice and Bob share title, `x`, `y` and folder `f`; Alice goes offline at
/// `t1`. Returns the hub handle of Alice's old connection.
fn shared_layers() -> (Relay, Client, Client, Instant, Instant) {
    let (mut relay, mut a, mut b, t0) = online();
    for name in ["x", "y", "f"] {
        a.apply(t0, create(&layer(name)));
    }
    a.apply(t0, set(&layer("x"), "opacity", 10.0));
    a.apply(t0, set(&layer("y"), "opacity", 20.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0);
    (relay, a, b, t0, t0 + 60 * S)
}

#[test]
fn offline_edits_are_summarized_after_catching_up() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    let handle = relay.peers()[0].0;
    assert_eq!(
        relay.peers()[0].1.as_ref().map(ClientId::as_str),
        Some("alice")
    );
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(handle);
    a.link = None;
    assert!(a.tracker.offline_since().is_some());

    // Alice, offline. Her first op loses to Bob's later one; her last one wins.
    a.apply(t1 + S, set(&title(), "opacity", 50.0));
    a.apply(t1 + S, key(&title(), "position", 0, 5.0));
    a.apply(t1 + S, delete(&layer("x")));
    a.apply(t1 + S, set(&layer("y"), "opacity", 30.0));
    a.apply(t1 + S, create(&layer("f/new")));
    a.apply(t1 + S, create(&layer("added")));
    a.bump(t1 + S, 10);
    a.apply(t1 + 2 * S, set(&title(), "scale", 2.0));

    // Bob, online.
    b.apply(t1 + S, set(&title(), "scale", 3.0));
    b.apply(t1 + S, set(&layer("x"), "opacity", 11.0));
    b.apply(t1 + S, delete(&layer("y")));
    b.apply(t1 + S, delete(&layer("f")));
    b.apply(t1 + S, create(&layer("b_new")));
    b.bump(t1 + S, 5);
    b.apply(t1 + 3 * S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + 3 * S);

    // Alice reconnects.
    let t2 = t1 + 5 * S;
    connect(&mut relay, &mut a);
    assert!(!a.session.is_caught_up(&a.stage));
    for _ in 0..10 {
        settle(&mut relay, &mut [&mut a, &mut b], t2);
        if a.session.is_caught_up(&a.stage) {
            break;
        }
    }
    assert!(a.session.is_caught_up(&a.stage));
    assert_eq!(fingerprint(&a.stage), fingerprint(&b.stage));
    // Alice's offline edits are left to the summary.
    assert!(a.take_notices().is_empty());
    let s = a
        .tracker
        .caught_up(&a.stage)
        .unwrap()
        .expect("a reconnect was pending");
    assert!(a.tracker.offline_since().is_none());
    assert!(a.tracker.caught_up(&a.stage).unwrap().is_none());
    let (alice, bob) = (a.who.clone(), b.who.clone());

    let brief = |e: &bae_sync::OfflineEdit| (e.path.clone(), e.attr.clone(), e.element);
    assert_eq!(
        s.kept.iter().map(brief).collect::<Vec<_>>(),
        [
            (p("/comps/main"), "bump_alice".to_owned(), Element::Value),
            (title(), "position".to_owned(), Element::Keyframe(sec(0))),
            (title(), "scale".to_owned(), Element::Value),
        ]
    );
    assert_eq!(s.kept[1].others, Vec::<AuthorId>::new());
    assert_eq!(s.kept[2].ours, value(2.0));
    assert_eq!(s.kept[2].others, std::slice::from_ref(&bob));
    assert_eq!(s.kept[2].author, Some(alice.clone()));

    assert_eq!(
        s.overridden.iter().map(brief).collect::<Vec<_>>(),
        [
            (title(), "opacity".to_owned(), Element::Value),
            (layer("y"), "opacity".to_owned(), Element::Value),
        ]
    );
    let o = &s.overridden[0];
    assert_eq!(
        (o.ours.clone(), o.result.clone()),
        (value(50.0), value(80.0))
    );
    assert_eq!(
        (o.winner.clone(), o.prim_removed),
        (Some(bob.clone()), false)
    );
    let o = &s.overridden[1];
    assert_eq!((o.ours.clone(), o.result.clone()), (value(30.0), None));
    assert_eq!(
        (o.winner.clone(), o.prim_removed),
        (Some(bob.clone()), true)
    );

    let prims: Vec<_> = s
        .prims
        .iter()
        .map(|p| (p.path.clone(), p.change, p.by.clone()))
        .collect();
    assert_eq!(
        prims,
        [
            (layer("added"), OfflinePrimChange::Added, vec![]),
            (
                layer("f/new"),
                OfflinePrimChange::AddedButRemoved,
                vec![bob.clone()]
            ),
            (
                layer("x"),
                OfflinePrimChange::RemovedButChanged,
                vec![bob.clone()]
            ),
        ]
    );
    assert_eq!(s.conflicts(), 4);

    // What Bob changed, by prim, from what Alice had right before merging.
    let merged: Vec<_> = s
        .merged
        .iter()
        .map(|d| (d.path.clone(), d.change.clone(), d.attrs.clone()))
        .collect();
    assert_eq!(
        merged,
        [
            (
                p("/comps/main"),
                PrimChange::Modified,
                vec!["bump_bob".to_owned()]
            ),
            (layer("b_new"), PrimChange::Added, vec![]),
            (layer("f"), PrimChange::Removed, vec![]),
            (layer("f/new"), PrimChange::Removed, vec![]),
            (title(), PrimChange::Modified, vec!["opacity".to_owned()]),
            (layer("y"), PrimChange::Removed, vec!["opacity".to_owned()]),
        ]
    );
    for d in &s.merged {
        // `f/new` went with its parent: no op of Bob's names it.
        let expected = if d.path == layer("f/new") {
            vec![]
        } else {
            vec![bob.clone()]
        };
        assert_eq!(d.authors, expected, "{}", d.path);
    }

    // Bob was online: his recent edits that met Alice's get notices.
    let nb: Vec<_> = b
        .take_notices()
        .iter()
        .map(|n| (n.path.clone(), n.attr.clone(), n.outcome, n.winner.clone()))
        .collect();
    assert_eq!(
        nb,
        [
            (
                title(),
                "opacity".to_owned(),
                ConflictOutcome::Kept,
                Some(bob.clone())
            ),
            (
                title(),
                "scale".to_owned(),
                ConflictOutcome::Overridden,
                Some(alice.clone())
            ),
            (
                layer("x"),
                "opacity".to_owned(),
                ConflictOutcome::PrimRemoved,
                Some(alice.clone())
            ),
        ]
    );
}

#[test]
fn a_removal_another_author_undid_by_moving_the_prim() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    // Alice removes `x` and `y` first thing; Bob moves `x` into `f` later
    // (a tree move, which wins over her delete by Loro order).
    a.apply(t1 + S, delete(&layer("x")));
    a.apply(t1 + S, delete(&layer("y")));
    b.bump(t1 + S, 3);
    b.apply(
        t1 + S,
        Op::MovePrim {
            from: layer("x"),
            to: layer("f/x"),
            index: None,
        },
    );
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t1 + 2 * S);
    assert!(a.session.is_caught_up(&a.stage));
    assert!(a.root().node_id(&layer("f/x")).is_some());
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap();
    let prims: Vec<_> = s
        .prims
        .iter()
        .map(|p| (p.path.clone(), p.change, p.by.clone()))
        .collect();
    assert_eq!(
        prims,
        [
            (
                layer("f/x"),
                OfflinePrimChange::RemovalUndone,
                vec![b.who.clone()]
            ),
            (layer("y"), OfflinePrimChange::Removed, vec![]),
        ]
    );
    assert_eq!(s.conflicts(), 1);
}

#[test]
fn a_reconnect_with_nothing_new_on_either_side_is_empty() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(relay.peers()[0].0);
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);
    assert!(a.session.is_caught_up(&a.stage));
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap();
    assert!(s.is_empty(), "{s:#?}");
    assert_eq!(s.conflicts(), 0);
}

/// A project directory with the base project saved in it.
fn project_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    let mut project = Project::from_stage(client_stage(&base_snapshots()));
    project.save_as(&dir).unwrap();
    (tmp, dir)
}

fn folder_client(dir: &std::path::Path, name: &str) -> Client {
    let mut c = Client::new(Project::open(dir).unwrap().stage, name);
    let config = DirConfig {
        poll_interval: Duration::ZERO,
        watch: false,
        ..DirConfig::default()
    };
    c.link = Some(Box::new(
        DirTransport::open(dir, ClientId::new(name).unwrap(), config).unwrap(),
    ));
    c
}

#[test]
fn offline_summary_through_a_shared_folder() {
    let (_tmp, dir) = project_dir();
    let mut a = folder_client(&dir, "alice");
    let mut b = folder_client(&dir, "bob");
    let t0 = Instant::now();
    for _ in 0..5 {
        a.pump(t0);
        b.pump(t0);
    }
    assert_eq!(fingerprint(&a.stage), fingerprint(&b.stage));

    // The share becomes unreachable for Alice.
    let t1 = t0 + 60 * S;
    a.tracker.connection_lost_at(&a.stage, t1);
    let link = a.link.take();
    a.apply(t1 + S, set(&title(), "opacity", 50.0));
    a.apply(t1 + S, key(&title(), "position", 0, 5.0));
    b.bump(t1 + S, 5);
    b.apply(t1 + S, set(&title(), "opacity", 80.0));
    b.apply(t1 + S, create(&layer("b_new")));
    b.pump(t1 + S);

    // Back: forget what the folder had, then pump until caught up.
    a.link = link;
    a.session.reset();
    let t2 = t1 + 5 * S;
    for _ in 0..10 {
        a.pump(t2);
        if a.session.is_caught_up(&a.stage) {
            break;
        }
    }
    assert!(a.session.is_caught_up(&a.stage));
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap();
    let bob = b.who.clone();
    assert_eq!(s.kept.len(), 1, "{s:#?}");
    assert_eq!(s.kept[0].element, Element::Keyframe(sec(0)));
    assert_eq!(s.overridden.len(), 1, "{s:#?}");
    assert_eq!(s.overridden[0].path, title());
    assert_eq!(s.overridden[0].result, value(80.0));
    assert_eq!(s.overridden[0].winner, Some(bob.clone()));
    assert!(s.prims.is_empty());
    let merged: Vec<_> = s
        .merged
        .iter()
        .map(|d| (d.path.clone(), d.change.clone()))
        .collect();
    assert_eq!(
        merged,
        [
            (p("/comps/main"), PrimChange::Modified),
            (layer("b_new"), PrimChange::Added),
            (title(), PrimChange::Modified),
        ]
    );
    assert!(a.take_notices().is_empty());

    // Bob reads Alice's offline edits from the folder: his recent opacity won.
    b.pump(t2);
    b.pump(t2);
    assert_eq!(fingerprint(&a.stage), fingerprint(&b.stage));
    let nb = b.take_notices();
    assert_eq!(nb.len(), 1, "{nb:#?}");
    assert_eq!(nb[0].outcome, ConflictOutcome::Kept);
    assert_eq!(nb[0].theirs, value(50.0));
}

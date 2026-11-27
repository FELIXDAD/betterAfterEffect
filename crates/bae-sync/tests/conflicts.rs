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
    let caught = a
        .tracker
        .caught_up(&a.stage)
        .unwrap()
        .expect("a reconnect was pending");
    // Every pump was checked already: nothing left for the catch-up's check.
    assert!(caught.notices.is_empty(), "{:#?}", caught.notices);
    let s = caught.summary;
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
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
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
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
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
    folder_client_polling(dir, name, Duration::ZERO)
}

/// A folder client whose transport polls at most every `poll_interval` (real
/// time) on its own.
fn folder_client_polling(dir: &std::path::Path, name: &str, poll_interval: Duration) -> Client {
    let mut c = Client::new(Project::open(dir).unwrap().stage, name);
    let config = DirConfig {
        poll_interval,
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
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
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

// ---- timing of observe, reconnects and catch-up ----

/// Import into `to` what `from` has and it lacks, and observe the event.
fn import_from(to: &mut Client, from: &Client, now: Instant) {
    let root = SublayerId::root();
    let bytes = from.root().export_updates(&to.root().version()).unwrap();
    let event = to.stage.import_updates(&root, &bytes).unwrap();
    to.observe(&event, now);
}

#[test]
fn a_local_edit_observed_after_a_same_frame_import_is_still_checked() {
    let (_relay, mut a, mut b, t0) = online();
    let (_, local) = a
        .stage
        .transact(&a.who, "edit", |tx| {
            tx.apply(set(&title(), "opacity", 50.0))
        })
        .unwrap();
    b.bump(t0, 3);
    b.apply(t0, set(&title(), "opacity", 80.0));
    // A pump imports Bob's change before the app hands the frame's events
    // to the tracker, in order.
    let bytes = b.root().export_updates(&a.root().version()).unwrap();
    let remote = a.stage.import_updates(&SublayerId::root(), &bytes).unwrap();
    a.observe(&local, t0 + S);
    a.observe(&remote, t0 + S);
    let n = a.tracker.check_at(&a.stage, t0 + S);
    a.notices.extend(n);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(na[0].ours, value(50.0));
    assert_eq!(na[0].result, value(80.0));
    assert!(na[0].concurrent);
    assert_eq!(a.tracker.stats().remote_events, 1);

    // Also when a check ran between the import and the observe.
    let (_, local) = a
        .stage
        .transact(&a.who, "edit", |tx| tx.apply(set(&title(), "scale", 2.0)))
        .unwrap();
    b.bump(t0 + 3 * S, 5);
    b.apply(t0 + 3 * S, set(&title(), "scale", 3.0));
    let bytes = b.root().export_updates(&a.root().version()).unwrap();
    let remote = a.stage.import_updates(&SublayerId::root(), &bytes).unwrap();
    a.observe(&remote, t0 + 3 * S);
    assert!(a.take_notices().is_empty());
    a.observe(&local, t0 + 3 * S);
    let n = a.tracker.check_at(&a.stage, t0 + 3 * S);
    assert_eq!(n.len(), 1, "{n:#?}");
    assert_eq!(
        (n[0].attr.as_str(), n[0].result.clone()),
        ("scale", value(3.0))
    );
}

#[test]
fn an_edit_the_dropped_link_never_delivered_is_reported_after_reconnecting() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    // Alice's last edit never leaves: the link is already dead, and the app
    // notices a second later.
    a.apply(t1, set(&title(), "opacity", 50.0));
    a.tracker.connection_lost_at(&a.stage, t1 + S);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    b.bump(t1, 5);
    b.apply(t1, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);

    // Back a minute later: the edit is as old as it was when the link went.
    let t2 = t1 + 60 * S;
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t2);
    assert!(a.session.is_caught_up(&a.stage));
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(
        (na[0].ours.clone(), na[0].result.clone()),
        (value(50.0), value(80.0))
    );
    assert_eq!(na[0].winner, Some(b.who.clone()));
    assert!(na[0].concurrent);
    assert_eq!((na[0].edited_at, na[0].at), (t1, t2));
    // Made before the mark: not an offline edit.
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    assert!(s.overridden.is_empty() && s.kept.is_empty(), "{s:#?}");
    assert!(b.take_notices().is_empty());
}

/// An app that asks for the summary right after the pump that caught up,
/// before any check: `caught_up` checks what arrived first, so the edit the
/// dead link never delivered still gets its notice (before, the summary
/// counted the catch-up's remote ops as compared, and the notice was lost).
#[test]
fn the_catch_up_checks_what_arrived_before_summarizing() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.apply(t1, set(&title(), "opacity", 50.0));
    a.tracker.connection_lost_at(&a.stage, t1 + S);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    b.bump(t1, 5);
    b.apply(t1, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);

    let t2 = t1 + 60 * S;
    connect(&mut relay, &mut a);
    catch_up_unchecked(&mut relay, &mut a, &mut b, t2);
    let caught = a.tracker.caught_up_at(&a.stage, t2).unwrap().unwrap();
    assert_eq!(caught.notices.len(), 1, "{:#?}", caught.notices);
    let n = &caught.notices[0];
    assert_eq!(n.outcome, ConflictOutcome::Overridden);
    assert_eq!(
        (n.ours.clone(), n.result.clone()),
        (value(50.0), value(80.0))
    );
    assert_eq!(n.winner, Some(b.who.clone()));
    assert!(
        caught.summary.overridden.is_empty(),
        "{:#?}",
        caught.summary
    );
    // Reported once.
    assert!(a.tracker.check_at(&a.stage, t2).is_empty());
    assert!(a.tracker.offline_since().is_none());
}

/// [`ConflictTracker::caught_up_detached`]: the same check and notices on
/// the stage's thread, the summary on another thread, equal to the one
/// computed on the live stage.
#[test]
fn the_summary_can_be_computed_on_another_thread() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.apply(t1, set(&title(), "opacity", 50.0));
    a.tracker.connection_lost_at(&a.stage, t1 + S);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    a.apply(t1 + 2 * S, set(&layer("y"), "opacity", 30.0));
    a.apply(t1 + 2 * S, create(&layer("added")));
    b.bump(t1, 5);
    b.apply(t1, set(&title(), "opacity", 80.0));
    b.apply(t1 + S, delete(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);

    let t2 = t1 + 60 * S;
    connect(&mut relay, &mut a);
    catch_up_unchecked(&mut relay, &mut a, &mut b, t2);
    let since = a.tracker.offline_since().unwrap().clone();
    let live = bae_sync::merge_summary(&a.stage, &since).unwrap();
    let caught = a.tracker.caught_up_detached_at(&a.stage, t2).unwrap();
    assert!(a.tracker.offline_since().is_none());
    assert_eq!(caught.notices.len(), 1, "{:#?}", caught.notices);
    assert_eq!(caught.notices[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(caught.merge.since(), &since);
    // The stage goes on changing meanwhile; the copy was pinned.
    a.apply(t2, set(&layer("y"), "opacity", 31.0));
    let merge = caught.merge;
    let summary = std::thread::spawn(move || merge.summarize())
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(summary, live);
    assert_eq!(summary.kept.len(), 1, "{summary:#?}");
    assert_eq!(summary.prims.len(), 1, "{summary:#?}");
    assert!(!summary.merged.is_empty());
}

/// Reconnect `a` and pump until it caught up, without any check (an app that
/// asks for the summary first).
fn catch_up_unchecked(relay: &mut Relay, a: &mut Client, b: &mut Client, now: Instant) {
    for _ in 0..50 {
        let link = a.link.as_mut().unwrap();
        a.session.pump_at(&mut a.stage, link, now).unwrap();
        relay.pump_at(now);
        b.pump(now);
        if a.session.is_caught_up(&a.stage) {
            return;
        }
    }
    panic!("never caught up");
}

/// A catch-up that never completes (`is_caught_up` stays false) silences
/// every later online notice until the mark goes: `cancel_offline` drops it.
#[test]
fn cancel_offline_resumes_online_notices() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.connection_lost_at(&a.stage, t1);
    // The link stays (or comes back) up, but the app never sees a catch-up.
    let race = |a: &mut Client, b: &mut Client, relay: &mut Relay, t: Instant, v: f64| {
        a.apply(t, set(&title(), "opacity", v));
        b.bump(t, 3);
        b.apply(t, set(&title(), "opacity", v + 1.0));
        settle(relay, &mut [&mut *a, &mut *b], t + S);
    };
    race(&mut a, &mut b, &mut relay, t1 + S, 10.0);
    assert!(a.take_notices().is_empty(), "left to the summary");
    race(&mut a, &mut b, &mut relay, t1 + 600 * S, 20.0);
    assert!(a.take_notices().is_empty(), "still waiting for a catch-up");

    assert!(a.tracker.cancel_offline());
    assert!(!a.tracker.cancel_offline());
    assert!(a.tracker.offline_since().is_none());
    assert!(a.tracker.caught_up(&a.stage).unwrap().is_none());
    race(&mut a, &mut b, &mut relay, t1 + 700 * S, 30.0);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].ours, value(30.0));
    assert_eq!(na[0].outcome, ConflictOutcome::Overridden);
}

/// `catch_up_timeout` counts from the catch-up (others' ops beyond the mark
/// arrived), not from the disconnection: a long offline period keeps its
/// mark, a catch-up that never completes loses it after the timeout.
#[test]
fn the_catch_up_timeout_starts_when_the_catch_up_begins() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.config_mut().catch_up_timeout = Some(30 * S);
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    a.apply(t1 + S, set(&title(), "opacity", 10.0));
    b.apply(t1 + S, set(&layer("x"), "opacity", 11.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);
    // Offline for hours, checked all along (an app that checks every frame):
    // nothing of the others arrived, the mark stays.
    let t2 = t1 + 3600 * 5 * S;
    for t in [t1 + 2 * S, t1 + 60 * S, t2] {
        assert!(a.tracker.check_at(&a.stage, t).is_empty());
        assert!(a.tracker.offline_since().is_some(), "dropped while offline");
    }

    // Back: Bob's edit arrives, the app never calls `caught_up`.
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t2);
    assert!(a.tracker.check_at(&a.stage, t2 + 29 * S).is_empty());
    assert!(a.tracker.offline_since().is_some());
    assert!(a.tracker.check_at(&a.stage, t2 + 30 * S).is_empty());
    assert!(a.tracker.offline_since().is_none(), "timed out");

    // Online notices are back.
    let t3 = t2 + 40 * S;
    a.apply(t3, set(&title(), "opacity", 20.0));
    b.bump(t3, 3);
    b.apply(t3, set(&title(), "opacity", 21.0));
    settle(&mut relay, &mut [&mut a, &mut b], t3 + S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].ours, value(20.0));
}

/// Drop `c`'s link; the hub forgets the connection too.
fn drop_link(relay: &mut Relay, c: &mut Client) {
    let handle = relay
        .peers()
        .iter()
        .find(|(_, id)| id.as_ref().map(ClientId::as_str) == Some(c.name.as_str()))
        .expect("connected")
        .0;
    relay.disconnect(handle);
    c.link = None;
}

/// A catch-up the link drops again before it completes (flaky Wi-Fi): the
/// next `connection_lost` restarts the timeout. Neither the checks while
/// offline again nor the ops the interrupted catch-up brought start it, so
/// the reconnect an hour later still ends in a summary of the whole time.
#[test]
fn an_interrupted_catch_up_restarts_the_timeout() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.config_mut().catch_up_timeout = Some(30 * S);
    a.tracker.connection_lost_at(&a.stage, t1);
    drop_link(&mut relay, &mut a);
    a.apply(t1 + S, set(&title(), "opacity", 10.0));
    b.apply(t1 + S, set(&layer("x"), "opacity", 11.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);

    // Back: Bob's ops arrive (the catch-up began), and the link drops again
    // before the app saw it caught up.
    let t2 = t1 + 60 * S;
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t2);
    assert!(a.tracker.offline_since().is_some());
    a.tracker.connection_lost_at(&a.stage, t2 + 2 * S);
    drop_link(&mut relay, &mut a);

    // Offline for an hour, checked all along. Bob overrides Alice's edit.
    a.apply(t2 + 3 * S, set(&title(), "scale", 5.0));
    b.bump(t2 + 3 * S, 5);
    b.apply(t2 + 3 * S, set(&title(), "scale", 7.0));
    settle(&mut relay, &mut [&mut b], t2 + 3 * S);
    for t in [40, 600, 3599] {
        assert!(a.tracker.check_at(&a.stage, t2 + t * S).is_empty());
        assert!(
            a.tracker.offline_since().is_some(),
            "dropped offline at +{t} s"
        );
    }

    // Back again: the summary covers both offline stretches.
    let t3 = t2 + 3600 * S;
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t3);
    assert!(a.session.is_caught_up(&a.stage));
    assert!(a.take_notices().is_empty(), "left to the summary");
    let caught = a
        .tracker
        .caught_up_at(&a.stage, t3)
        .unwrap()
        .expect("a summary");
    let s = caught.summary;
    let brief = |e: &bae_sync::OfflineEdit| (e.attr.clone(), e.ours.clone());
    assert_eq!(
        s.overridden.iter().map(brief).collect::<Vec<_>>(),
        [("scale".to_owned(), value(5.0))]
    );
    assert!(
        s.kept
            .iter()
            .any(|e| e.attr == "opacity" && e.ours == value(10.0)),
        "{:#?}",
        s.kept
    );

    // The timeout still works on the next catch-up that never completes.
    a.tracker.connection_lost_at(&a.stage, t3 + S);
    drop_link(&mut relay, &mut a);
    b.apply(t3 + 2 * S, set(&layer("y"), "opacity", 21.0));
    settle(&mut relay, &mut [&mut b], t3 + 2 * S);
    let t4 = t3 + 600 * S;
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t4);
    assert!(a.tracker.check_at(&a.stage, t4 + 29 * S).is_empty());
    assert!(a.tracker.offline_since().is_some());
    assert!(a.tracker.check_at(&a.stage, t4 + 30 * S).is_empty());
    assert!(a.tracker.offline_since().is_none(), "timed out");
}

/// A layer file that carries others' history, added to the stack while
/// offline, is no sign of a link: it does not start the catch-up timeout.
/// Ops of others that arrive in it later do.
#[test]
fn a_layer_added_offline_does_not_start_the_catch_up_timeout() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.config_mut().catch_up_timeout = Some(30 * S);
    a.tracker.connection_lost_at(&a.stage, t1);
    drop_link(&mut relay, &mut a);
    let bytes = b.root().export_snapshot().unwrap();
    let extra = bae_model::Sublayer::from_snapshot(sl("extra"), &bytes).unwrap();
    let event = a.stage.insert_sublayer(extra, 0).unwrap();
    a.observe(&event, t1 + 5 * S);
    for t in [6, 40, 3600] {
        assert!(a.tracker.check_at(&a.stage, t1 + t * S).is_empty());
        assert!(
            a.tracker.offline_since().is_some(),
            "dropped offline at +{t} s"
        );
    }

    // Back: Bob's ops arrive, the timeout starts.
    b.apply(t1 + 3601 * S, set(&layer("x"), "opacity", 11.0));
    settle(&mut relay, &mut [&mut b], t1 + 3601 * S);
    let t2 = t1 + 3700 * S;
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t2);
    assert!(a.tracker.check_at(&a.stage, t2 + 29 * S).is_empty());
    assert!(a.tracker.offline_since().is_some());
    assert!(a.tracker.check_at(&a.stage, t2 + 30 * S).is_empty());
    assert!(a.tracker.offline_since().is_none(), "timed out");
}

#[test]
fn a_delivered_edit_changed_later_by_someone_who_saw_it_is_not_reported_on_reconnect() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.apply(t1, set(&title(), "opacity", 50.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1);
    a.tracker.connection_lost_at(&a.stage, t1 + S);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    // Half a minute later Bob changes it, knowing Alice's value.
    b.apply(t1 + 30 * S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + 30 * S);
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t1 + 60 * S);
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
    assert!(a.take_notices().is_empty());
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    assert_eq!(s.merged.len(), 1, "{s:#?}");
    assert_eq!(s.merged[0].attrs, ["opacity"]);
}

#[test]
fn others_changes_imported_before_an_edit_during_catch_up_are_merged() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    a.apply(t1 + S, set(&title(), "opacity", 50.0));
    // The catch-up arrives in two parts; Alice edits in between.
    b.apply(t1 + S, create(&layer("b1")));
    import_from(&mut a, &b, t1 + 2 * S);
    a.apply(t1 + 2 * S, set(&title(), "scale", 2.0));
    b.apply(t1 + 2 * S, create(&layer("b2")));
    import_from(&mut a, &b, t1 + 3 * S);
    assert!(a.take_notices().is_empty());

    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    let merged: Vec<_> = s
        .merged
        .iter()
        .map(|d| (d.path.clone(), d.change.clone(), d.authors.clone()))
        .collect();
    assert_eq!(
        merged,
        [
            (layer("b1"), PrimChange::Added, vec![b.who.clone()]),
            (layer("b2"), PrimChange::Added, vec![b.who.clone()]),
        ]
    );
    // Both were made while the reconnect was pending.
    let kept: Vec<_> = s.kept.iter().map(|e| e.attr.as_str()).collect();
    assert_eq!(kept, ["opacity", "scale"]);
    assert!(s.overridden.is_empty());
}

#[test]
fn a_folder_reconnect_rereads_the_folder_before_it_is_caught_up() {
    let (_tmp, dir) = project_dir();
    // Alice's own polling is never due again: only the reconnect reads.
    let mut a = folder_client_polling(&dir, "alice", Duration::from_secs(3600));
    let mut b = folder_client(&dir, "bob");
    let t0 = Instant::now();
    for _ in 0..3 {
        a.pump(t0);
        b.pump(t0);
    }
    let t1 = t0 + 60 * S;
    a.tracker.connection_lost_at(&a.stage, t1);
    let link = a.link.take();
    a.apply(t1 + S, set(&title(), "opacity", 50.0));
    b.bump(t1 + S, 5);
    b.apply(t1 + S, set(&title(), "opacity", 80.0));
    b.pump(t1 + S);

    a.link = link;
    a.session.reset();
    let t2 = t1 + 60 * S;
    a.pump(t2);
    assert!(a.session.is_caught_up(&a.stage));
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    assert!(s.kept.is_empty(), "{s:#?}");
    assert_eq!(s.overridden.len(), 1, "{s:#?}");
    assert_eq!(s.overridden[0].result, value(80.0));
    assert_eq!(s.overridden[0].winner, Some(b.who.clone()));
}

#[test]
fn a_folder_catch_up_spanning_several_pumps_is_not_cut_short() {
    let (_tmp, dir) = project_dir();
    let mut a = folder_client(&dir, "alice");
    let mut b = folder_client(&dir, "bob");
    a.session.config_mut().max_messages_per_pump = 4;
    let t0 = Instant::now();
    for _ in 0..3 {
        a.pump(t0);
        b.pump(t0);
    }
    let t1 = t0 + 60 * S;
    a.tracker.connection_lost_at(&a.stage, t1);
    let link = a.link.take();
    a.apply(t1 + S, set(&title(), "opacity", 50.0));
    // Bob writes a record per pump, the last one overrides Alice.
    for i in 0..10 {
        b.apply(t1 + S, set(&p("/comps/main"), "step", f64::from(i)));
        b.pump(t1 + S);
    }
    b.apply(t1 + S, set(&title(), "opacity", 80.0));
    b.pump(t1 + S);

    a.link = link;
    a.session.reset();
    let t2 = t1 + 60 * S;
    let mut pumps = 0;
    while !a.session.is_caught_up(&a.stage) {
        assert!(pumps < 20, "never caught up");
        a.pump(t2);
        pumps += 1;
    }
    assert!(pumps > 1, "{pumps}");
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    assert!(s.kept.is_empty(), "{s:#?}");
    assert_eq!(s.overridden.len(), 1, "{s:#?}");
    assert_eq!(s.overridden[0].result, value(80.0));
}

#[test]
fn is_caught_up_waits_for_the_updates_after_the_hello() {
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    b.apply(t1, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t1);

    connect(&mut relay, &mut a);
    a.session.config_mut().max_messages_per_pump = 1;
    assert!(a.session.remote_versions().is_none());
    a.pump(t1);
    relay.pump_at(t1);
    // Only the hub's hello so far: it has Bob's change, Alice not yet.
    a.pump(t1);
    let root = bae_sync::DocKey::Sublayer(SublayerId::root());
    let theirs = a.session.remote_versions().expect("hello")[&root].clone();
    assert!(!a.root().version().includes_vv(&theirs));
    assert!(!a.session.is_caught_up(&a.stage));
    for _ in 0..10 {
        if a.session.is_caught_up(&a.stage) {
            break;
        }
        a.pump(t1);
    }
    assert!(a.session.is_caught_up(&a.stage));
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(80.0)));
}

// ---- what is and is not an edit of ours, or theirs ----

#[test]
fn a_rollback_that_overwrote_ours_is_reported_and_a_losing_one_is_not() {
    let fail = |c: &mut Client, v: f64| {
        let r = c.stage.transact(&c.who, "script", |tx| {
            tx.apply(set(&title(), "opacity", v))?;
            Err::<(), _>(ModelError::NoSuchPrim(p("/nope")))
        });
        assert!(r.is_err());
    };
    let (mut relay, mut a, mut b, t0) = online();
    b.apply(t0, set(&title(), "opacity", 10.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0);

    // Bob changes it while Alice's script fails: its rollback writes 10 back
    // and wins by Loro order.
    b.apply(t0 + S, set(&title(), "opacity", 80.0));
    a.bump(t0 + S, 5);
    fail(&mut a, 99.0);
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert_eq!(b.value(&title(), "opacity"), Some(Value::Float(10.0)));
    let nb = b.take_notices();
    assert_eq!(nb.len(), 1, "{nb:#?}");
    assert_eq!(nb[0].outcome, ConflictOutcome::Overridden);
    assert_eq!(nb[0].result, value(10.0));
    assert_eq!(nb[0].winner, Some(a.who.clone()));
    // A failed transaction is no edit of Alice's to protect.
    assert!(a.take_notices().is_empty());

    // Bob's value wins over a failed script: nothing to report.
    b.bump(t0 + 3 * S, 10);
    b.apply(t0 + 3 * S, set(&title(), "opacity", 70.0));
    fail(&mut a, 98.0);
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 4 * S);
    assert_eq!(a.value(&title(), "opacity"), Some(Value::Float(70.0)));
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());
}

#[test]
fn a_failed_transaction_of_ours_is_not_remembered() {
    let (mut relay, mut a, mut b, t0) = online();
    let r = a.stage.transact(&a.who, "script", |tx| {
        tx.apply(set(&title(), "opacity", 99.0))?;
        Err::<(), _>(ModelError::NoSuchPrim(p("/nope")))
    });
    assert!(r.is_err());
    // The next local event covers the rollback commit too.
    a.apply(t0 + S, set(&title(), "scale", 2.0));
    assert_eq!(a.tracker.remembered(), 1);
    b.bump(t0 + S, 5);
    b.apply(t0 + S, set(&title(), "opacity", 80.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
}

#[test]
fn a_prim_we_removed_ourselves_is_not_reported_as_removed_by_another() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0, create(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t0);
    a.apply(t0 + S, set(&layer("x"), "opacity", 50.0));
    a.apply(t0 + S, delete(&layer("x")));
    // Bob removes it too, and his delete wins.
    b.bump(t0 + S, 3);
    b.apply(t0 + S, delete(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());

    // Offline, the same.
    let (mut relay, mut a, mut b, _, t1) = shared_layers();
    a.tracker.connection_lost_at(&a.stage, t1);
    relay.disconnect(relay.peers()[0].0);
    a.link = None;
    a.apply(t1 + S, set(&layer("x"), "opacity", 50.0));
    a.apply(t1 + S, delete(&layer("x")));
    b.bump(t1 + S, 5);
    b.apply(t1 + S, delete(&layer("x")));
    settle(&mut relay, &mut [&mut a, &mut b], t1 + S);
    connect(&mut relay, &mut a);
    settle(&mut relay, &mut [&mut a, &mut b], t1 + 2 * S);
    let s = a.tracker.caught_up(&a.stage).unwrap().unwrap().summary;
    assert!(s.overridden.is_empty(), "{s:#?}");
    let prims: Vec<_> = s.prims.iter().map(|p| (p.path.clone(), p.change)).collect();
    assert_eq!(prims, [(layer("x"), OfflinePrimChange::Removed)]);
    assert_eq!(s.conflicts(), 0);
}

#[test]
fn the_same_value_set_by_both_is_not_reported() {
    let (mut relay, mut a, mut b, t0) = online();
    a.apply(t0 + S, set(&title(), "opacity", 50.0));
    b.bump(t0 + S, 3);
    b.apply(t0 + S, set(&title(), "opacity", 50.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    // Bob's op won (Alice's "overridden" case), Alice's lost (Bob's "kept").
    assert!(a.take_notices().is_empty());
    assert!(b.take_notices().is_empty());
    assert_eq!(a.tracker.stats().elements_checked, 1);
    assert_eq!(b.tracker.stats().elements_checked, 1);
}

#[test]
fn a_shared_drag_is_reported_once_not_every_frame() {
    let (mut relay, mut a, mut b, t0) = online();
    let frame = Duration::from_millis(16);
    for i in 0..60u32 {
        let now = t0 + frame * i;
        a.apply(now, set(&title(), "opacity", f64::from(i)));
        b.apply(now, set(&title(), "opacity", 100.0 + f64::from(i)));
        settle(&mut relay, &mut [&mut a, &mut b], now);
    }
    // Equal Lamports every frame: the same side wins each time.
    let (na, nb) = (a.take_notices(), b.take_notices());
    assert_eq!((na.len(), nb.len()), (1, 1), "{na:#?} {nb:#?}");
    let held = a.tracker.stats().held_back + b.tracker.stats().held_back;
    assert_eq!(held, 2 * 59);
    let mut outcomes = [na[0].outcome, nb[0].outcome];
    outcomes.sort_by_key(|o| *o == ConflictOutcome::Kept);
    assert_eq!(
        outcomes,
        [ConflictOutcome::Overridden, ConflictOutcome::Kept]
    );

    // After a pause, a new conflict on it is reported again.
    let later = t0 + frame * 60 + 5 * S;
    a.apply(later, set(&title(), "opacity", 1.0));
    b.apply(later, set(&title(), "opacity", 2.0));
    settle(&mut relay, &mut [&mut a, &mut b], later);
    assert_eq!(a.take_notices().len() + b.take_notices().len(), 2);
}

// ---- bounds ----

#[test]
fn bulk_edits_are_not_remembered() {
    let (mut relay, mut a, mut b, t0) = online();
    a.tracker.config_mut().max_ops_per_edit = 10;
    a.stage
        .transact(&a.who, "paste", |tx| {
            for i in 0..20 {
                tx.apply(key(&title(), "position", i, f64::from(i as i32)))?;
            }
            Ok::<_, ModelError>(())
        })
        .map(|(_, e)| a.observe(&e, t0 + S))
        .unwrap();
    assert_eq!(a.tracker.stats().bulk_edits, 1);
    assert_eq!(a.tracker.remembered(), 0);
    b.bump(t0 + S, 5);
    b.apply(t0 + S, key(&title(), "position", 3, 99.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + 2 * S);
    assert!(a.take_notices().is_empty());
}

#[test]
fn remembered_edits_are_bounded() {
    let (_relay, mut a, _b, t0) = online();
    a.tracker.config_mut().max_recent = 8;
    // All at one instant: the bound does not depend on the clock moving.
    for i in 0..20 {
        a.apply(t0, key(&title(), "position", i, 1.0));
        assert!(a.tracker.remembered() <= 8, "{}", a.tracker.remembered());
    }
    // The newest are kept.
    let (mut relay, mut a, mut b, t0) = online();
    a.tracker.config_mut().max_recent = 8;
    for i in 0..20 {
        a.apply(t0, key(&title(), "position", i, 1.0));
    }
    b.bump(t0, 30);
    b.apply(t0, key(&title(), "position", 0, 5.0));
    b.apply(t0, key(&title(), "position", 19, 5.0));
    settle(&mut relay, &mut [&mut a, &mut b], t0 + S);
    let na = a.take_notices();
    assert_eq!(na.len(), 1, "{na:#?}");
    assert_eq!(na[0].element, Element::Keyframe(sec(19)));
}

#[test]
fn an_idle_check_costs_neither_per_remembered_edit_nor_per_peer() {
    let base = base_snapshots();
    let root = SublayerId::root();
    let mut a = Client::new(client_stage(&base), "alice");
    let t0 = Instant::now();
    // Every session that ever edited a sublayer is a peer in its history.
    let import = |a: &mut Client, i: usize, now: Instant| {
        let mut other = client_stage(&base);
        other
            .transact(&author("user:other"), "edit", |tx| {
                tx.apply(set(&p("/comps/main"), &format!("p{i}"), 1.0))
            })
            .unwrap();
        let have = a.root().version();
        let bytes = other
            .sublayer(&root)
            .unwrap()
            .export_updates(&have)
            .unwrap();
        let event = a.stage.import_updates(&root, &bytes).unwrap();
        a.observe(&event, now);
    };
    for i in 0..40 {
        import(&mut a, i, t0);
    }
    // One step that remembers a thousand keyframes.
    let (_, event) = a
        .stage
        .transact(&a.who, "keys", |tx| {
            for i in 0..1000 {
                tx.apply(key(&title(), "position", i, 1.0))?;
            }
            Ok::<_, ModelError>(())
        })
        .unwrap();
    a.observe(&event, t0);
    assert_eq!(a.tracker.remembered(), 1000);
    assert!(a.tracker.check_at(&a.stage, t0).is_empty());

    // Nothing committed or imported: a frontiers comparison per check.
    let before = a.tracker.stats().clone();
    for i in 0..200 {
        let now = t0 + Duration::from_millis(i);
        assert!(a.tracker.check_at(&a.stage, now).is_empty());
    }
    let idle = a.tracker.stats().clone();
    assert_eq!(idle.quick_checks - before.quick_checks, 200);
    assert_eq!(idle.versions_compared, before.versions_compared);

    // Others' ops on what we did not touch: a few comparisons per check (the
    // edits share one version), nothing decoded.
    for i in 0..20 {
        import(&mut a, 100 + i, t0 + S);
        assert!(a.tracker.check_at(&a.stage, t0 + S).is_empty());
    }
    let busy = a.tracker.stats();
    assert!(
        busy.versions_compared - idle.versions_compared <= 20 * 2,
        "{busy:?}"
    );
    assert_eq!((busy.elements_checked, busy.remote_ops), (0, 0));
    assert_eq!(busy.remote_events, 60);
}

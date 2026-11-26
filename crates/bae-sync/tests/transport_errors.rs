//! A transport that fails in the middle of a pump: what arrived before the error
//! is imported, and what the failed pump did is reported exactly once.

mod common;

use std::collections::VecDeque;

use bae_model::loro::VersionVector;
use bae_model::{EventOrigin, Op, PrimType, Specifier, Stage, SublayerId, Value};
use bae_sync::{
    ClientId, DocKey, DocSet, MemoryTransport, Message, Relay, SessionConfig, SyncError,
    SyncSession, Transport,
};
use common::*;

/// Plays a script: `recv` returns the queued results in order, then `Ok(None)`.
/// `send` records what was sent and fails for messages `fail_send` matches.
struct Scripted {
    script: VecDeque<Result<Option<Message>, SyncError>>,
    sent: Vec<Message>,
    fail_send: fn(&Message) -> bool,
}

impl Scripted {
    fn new(script: Vec<Result<Option<Message>, SyncError>>) -> Self {
        Scripted {
            script: script.into(),
            sent: Vec::new(),
            fail_send: |_| false,
        }
    }

    fn push(&mut self, msg: Message) {
        self.script.push_back(Ok(Some(msg)));
    }
}

impl Transport for Scripted {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        if (self.fail_send)(&msg) {
            return Err(SyncError::Closed);
        }
        self.sent.push(msg);
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        self.script.pop_front().unwrap_or(Ok(None))
    }
}

fn root() -> DocKey {
    DocKey::sublayer(&SublayerId::root())
}

fn session(name: &str) -> SyncSession {
    SyncSession::new(SessionConfig::new(ClientId::new(name).unwrap()))
}

/// What a collaborator working on a copy of `base` sends after setting `attr`
/// of the title layer to `value`.
fn collaborator_edit(base: &[(SublayerId, Vec<u8>)], who: &str, attr: &str, value: f64) -> Message {
    let mut stage = client_stage(base);
    stage.set_edit_target(&SublayerId::root()).unwrap();
    stage
        .transact(&author(who), "edit", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: attr.into(),
                value: Some(Value::Float(value)),
            })
        })
        .unwrap();
    let doc = root();
    let version = stage.version(&doc).unwrap();
    let data = stage
        .export_updates(&doc, &VersionVector::default())
        .unwrap();
    Message::Updates { doc, version, data }
}

fn version_of(msg: &Message) -> VersionVector {
    match msg {
        Message::Updates { version, .. } => version.clone(),
        other => panic!("not updates: {other:?}"),
    }
}

fn has(stage: &Stage, version: &VersionVector) -> bool {
    stage
        .version(&root())
        .is_some_and(|v| v.includes_vv(version))
}

#[test]
fn updates_received_before_a_transport_error_are_imported() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let mut session = session("alice");
    let edit = collaborator_edit(&base, "user:bob", "opacity", 50.0);
    let want = version_of(&edit);
    let mut link = Scripted::new(vec![Ok(Some(edit)), Err(SyncError::Closed)]);

    let err = session.pump(&mut stage, &mut link).unwrap_err();
    assert!(matches!(err, SyncError::Closed), "{err:?}");
    assert!(
        has(&stage, &want),
        "the edit received before the error is lost"
    );
}

#[test]
fn take_interrupted_reports_the_failed_pump_once() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let mut session = session("alice");
    let edit = collaborator_edit(&base, "user:bob", "opacity", 50.0);
    let mut link = Scripted::new(vec![Ok(Some(edit)), Err(SyncError::Closed)]);
    assert!(session.pump(&mut stage, &mut link).is_err());

    // Reconnecting does not lose it.
    session.reset();
    let failed = session
        .take_interrupted()
        .expect("outcome of the failed pump");
    assert_eq!(failed.received, 1);
    assert_eq!(failed.sent, 1, "the hello");
    assert_eq!(failed.events.len(), 1, "{:?}", failed.events);
    assert_eq!(failed.events[0].origin, EventOrigin::Remote);
    assert!(session.take_interrupted().is_none());

    let next = session.pump(&mut stage, &mut link).unwrap();
    assert!(next.events.is_empty(), "reported twice: {:?}", next.events);
    assert_eq!(next.received, 0);
}

#[test]
fn the_next_pump_reports_the_failed_pump_once() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let mut session = session("alice");
    let first = collaborator_edit(&base, "user:bob", "opacity", 50.0);
    let mut link = Scripted::new(vec![Ok(Some(first)), Err(SyncError::Closed)]);
    assert!(session.pump(&mut stage, &mut link).is_err());

    // The next pump reports the failed one along with its own work. (Another
    // attribute: a concurrent value for the same one might lose to Bob's and
    // change nothing.)
    let second = collaborator_edit(&base, "user:carol", "rotation", 75.0);
    let want = version_of(&second);
    link.push(second);
    let next = session.pump(&mut stage, &mut link).unwrap();
    assert!(has(&stage, &want));
    assert_eq!(next.received, 2);
    assert_eq!(next.sent, 1, "the hello of the failed pump");
    assert_eq!(next.events.len(), 2, "{:?}", next.events);
    assert!(next.events.iter().all(|e| e.origin == EventOrigin::Remote));

    assert!(session.take_interrupted().is_none());
    let after = session.pump(&mut stage, &mut link).unwrap();
    assert!(
        after.events.is_empty(),
        "reported twice: {:?}",
        after.events
    );
    assert_eq!((after.received, after.sent), (0, 0));
}

#[test]
fn updates_are_imported_when_answering_a_request_fails() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let mut session = session("alice");
    let edit = collaborator_edit(&base, "user:bob", "opacity", 50.0);
    let want = version_of(&edit);
    let mut link = Scripted::new(vec![
        Ok(Some(edit)),
        Ok(Some(Message::SnapshotRequest { doc: root() })),
    ]);
    link.fail_send = |m| matches!(m, Message::Snapshot { .. });

    let err = session.pump(&mut stage, &mut link).unwrap_err();
    assert!(matches!(err, SyncError::Closed), "{err:?}");
    assert!(
        has(&stage, &want),
        "the edit received before the error is lost"
    );
    assert!(
        matches!(link.sent.as_slice(), [Message::Hello { .. }]),
        "{:?}",
        link.sent
    );
    let failed = session.take_interrupted().unwrap();
    assert_eq!(failed.received, 2);
    assert_eq!(failed.events.len(), 1);
}

#[test]
fn the_relay_forwards_what_a_failing_peer_sent() {
    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    // Bob is connected and in sync.
    let (mut bob_link, hub_end) = MemoryTransport::pair();
    relay.connect(Box::new(hub_end));
    let mut bob = client_stage(&base);
    let mut bob_session = session("bob");
    for _ in 0..5 {
        bob_session.pump(&mut bob, &mut bob_link).unwrap();
        relay.pump();
    }
    assert_eq!(relay.pump().received, 0, "not settled");

    // Alice sends an edit, then her link fails in the same pump.
    let edit = collaborator_edit(&base, "user:alice", "opacity", 50.0);
    let want = version_of(&edit);
    let alice = relay.connect(Box::new(Scripted::new(vec![
        Ok(Some(edit)),
        Err(SyncError::Closed),
    ])));
    let out = relay.pump();
    assert_eq!(out.disconnected.len(), 1);
    assert_eq!(out.disconnected[0].0, alice);
    assert_eq!(out.received, 1);
    // The same relay pump already sent it on to Bob.
    bob_session.pump(&mut bob, &mut bob_link).unwrap();
    assert!(has(&bob, &want), "alice's last edit did not reach bob");
}

/// Create the layer `name` in sublayer `id` of `stage`.
fn create_layer(stage: &mut Stage, id: &SublayerId, name: &str) {
    stage.set_edit_target(id).unwrap();
    stage
        .transact(&author("user:me"), "edit", |tx| {
            tx.apply(Op::CreatePrim {
                path: p(&format!("{LAYERS}/{name}")),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
}

/// `Updates` of `doc` among `sent`.
fn updates_of(sent: &[Message], doc: &DocKey) -> usize {
    sent.iter()
        .filter(|m| matches!(m, Message::Updates { doc: d, .. } if d == doc))
        .count()
}

/// A transport that writes in the background (`ThreadedTransport` over the
/// shared folder) reports a document the folder refuses to store
/// (`NewerLogVersion`) from `recv`, once for every message of it that was
/// already queued. The pump does not fail, the refusal is reported once, the
/// document is not sent again and stays unsynced until `reset`, and the other
/// documents go on.
#[test]
fn a_document_refused_in_the_background_is_skipped_until_reset() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let anim = DocKey::sublayer(&sl("anim/a"));
    let hello = Message::hello(
        ClientId::new("shared-folder").unwrap(),
        stage
            .doc_keys()
            .into_iter()
            .map(|k| {
                let v = stage.version(&k).unwrap();
                (k, v)
            })
            .collect(),
    );
    let mut s = session("me");
    let mut t = Scripted::new(vec![Ok(Some(hello.clone()))]);
    s.pump(&mut stage, &mut t).unwrap();
    assert!(s.is_connected());
    // Two edits of the root sublayer go out before the first refusal is back.
    for name in ["one", "two"] {
        create_layer(&mut stage, &SublayerId::root(), name);
        s.pump(&mut stage, &mut t).unwrap();
    }
    assert_eq!(updates_of(&t.sent, &root()), 2);
    let refusal = || {
        Err(SyncError::NewerLogVersion {
            doc: root(),
            path: "updates/root/me.log".into(),
            version: 2,
        })
    };
    t.script.extend([refusal(), refusal()]);
    create_layer(&mut stage, &sl("anim/a"), "three");
    let out = s
        .pump(&mut stage, &mut t)
        .expect("only the document is refused");
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(
        matches!(&out.errors[0], SyncError::NewerLogVersion { doc, .. } if doc == &root()),
        "{:?}",
        out.errors
    );
    assert_eq!(updates_of(&t.sent, &root()), 2, "sent again");
    assert_eq!(updates_of(&t.sent, &anim), 1);
    // Sent, but not stored.
    assert_eq!(s.unsynced(&stage), vec![root()]);
    create_layer(&mut stage, &SublayerId::root(), "four");
    let out = s.pump(&mut stage, &mut t).unwrap();
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert_eq!(updates_of(&t.sent, &root()), 2, "sent again");
    assert_eq!(s.unsynced(&stage), vec![root()]);
    // Reconnected (e.g. the folder reopened): sent again.
    s.reset();
    t.push(hello);
    s.pump(&mut stage, &mut t).unwrap();
    assert_eq!(updates_of(&t.sent, &root()), 3);
    assert!(s.unsynced(&stage).is_empty());
}

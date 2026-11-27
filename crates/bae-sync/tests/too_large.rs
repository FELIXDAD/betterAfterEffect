//! A document whose update does not fit in one message (spec 2.4): the
//! session skips it and reports it once; the link and the other documents go
//! on. While collaborators edit the document, the remote's version of it keeps
//! changing, but not in a way that can make the update smaller, so it is not
//! exported again. It is tried again once the remote got more of the missing
//! ops (here: by another route), or after `reset`. Ported from #760. A
//! presence that does not fit is skipped the same way.

mod common;

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use bae_model::loro::{ExportMode, LoroDoc, VersionVector};
use bae_sync::{
    ClientId, DocKey, DocSet, Imported, MemDocs, MemoryTransport, Message, PresenceState, Relay,
    SessionConfig, SyncError, SyncSession, Transport,
};
use common::*;

/// Largest message either side may send.
const LIMIT: usize = 64 << 10;

/// 100 ms of simulated time per round.
const TICK: Duration = Duration::from_millis(100);

/// A lossless link that refuses messages over [`LIMIT`], as `HubTransport`
/// does.
struct Limited(MemoryTransport);

impl Transport for Limited {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let size = msg.encoded_len();
        if size > LIMIT {
            return Err(SyncError::MessageTooLarge {
                doc: msg.doc().cloned(),
                size,
                max: LIMIT,
            });
        }
        self.0.send(msg)
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        self.0.recv()
    }

    fn max_message_size(&self) -> Option<usize> {
        Some(LIMIT)
    }
}

/// [`MemDocs`] that counts how often the session exported anything of one
/// document, and records which documents it exported snapshots of.
struct Counted {
    docs: MemDocs,
    doc: DocKey,
    exports: Cell<usize>,
    snapshots: RefCell<Vec<DocKey>>,
}

impl DocSet for Counted {
    fn doc_keys(&self) -> Vec<DocKey> {
        self.docs.doc_keys()
    }

    fn version(&self, doc: &DocKey) -> Option<VersionVector> {
        self.docs.version(doc)
    }

    fn export_updates(&self, doc: &DocKey, since: &VersionVector) -> Result<Vec<u8>, SyncError> {
        if doc == &self.doc {
            self.exports.set(self.exports.get() + 1);
        }
        self.docs.export_updates(doc, since)
    }

    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, SyncError> {
        if doc == &self.doc {
            self.exports.set(self.exports.get() + 1);
        }
        self.snapshots.borrow_mut().push(doc.clone());
        self.docs.export_snapshot(doc)
    }

    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError> {
        self.docs.import(doc, data)
    }
}

/// `a` has a large document (`big`) the remote `b` lacks a large part of,
/// and a small one (`small`); both pump once per round on simulated time.
struct Pair {
    a: Counted,
    b: MemDocs,
    sa: SyncSession,
    sb: SyncSession,
    ta: Limited,
    tb: Limited,
    t0: Instant,
    round: u32,
    /// `MessageTooLarge` errors `a` reported.
    too_large: usize,
}

fn big() -> DocKey {
    DocKey::parse("big").unwrap()
}

fn small() -> DocKey {
    DocKey::parse("small").unwrap()
}

impl Pair {
    fn new() -> Pair {
        let base = LoroDoc::new();
        base.get_text("t").insert(0, "base").unwrap();
        base.commit();
        let base = base.export(ExportMode::Snapshot).unwrap();
        let mut a = MemDocs::new();
        let mut b = MemDocs::new();
        for doc in [big(), small()] {
            a.load(doc.clone(), &base).unwrap();
            b.load(doc, &base).unwrap();
        }
        let (ta, tb) = MemoryTransport::pair();
        let config = |name: &str| {
            let mut c = SessionConfig::new(ClientId::new(name).unwrap());
            c.hello_interval = Some(Duration::from_millis(500));
            c
        };
        Pair {
            a: Counted {
                docs: a,
                doc: big(),
                exports: Cell::new(0),
                snapshots: RefCell::new(Vec::new()),
            },
            b,
            sa: SyncSession::new(config("a")),
            sb: SyncSession::new(config("b")),
            ta: Limited(ta),
            tb: Limited(tb),
            t0: Instant::now(),
            round: 0,
            too_large: 0,
        }
    }

    /// One round; a link error fails the test.
    fn pump(&mut self) {
        let now = self.t0 + TICK * self.round;
        self.round += 1;
        let out = self.sa.pump_at(&mut self.a, &mut self.ta, now).unwrap();
        for e in out.errors {
            assert!(
                matches!(&e, SyncError::MessageTooLarge { doc: Some(d), max: LIMIT, .. } if *d == big()),
                "{e:?}"
            );
            self.too_large += 1;
        }
        let out = self.sb.pump_at(&mut self.b, &mut self.tb, now).unwrap();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
    }

    /// A small edit by `b`, which reaches `a` and changes what `a` knows `b`
    /// has of the document.
    fn b_edits(&self, doc: &DocKey) {
        let d = self.b.get(doc).unwrap();
        d.get_text("t").insert(0, "b").unwrap();
        d.commit();
    }

    fn a_edits(&self, doc: &DocKey, text: &str) {
        let d = self.a.docs.get(doc).unwrap();
        d.get_text("t").insert(0, text).unwrap();
        d.commit();
    }

    /// Whether `b` has everything `a` has of `doc`.
    fn b_has_all_of(&self, doc: &DocKey) -> bool {
        let want = self.a.docs.version(doc).unwrap();
        self.b.version(doc).is_some_and(|v| v.includes_vv(&want))
    }
}

fn letters(len: usize, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    (0..len)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect()
}

/// Before #760's rule (and on the code it replaces here, where an oversized
/// update failed the whole link), the session tried again whenever the
/// remote's version of the document changed: every edit `b` sent made `a`
/// export the whole update again, only to fail again and report it again
/// (the hub logs each).
#[test]
fn an_oversized_update_is_not_exported_again_while_the_remote_edits() {
    let mut p = Pair::new();
    for _ in 0..3 {
        p.pump();
    }
    assert_eq!(p.too_large, 0);

    // 256 KiB of new text does not fit in a 64 KiB message.
    p.a_edits(&big(), &letters(256 << 10, 1));
    let exports = p.a.exports.get();
    p.pump();
    assert_eq!(p.too_large, 1);
    assert_eq!(p.a.exports.get(), exports + 1);

    // `b` keeps editing the document (and says hello every 5 rounds,
    // acknowledging `a`'s): `a` imports each edit, but does not try the large
    // update again. `a`'s edits of the other document go through meanwhile.
    for i in 0..20 {
        p.b_edits(&big());
        if i % 5 == 0 {
            p.a_edits(&small(), "s");
        }
        p.pump();
    }
    // `a` gets the last one, `b` the last edit of `small`.
    p.pump();
    let b_has = p.b.version(&big()).unwrap();
    assert!(
        p.a.docs
            .version(&big())
            .is_some_and(|v| v.includes_vv(&b_has)),
        "a did not get b's edits"
    );
    assert!(p.b_has_all_of(&small()), "the other document did not sync");
    assert_eq!(p.too_large, 1, "the update was tried again");
    assert_eq!(
        p.a.exports.get(),
        exports + 1,
        "the update was exported again"
    );
    // Neither on its way nor known by `b`: unsynced, although `b`'s `Hello`s
    // acknowledged `a`'s. `b` knows it lacks it, and which document.
    assert_eq!(p.sa.unsynced(&p.a), [big()]);
    assert_eq!(p.sa.too_large(&p.a), [big()]);
    assert!(!p.sb.is_caught_up(&p.b));
    assert_eq!(p.sb.behind(&p.b), [big()]);
    assert!(p.sa.is_caught_up(&p.a));
    assert!(p.sa.behind(&p.a).is_empty());

    // A local edit only makes the update larger: not tried again.
    p.a_edits(&big(), "a");
    for _ in 0..10 {
        p.pump();
    }
    assert_eq!(p.too_large, 1);
    assert_eq!(p.a.exports.get(), exports + 1);

    // A reconnect forgets it: tried once more after `b`'s next hello, then
    // not again.
    p.sa.reset();
    for _ in 0..20 {
        p.b_edits(&big());
        p.pump();
    }
    assert_eq!(p.too_large, 2, "not tried exactly once after reset");
    assert_eq!(p.a.exports.get(), exports + 2);

    // `b` gets part of what it lacks another way, but what is left still
    // does not fit: tried once more and reported again (spec 2.4 step 3:
    // reported again whenever a retry does not fit), then not again.
    let part =
        p.a.docs
            .export_updates(&big(), &p.b.version(&big()).unwrap())
            .unwrap();
    p.a_edits(&big(), &letters(256 << 10, 3));
    p.b.import(&big(), &part).unwrap();
    for _ in 0..20 {
        p.b_edits(&big());
        p.pump();
    }
    assert_eq!(p.too_large, 3, "a partial gain is not tried exactly once");
    assert_eq!(p.a.exports.get(), exports + 3);
    assert_eq!(p.sa.too_large(&p.a), [big()]);

    // `b` gets the large edit another way (say, a shared folder). Its next
    // hello shows it has more of `a`'s ops, the update is small now, and `a`'s
    // newer edits go out again.
    let snapshot = p.a.docs.export_snapshot(&big()).unwrap();
    p.b.import(&big(), &snapshot).unwrap();
    p.a_edits(&big(), "after");
    for _ in 0..20 {
        p.pump();
    }
    assert!(p.b_has_all_of(&big()), "a's later edit did not arrive");
    assert_eq!(p.too_large, 3);
    assert!(p.sa.unsynced(&p.a).is_empty());
    assert!(p.sa.too_large(&p.a).is_empty());
    assert!(p.sb.is_caught_up(&p.b));
    assert!(p.sb.behind(&p.b).is_empty());
}

/// A snapshot answer that does not fit: reported once, the link stays, and
/// later requests for the document are not answered (a snapshot only grows),
/// so a remote that keeps asking does not make `a` export the whole document
/// on every request. Tried again after `reset`.
#[test]
fn an_oversized_snapshot_is_not_exported_on_every_request() {
    let mut p = Pair::new();
    p.a_edits(&big(), &letters(256 << 10, 2));
    p.pump();
    p.pump();
    assert_eq!(p.too_large, 1);
    let exports = p.a.exports.get();
    // Without the memo, each of these requests exports the snapshot again
    // (10 exports; spec 2.4 cites this).
    for _ in 0..10 {
        p.tb.send(Message::SnapshotRequest { doc: big() }).unwrap();
        p.pump();
    }
    assert_eq!(p.too_large, 2);
    assert_eq!(p.a.exports.get(), exports + 1);
    assert_eq!(p.sa.pending_snapshot_requests(), 0);
    // Other answers still go out (`b` has all of `small`, so only the
    // answer can bring it anything: count the export and the arrival).
    assert!(p.b_has_all_of(&small()));
    p.a.snapshots.borrow_mut().clear();
    let arrived = |p: &Pair| p.sb.stats().imports + p.sb.stats().redundant;
    let before = arrived(&p);
    p.tb.send(Message::SnapshotRequest { doc: small() })
        .unwrap();
    p.pump();
    p.pump();
    assert_eq!(*p.a.snapshots.borrow(), [small()]);
    assert_eq!(arrived(&p), before + 1, "the answer did not arrive");
    assert_eq!(p.sa.pending_snapshot_requests(), 0);

    p.sa.reset();
    p.tb.send(Message::SnapshotRequest { doc: big() }).unwrap();
    for _ in 0..10 {
        p.pump();
    }
    // The update and the snapshot, once each.
    assert_eq!(p.too_large, 4);
    assert_eq!(p.a.exports.get(), exports + 3);
}

/// A collaborator's presence that does not fit in one message to a client (a
/// very long selection, a link that takes little): the relay skips it for
/// that client and reports it once, and the client stays connected. Before,
/// the relay dropped the client; it reconnected, got the same presence and
/// was dropped again for as long as the selection stayed.
#[test]
fn a_presence_that_does_not_fit_is_skipped_by_the_relay() {
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let mut sessions = Vec::new();
    let mut links = Vec::new();
    let mut handles = Vec::new();
    let mut docs = Vec::new();
    for (i, limited) in [false, false, true].into_iter().enumerate() {
        let (mine, theirs) = MemoryTransport::pair();
        handles.push(if limited {
            relay.connect(Box::new(Limited(theirs)))
        } else {
            relay.connect(Box::new(theirs))
        });
        links.push(mine);
        docs.push(MemDocs::new());
        let mut config = SessionConfig::new(ClientId::new(&format!("p{i}")).unwrap());
        config.hello_interval = Some(Duration::from_secs(5));
        sessions.push(SyncSession::new(config));
    }
    let t0 = Instant::now();
    let mut now = Duration::ZERO;
    // Pumps everyone twice; returns what the relay reported.
    let mut step = |sessions: &mut Vec<SyncSession>, dt: Duration| {
        now += dt;
        let mut too_large = Vec::new();
        for _ in 0..2 {
            for i in 0..sessions.len() {
                let out = sessions[i]
                    .pump_at(&mut docs[i], &mut links[i], t0 + now)
                    .unwrap();
                assert!(out.errors.is_empty(), "{:?}", out.errors);
            }
            let out = relay.pump_at(t0 + now);
            assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
            too_large.extend(out.errors);
        }
        too_large
    };
    let names = |s: &SyncSession| -> Vec<String> {
        s.presence().into_iter().map(|p| p.state.name).collect()
    };
    let presence = |name: &str, selected: usize| PresenceState {
        name: name.to_owned(),
        selection: (0..selected)
            .map(|i| p(&format!("/comps/main/layers/layer_{i:05}")))
            .collect(),
        ..PresenceState::default()
    };

    // About 100 KiB of selection: more than the third client takes.
    sessions[0].set_presence(presence("alice", 3000));
    sessions[1].set_presence(presence("bob", 0));
    let mut too_large = step(&mut sessions, Duration::from_millis(10));
    assert_eq!(names(&sessions[1]), ["alice"]);
    assert_eq!(names(&sessions[2]), ["bob"]);
    assert!(
        matches!(
            &too_large[..],
            [(h, SyncError::MessageTooLarge { doc: None, max: LIMIT, .. })] if *h == handles[2]
        ),
        "{too_large:?}"
    );
    // Heartbeats keep it going to the others, and are neither sent to the
    // third client nor reported again; nobody is dropped.
    for _ in 0..6 {
        too_large.extend(step(&mut sessions, Duration::from_secs(5)));
    }
    assert_eq!(too_large.len(), 1, "{too_large:?}");
    assert_eq!(names(&sessions[1]), ["alice"]);
    assert_eq!(names(&sessions[2]), ["bob"]);
    // A smaller selection reaches it.
    sessions[0].set_presence(presence("alice", 3));
    too_large.extend(step(&mut sessions, Duration::from_millis(10)));
    assert_eq!(names(&sessions[2]), ["alice", "bob"]);
    // Too large again after one fitted: reported again.
    sessions[0].set_presence(presence("alice", 3000));
    too_large.extend(step(&mut sessions, Duration::from_millis(10)));
    assert_eq!(too_large.len(), 2, "{too_large:?}");
}

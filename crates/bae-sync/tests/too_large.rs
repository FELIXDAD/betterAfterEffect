//! A document whose update does not fit in one message: the session skips it
//! and reports it once. While collaborators edit the document, the remote's
//! version of it keeps changing, but not in a way that can make the update
//! smaller, so it is not exported again. It is tried again once the remote got
//! more of the missing ops (here: by another route), or after `reset`.

mod common;

use std::cell::Cell;
use std::time::{Duration, Instant};

use bae_model::loro::{ExportMode, LoroDoc, VersionVector};
use bae_sync::{
    ClientId, DocKey, DocSet, Imported, MemDocs, MemoryTransport, Message, SessionConfig,
    SyncError, SyncSession, Transport,
};
use common::*;

/// Largest message either side may send.
const LIMIT: usize = 64 << 10;

/// 100 ms of simulated time per round.
const TICK: Duration = Duration::from_millis(100);

/// A lossless link that refuses messages over [`LIMIT`], as `HubTransport` does.
struct Limited(MemoryTransport);

impl Transport for Limited {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let size = msg.encoded_len();
        if size > LIMIT {
            return Err(SyncError::MessageTooLarge { size, max: LIMIT });
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
/// document.
struct Counted {
    docs: MemDocs,
    doc: DocKey,
    exports: Cell<usize>,
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
        self.docs.export_snapshot(doc)
    }

    fn import(&mut self, doc: &DocKey, data: &[u8]) -> Result<Imported, SyncError> {
        self.docs.import(doc, data)
    }
}

/// `a` has a document the remote `b` lacks a large part of; both pump once per
/// round on simulated time.
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

impl Pair {
    fn new(doc: &DocKey) -> Pair {
        let base = LoroDoc::new();
        base.get_text("t").insert(0, "base").unwrap();
        base.commit();
        let base = base.export(ExportMode::Snapshot).unwrap();
        let mut a = MemDocs::new();
        a.load(doc.clone(), &base).unwrap();
        let mut b = MemDocs::new();
        b.load(doc.clone(), &base).unwrap();
        let (ta, tb) = MemoryTransport::pair();
        let config = |name: &str| {
            let mut c = SessionConfig::new(ClientId::new(name).unwrap());
            c.hello_interval = Some(Duration::from_millis(500));
            c
        };
        Pair {
            a: Counted {
                docs: a,
                doc: doc.clone(),
                exports: Cell::new(0),
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

    fn pump(&mut self) {
        let now = self.t0 + TICK * self.round;
        self.round += 1;
        let out = self.sa.pump_at(&mut self.a, &mut self.ta, now).unwrap();
        for e in out.errors {
            assert!(matches!(e, SyncError::MessageTooLarge { .. }), "{e:?}");
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
}

fn letters(len: usize, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    (0..len)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect()
}

/// Before, the session tried again whenever the remote's version of the
/// document changed: every edit `b` sent made `a` export the whole update
/// again, only to fail again and report it again (the hub logs each).
#[test]
fn an_oversized_update_is_not_exported_again_while_the_remote_edits() {
    let doc = DocKey::parse("big").unwrap();
    let mut p = Pair::new(&doc);
    for _ in 0..3 {
        p.pump();
    }
    assert_eq!(p.too_large, 0);

    // 256 KiB of new text does not fit in a 64 KiB message.
    p.a_edits(&doc, &letters(256 << 10, 1));
    let exports = p.a.exports.get();
    p.pump();
    assert_eq!(p.too_large, 1);
    assert_eq!(p.a.exports.get(), exports + 1);

    // `b` keeps editing the document (and says hello every 5 rounds): `a`
    // imports each edit, but does not try the large update again.
    for _ in 0..20 {
        p.b_edits(&doc);
        p.pump();
    }
    // `a` gets the last one.
    p.pump();
    let b_has = p.b.version(&doc).unwrap();
    assert!(
        p.a.docs
            .version(&doc)
            .is_some_and(|v| v.includes_vv(&b_has)),
        "a did not get b's edits"
    );
    assert_eq!(p.too_large, 1, "the update was tried again");
    assert_eq!(
        p.a.exports.get(),
        exports + 1,
        "the update was exported again"
    );

    // A reconnect forgets it: tried once more after `b`'s next hello, then
    // not again.
    p.sa.reset();
    for _ in 0..20 {
        p.b_edits(&doc);
        p.pump();
    }
    assert_eq!(p.too_large, 2, "not tried exactly once after reset");
    assert_eq!(p.a.exports.get(), exports + 2);

    // `b` gets the large edit another way (say, a shared folder). Its next
    // hello shows it has more of `a`'s ops, the update is small now, and `a`'s
    // newer edits go out again.
    let snapshot = p.a.docs.export_snapshot(&doc).unwrap();
    p.b.import(&doc, &snapshot).unwrap();
    p.a_edits(&doc, "after");
    let want = p.a.docs.version(&doc).unwrap();
    for _ in 0..20 {
        p.pump();
    }
    assert!(
        p.b.version(&doc).is_some_and(|v| v.includes_vv(&want)),
        "a's later edit did not arrive"
    );
    assert_eq!(p.too_large, 2);
}

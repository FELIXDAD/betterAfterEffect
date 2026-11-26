//! Size limits of the hub transport (`HubLimits`) and the session's backpressure
//! over real WebSocket connections on loopback: messages over tungstenite's old
//! 16 MiB frame limit go through, messages over the limit are refused with a
//! clear error, a peer that does not read makes a bare `send` fail once the
//! write buffer cap is reached, and sessions never get there: they send a
//! project larger than the cap one document at a time, and hold back what a
//! peer that does not read would get.
#![cfg(feature = "hub")]

mod common;

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bae_model::SublayerId;
use bae_model::loro::{LoroDoc, VersionVector};
use bae_sync::{
    ClientId, DocKey, DocSet, HubLimits, HubTransport, MemDocs, MemoryTransport, Message, Relay,
    SessionConfig, SyncError, SyncSession, Transport,
};
use common::*;

/// A connected client (`connect_with`) and server (`accept_with`) on loopback.
fn pair(client: HubLimits, server: HubLimits) -> (HubTransport, HubTransport) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let accept = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        HubTransport::accept_with(stream, None, server).unwrap()
    });
    let client = HubTransport::connect_with(
        &format!("ws://{addr}/projects/film"),
        None,
        Duration::from_secs(5),
        client,
    )
    .unwrap();
    (client, accept.join().unwrap())
}

/// `len` pseudo-random (incompressible) bytes.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        out.extend_from_slice(&rng.next().to_le_bytes());
    }
    out.truncate(len);
    out
}

fn snapshot(data: Vec<u8>) -> Message {
    Message::Snapshot {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: VersionVector::default(),
        data,
    }
}

/// Send `msg` from `from` and pump both ends until `to` has a message.
fn deliver(
    msg: Message,
    from: &mut HubTransport,
    to: &mut HubTransport,
) -> Result<Message, SyncError> {
    from.send(msg)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        // `recv` also writes out what `send` left queued.
        assert!(from.recv()?.is_none(), "unexpected message back");
        if let Some(m) = to.recv()? {
            return Ok(m);
        }
        assert!(Instant::now() < deadline, "message not delivered");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Small limits: 64 KiB messages, the smallest write buffer cap for them.
fn small() -> HubLimits {
    limits(64 << 10)
}

/// Messages of at most `max` bytes, the smallest write buffer cap for them.
fn limits(max: usize) -> HubLimits {
    HubLimits {
        max_message_size: max,
        max_write_buffer_size: 0,
    }
}

/// `len` letters from `seed`: Loro cannot compress them much.
fn letters(len: usize, seed: u64) -> String {
    noise(len, seed)
        .iter()
        .map(|b| char::from(b'a' + b % 26))
        .collect()
}

/// `n` documents `doc0`… of about `len` bytes each (a text of random letters).
fn big_docs(n: usize, len: usize) -> MemDocs {
    let mut docs = MemDocs::new();
    for i in 0..n {
        let doc = LoroDoc::new();
        doc.get_text("t")
            .insert(0, &letters(len, i as u64))
            .unwrap();
        doc.commit();
        let key = DocKey::parse(&format!("doc{i}")).unwrap();
        docs.load(
            key,
            &doc.export(bae_model::loro::ExportMode::Snapshot).unwrap(),
        )
        .unwrap();
    }
    docs
}

/// Every document of `a` is in `b` at the same version.
fn same_docs(a: &MemDocs, b: &MemDocs) -> bool {
    a.doc_keys()
        .iter()
        .all(|k| a.version(k).is_some() && b.version(k) == a.version(k))
}

/// Records the largest backlog the wrapped transport ever reported.
struct Watched<T> {
    inner: T,
    max: Arc<AtomicUsize>,
}

impl<T: Transport> Watched<T> {
    fn note(&self) {
        self.max.fetch_max(self.inner.backlog(), Ordering::Relaxed);
    }
}

impl<T: Transport> Transport for Watched<T> {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let r = self.inner.send(msg);
        self.note();
        r
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        let r = self.inner.recv();
        self.note();
        r
    }

    fn backlog(&self) -> usize {
        self.inner.backlog()
    }
}

#[test]
fn default_limits_and_the_write_buffer_floor() {
    let d = HubLimits::default();
    assert_eq!(d.max_message_size, 256 << 20);
    assert_eq!(d.max_message_size, HubLimits::DEFAULT_MAX_MESSAGE_SIZE);
    assert_eq!(d.max_write_buffer_size, (256 + 16) << 20);
    assert_eq!(d.write_buffer_cap(), d.max_write_buffer_size);
    assert_eq!(HubLimits::with_max_message_size(256 << 20), d);

    // A cap too small for one message of the largest size is raised: room for
    // the message plus 2 MiB (what a session may still have queued when it
    // starts the message, and small messages behind it).
    let s = small();
    assert_eq!(s.min_write_buffer_size(), (64 << 10) + (2 << 20));
    assert_eq!(s.write_buffer_cap(), s.min_write_buffer_size());
    let huge = HubLimits::with_max_message_size(usize::MAX);
    assert_eq!(huge.write_buffer_cap(), usize::MAX);
}

/// tungstenite's default frame limit is 16 MiB and it never splits outgoing
/// messages, so with its default settings this fails on the receiving side.
#[test]
fn messages_over_16_mib_go_through_both_ways() {
    let (mut client, mut server) = pair(HubLimits::default(), HubLimits::default());
    let data = noise(20 << 20, 1);
    let msg = snapshot(data);
    assert!(msg.encode().len() > 16 << 20);

    let got = deliver(msg.clone(), &mut client, &mut server).expect("client → hub");
    assert!(got == msg, "client → hub: the snapshot arrived damaged");
    let got = deliver(msg.clone(), &mut server, &mut client).expect("hub → client");
    assert!(got == msg, "hub → client: the snapshot arrived damaged");
}

/// Pump `to` until a message arrives.
fn receive(to: &mut HubTransport) -> Result<Message, SyncError> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(m) = to.recv()? {
            return Ok(m);
        }
        assert!(Instant::now() < deadline, "message not delivered");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Builds before `HubLimits` use tungstenite's default settings: they send
/// each message as one frame and accept messages up to 64 MiB, so a hub of
/// that kind relays a 20 MiB edit to a new client as a single 20 MiB frame.
/// New builds accept frames up to their message limit (they only send 256 KiB
/// frames themselves). Before, they refused frames over 16 MiB at the frame
/// header, and the client reconnected and got the same frame again, forever.
#[test]
fn a_single_large_frame_from_an_older_build_is_received() {
    let msg = snapshot(noise(20 << 20, 5));
    let bytes = msg.encode();
    assert!(bytes.len() > 16 << 20);

    // Old hub → new client.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let old_hub = std::thread::spawn({
        let bytes = bytes.clone();
        move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept(stream).unwrap();
            ws.send(tungstenite::Message::Binary(bytes.into())).unwrap();
            // Until the client hangs up.
            while ws.read().is_ok() {}
        }
    });
    let url = format!("ws://{addr}/projects/film");
    let mut client = HubTransport::connect(&url, None, Duration::from_secs(5)).unwrap();
    let got = receive(&mut client).expect("old hub → new client");
    assert!(
        got == msg,
        "old hub → new client: the snapshot arrived damaged"
    );
    drop(client);
    old_hub.join().unwrap();

    // Old client → new hub.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/projects/film", listener.local_addr().unwrap());
    let old_client = std::thread::spawn(move || {
        let (mut ws, _) = tungstenite::connect(url).unwrap();
        ws.send(tungstenite::Message::Binary(bytes.into())).unwrap();
        while ws.read().is_ok() {}
    });
    let (stream, _) = listener.accept().unwrap();
    let mut hub = HubTransport::accept(stream, None).unwrap();
    let got = receive(&mut hub).expect("old client → new hub");
    assert!(
        got == msg,
        "old client → new hub: the snapshot arrived damaged"
    );
    drop(hub);
    old_client.join().unwrap();
}

#[test]
fn messages_over_the_limit_are_refused() {
    // The sender checks its own limit before sending anything; only that
    // message is refused, the link stays usable.
    let (mut client, mut server) = pair(small(), small());
    let big = snapshot(noise(100 << 10, 2));
    let size = big.encode().len();
    match client.send(big.clone()) {
        Err(SyncError::MessageTooLarge { size: s, max }) => {
            assert_eq!((s, max), (size, 64 << 10));
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(client.backlog(), 0, "nothing was queued");
    // A message right at the limit goes through, both ways.
    let fits = snapshot(noise((64 << 10) - 64, 3));
    assert!(fits.encode().len() <= 64 << 10);
    let got = deliver(fits.clone(), &mut client, &mut server).expect("at the limit");
    assert!(got == fits);
    let got = deliver(fits.clone(), &mut server, &mut client).expect("at the limit");
    assert!(got == fits);

    // The receiver refuses a message over its limit (mismatched limits) with an
    // error that says so.
    let (mut client, mut server) = pair(HubLimits::default(), small());
    match deliver(big, &mut client, &mut server) {
        Err(SyncError::Hub(m)) => assert!(m.contains("over the limit of 65536"), "{m}"),
        other => panic!("expected the hub to refuse, got {other:?}"),
    }
}

#[test]
fn send_fails_once_a_peer_that_does_not_read_fills_the_write_buffer() {
    let limits = small();
    let (_lazy, mut server) = pair(limits, limits);
    let chunk = snapshot(noise(60 << 10, 4));
    let size = chunk.encode().len();
    let mut sent = 0usize;
    let err = loop {
        match server.send(chunk.clone()) {
            Ok(()) => sent += size,
            Err(e) => break e,
        }
        // The kernel's socket buffers take a few MiB before writes block.
        assert!(
            sent < 256 << 20,
            "{sent} bytes buffered and send still succeeds"
        );
    };
    eprintln!(
        "send failed after {} KiB (write buffer cap {} KiB)",
        sent >> 10,
        limits.write_buffer_cap() >> 10
    );
    match err {
        SyncError::Hub(m) => assert!(m.contains("write buffer full"), "{m}"),
        other => panic!("expected write buffer full, got {other:?}"),
    }
    assert!(
        sent >= limits.write_buffer_cap() - size,
        "failed early: {sent}"
    );
}

/// A peer that does not read for a while (busy, or on a slow link): the relay
/// holds back what it would get instead of queuing it, so the queue stays at
/// about `SessionConfig::max_backlog` plus one message and the peer is not
/// dropped. Once it reads again it catches up. With the session's old "send
/// everything at once", the queue reached the write buffer cap and the relay
/// dropped the peer.
#[test]
fn relay_holds_back_a_peer_that_does_not_read() {
    let limits = limits(512 << 10);
    let (mut lazy, hub_end) = pair(limits, limits);
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let most = Arc::new(AtomicUsize::new(0));
    let lazy_handle = relay.connect(Box::new(Watched {
        inner: hub_end,
        max: most.clone(),
    }));
    // It says hello (so the relay sends it everything), then does not read.
    lazy.send(Message::hello(ClientId::new("lazy").unwrap(), Vec::new()))
        .unwrap();

    // An editor with many small documents, so that no document's backlog grows
    // beyond one message.
    let n = 128;
    let mut editor = big_docs(n, 1 << 10);
    let (mut link, editor_end) = MemoryTransport::pair();
    relay.connect(Box::new(editor_end));
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("editor").unwrap()));
    // 24 MiB of edits, far more than the cap (2.5 MiB) and the socket buffers.
    for k in 0..768 {
        let key = DocKey::parse(&format!("doc{}", k % n)).unwrap();
        let doc = editor.get(&key).unwrap();
        doc.get_text("t")
            .insert(0, &letters(32 << 10, k as u64))
            .unwrap();
        doc.commit();
        session.pump(&mut editor, &mut link).unwrap();
        let out = relay.pump();
        assert!(
            out.disconnected.is_empty(),
            "after {k} edits: {:?}",
            out.disconnected
        );
        assert!(out.errors.is_empty(), "{:?}", out.errors);
    }
    let most = most.load(Ordering::Relaxed);
    eprintln!(
        "largest backlog for the peer that does not read: {} KiB",
        most >> 10
    );
    let bound = SessionConfig::DEFAULT_MAX_BACKLOG + (512 << 10);
    assert!(
        most <= bound,
        "backlog reached {most} bytes (bound {bound})"
    );
    assert!(relay.peers().iter().any(|(h, _)| *h == lazy_handle));

    // It reads again and gets everything.
    let mut docs = MemDocs::new();
    let mut lazy_session = SyncSession::new(SessionConfig::new(ClientId::new("lazy").unwrap()));
    let deadline = Instant::now() + Duration::from_secs(60);
    while !same_docs(&editor, &docs) {
        lazy_session.pump(&mut docs, &mut lazy).unwrap();
        let out = relay.pump();
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        assert!(Instant::now() < deadline, "the peer did not catch up");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A first sync much larger than the write buffer cap, both ways: a client
/// uploads a project to an empty relay (e.g. after the hub lost its data), then
/// a new client downloads it. Both sessions send one document at a time as the
/// socket drains. With the session's old "export everything the remote lacks
/// and send it in one pump", the sender's queue went over the cap and the link
/// was dropped on every attempt.
#[test]
fn a_first_sync_larger_than_the_write_buffer_cap_goes_through() {
    let limits = limits(512 << 10);
    let project = big_docs(40, 400 << 10);
    let mut relay = Relay::new(ClientId::new("hub").unwrap());

    // Upload: 16 MiB, the cap is 2.5 MiB.
    let (up_link, hub_end) = pair(limits, limits);
    let most = Arc::new(AtomicUsize::new(0));
    relay.connect(Box::new(hub_end));
    let mut up_link = Watched {
        inner: up_link,
        max: most.clone(),
    };
    let mut uploader = SyncSession::new(SessionConfig::new(ClientId::new("up").unwrap()));
    let mut up_docs = project;
    let deadline = Instant::now() + Duration::from_secs(60);
    while !same_docs(&up_docs, relay.docs()) {
        uploader
            .pump(&mut up_docs, &mut up_link)
            .expect("uploader link");
        let out = relay.pump();
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        assert!(Instant::now() < deadline, "upload did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    let bound = SessionConfig::DEFAULT_MAX_BACKLOG + (512 << 10);
    let most = most.load(Ordering::Relaxed);
    assert!(most <= bound, "upload backlog reached {most} bytes");
    // Each document went out once.
    let total: u64 = up_docs
        .doc_keys()
        .iter()
        .map(|k| {
            let all = up_docs.export_updates(k, &VersionVector::default());
            all.unwrap().len() as u64
        })
        .sum();
    let sent = uploader.stats().bytes_sent;
    assert!(sent < total + total / 4, "sent {sent} bytes for {total}");

    // Download into an empty client.
    let (mut down_link, hub_end) = pair(limits, limits);
    let most = Arc::new(AtomicUsize::new(0));
    relay.connect(Box::new(Watched {
        inner: hub_end,
        max: most.clone(),
    }));
    let mut downloader = SyncSession::new(SessionConfig::new(ClientId::new("down").unwrap()));
    let mut down_docs = MemDocs::new();
    while !same_docs(&up_docs, &down_docs) {
        downloader
            .pump(&mut down_docs, &mut down_link)
            .expect("downloader link");
        let out = relay.pump();
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        assert!(Instant::now() < deadline, "download did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    let most = most.load(Ordering::Relaxed);
    assert!(most <= bound, "download backlog reached {most} bytes");
}

/// One document larger than a message can hold does not stop the others: the
/// relay reports it, sends the rest, and keeps the link.
#[test]
fn a_document_over_the_message_limit_does_not_block_the_others() {
    let limits = limits(256 << 10);
    let mut relay = Relay::with_docs(ClientId::new("hub").unwrap(), {
        // `doc0` is the big one; `doc1`… sort after it.
        let mut docs = big_docs(1, 400 << 10);
        let small = big_docs(6, 8 << 10);
        for k in small.doc_keys().into_iter().skip(1) {
            docs.load(k.clone(), &small.export_snapshot(&k).unwrap())
                .unwrap();
        }
        docs
    });
    let big = DocKey::parse("doc0").unwrap();
    let size = relay.docs().export_snapshot(&big).unwrap().len();
    assert!(size > 256 << 10, "the big document is only {size} bytes");
    // Hellos both ways while the test runs.
    relay.set_hello_interval(Some(Duration::from_millis(20)));
    let (mut link, hub_end) = pair(limits, limits);
    let handle = relay.connect(Box::new(hub_end));
    let mut config = SessionConfig::new(ClientId::new("c").unwrap());
    config.hello_interval = Some(Duration::from_millis(20));
    let mut session = SyncSession::new(config);
    let mut docs = MemDocs::new();
    let mut too_large = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    while docs.doc_keys().len() < 5 {
        session.pump(&mut docs, &mut link).expect("client link");
        let out = relay.pump();
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        for (h, e) in out.errors {
            assert_eq!(h, handle);
            assert!(matches!(e, SyncError::MessageTooLarge { .. }), "{e:?}");
            too_large += 1;
        }
        assert!(
            Instant::now() < deadline,
            "the small documents did not arrive"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // A few more rounds (hellos included): reported once, not exported again
    // on every pump.
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        session.pump(&mut docs, &mut link).expect("client link");
        too_large += relay.pump().errors.len();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(too_large, 1);
    assert!(docs.version(&big).is_none());
}

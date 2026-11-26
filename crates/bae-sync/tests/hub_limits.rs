//! Size limits of the hub transport (`HubLimits`) over real WebSocket connections
//! on loopback: messages over tungstenite's old 16 MiB frame limit go through,
//! messages over the limit are refused with a clear error, and a peer that does
//! not read makes `send` fail once the write buffer cap is reached instead of
//! buffering without end.
#![cfg(feature = "hub")]

mod common;

use std::net::TcpListener;
use std::time::{Duration, Instant};

use bae_model::loro::VersionVector;
use bae_model::{Op, SublayerId, Value};
use bae_sync::{
    ClientId, DocKey, HubLimits, HubTransport, MemoryTransport, Message, Relay, SessionConfig,
    SyncError, SyncSession, Transport,
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
    HubLimits {
        max_message_size: 64 << 10,
        max_write_buffer_size: 0,
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
    // tungstenite's write buffer (128 KiB) plus one frame (header included).
    let s = small();
    assert_eq!(s.min_write_buffer_size(), (128 << 10) + (64 << 10) + 14);
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

#[test]
fn messages_over_the_limit_are_refused() {
    // The sender checks its own limit before sending anything.
    let (mut client, mut server) = pair(small(), small());
    let big = snapshot(noise(100 << 10, 2));
    match client.send(big.clone()) {
        Err(SyncError::Hub(m)) => assert!(m.contains("over the limit"), "{m}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    // A message right at the limit goes through.
    let fits = snapshot(noise((64 << 10) - 64, 3));
    assert!(fits.encode().len() <= 64 << 10);
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

/// The relay drops a peer whose write buffer is full at once (its session's
/// send fails), instead of waiting for a timeout.
#[test]
fn relay_drops_a_hub_peer_that_does_not_read() {
    let limits = small();
    let (mut lazy, hub_end) = pair(limits, limits);
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let lazy_handle = relay.connect(Box::new(hub_end));
    // It says hello (so the relay sends it everything), then never reads.
    lazy.send(Message::hello(ClientId::new("lazy").unwrap(), Vec::new()))
        .unwrap();

    let base = base_snapshots();
    let (mut link, editor_end) = MemoryTransport::pair();
    relay.connect(Box::new(editor_end));
    let mut stage = client_stage(&base);
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("editor").unwrap()));
    let who = author("user:editor");
    for k in 0..2000u64 {
        // Each edit is about 30 KiB on the wire, under the 64 KiB limit.
        let text: String = noise(30 << 10, k)
            .iter()
            .map(|b| char::from(b'a' + b % 26))
            .collect();
        stage.set_edit_target(&SublayerId::root()).unwrap();
        stage
            .transact(&who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "note".into(),
                    value: Some(Value::String(text)),
                })
            })
            .unwrap();
        session.pump(&mut stage, &mut link).unwrap();
        let out = relay.pump();
        if let Some((_, e)) = out.disconnected.iter().find(|(h, _)| *h == lazy_handle) {
            eprintln!("dropped after {} edits", k + 1);
            assert!(
                matches!(e, SyncError::Hub(m) if m.contains("write buffer full")),
                "{e:?}"
            );
            assert!(relay.peers().iter().all(|(h, _)| *h != lazy_handle));
            assert_eq!(relay.peers().len(), 1, "the editor stays connected");
            return;
        }
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
    }
    panic!("the relay never dropped the peer that does not read");
}

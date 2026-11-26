//! Hub mode over real WebSocket connections on loopback: a test hub (the
//! `Relay` behind `HubTransport::accept`) and clients using
//! `ThreadedTransport(HubTransport::connect)`.
#![cfg(feature = "hub")]

mod common;

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{
    ClientId, HubTransport, PresenceState, Relay, SessionConfig, SyncError, SyncSession,
    ThreadedTransport,
};
use common::*;

struct TestHub {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TestHub {
    fn start(token: Option<&'static str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut relay = Relay::new(ClientId::new("hub").unwrap());
            relay.set_hello_interval(Some(Duration::from_millis(500)));
            while !flag.load(Ordering::Relaxed) {
                if let Ok((stream, _)) = listener.accept()
                    && let Ok(t) = HubTransport::accept(stream, token)
                {
                    relay.connect(Box::new(t));
                }
                relay.pump();
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        TestHub {
            addr,
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> String {
        format!("ws://{}/projects/film", self.addr)
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}

struct Client {
    stage: Stage,
    session: SyncSession,
    link: ThreadedTransport,
    who: AuthorId,
    rng: Rng,
    n: u32,
    tag: String,
}

fn connect(url: &str, token: Option<&str>) -> Result<ThreadedTransport, SyncError> {
    let hub = HubTransport::connect(url, token, Duration::from_secs(5))?;
    Ok(ThreadedTransport::spawn(
        hub,
        Duration::from_millis(2),
        || {},
    ))
}

impl Client {
    fn new(i: usize, url: &str, base: &[(SublayerId, Vec<u8>)]) -> Self {
        let id = format!("w{i}");
        let mut config = SessionConfig::new(ClientId::new(&id).unwrap());
        config.hello_interval = Some(Duration::from_millis(500));
        Client {
            stage: client_stage(base),
            session: SyncSession::new(config),
            link: connect(url, Some("team-secret")).unwrap(),
            who: author(&format!("user:{id}")),
            rng: Rng::new(900 + i as u64),
            n: 0,
            tag: id,
        }
    }

    fn pump(&mut self) -> Result<bae_sync::PumpOutcome, SyncError> {
        self.session.pump(&mut self.stage, &mut self.link)
    }
}

fn wait_until(what: &str, timeout: Duration, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !f() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn agreed(clients: &[Client]) -> bool {
    let first = fingerprint(&clients[0].stage);
    clients.iter().all(|c| fingerprint(&c.stage) == first)
}

#[test]
fn clients_sync_and_see_each_other_through_a_websocket_hub() {
    let hub = TestHub::start(Some("team-secret"));
    let base = base_snapshots();
    let mut clients: Vec<Client> = (0..3).map(|i| Client::new(i, &hub.url(), &base)).collect();
    clients[0].session.set_presence(PresenceState {
        name: "w0".into(),
        ..PresenceState::default()
    });
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1) {
        for c in &mut clients {
            if c.rng.chance(40) {
                random_edit(&mut c.stage, &c.who, &mut c.rng, &c.tag, &mut c.n);
            }
            c.pump().unwrap();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    wait_until("convergence", Duration::from_secs(10), || {
        for c in &mut clients {
            c.pump().unwrap();
        }
        agreed(&clients)
            && clients
                .iter()
                .all(|c| c.session.unsynced(&c.stage).is_empty())
    });
    assert!(
        clients[0]
            .stage
            .sublayer(&SublayerId::root())
            .unwrap()
            .len()
            > 5
    );
    wait_until("presence", Duration::from_secs(5), || {
        clients[2].pump().unwrap();
        clients[2].session.presence().len() == 1
    });

    // Round trip of one edit from one client to another over loopback.
    let mut samples = Vec::new();
    for k in 0..20 {
        let sent = Instant::now();
        let a = &mut clients[0];
        a.stage.set_edit_target(&SublayerId::root()).unwrap();
        a.stage
            .transact(&a.who, "probe", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main"),
                    attr: "probe".into(),
                    value: Some(Value::Int(k)),
                })
            })
            .unwrap();
        a.pump().unwrap();
        wait_until("edit to arrive", Duration::from_secs(5), || {
            clients[1].pump().unwrap();
            clients[1]
                .stage
                .prim(&p("/comps/main"))
                .and_then(|m| m.value("probe").cloned())
                == Some(Value::Int(k))
        });
        samples.push(sent.elapsed());
        clients[0].pump().unwrap();
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    eprintln!(
        "edit latency client → hub → client over loopback: median {median:?}, max {:?}",
        samples.last().unwrap()
    );
    assert!(median < Duration::from_millis(100), "{median:?}");
}

#[test]
fn wrong_or_missing_token_is_refused() {
    let hub = TestHub::start(Some("team-secret"));
    for token in [None, Some("guess")] {
        match HubTransport::connect(&hub.url(), token, Duration::from_secs(5)) {
            Err(SyncError::Hub(msg)) => assert!(msg.contains("401"), "{msg}"),
            other => panic!("expected refusal, got {other:?}"),
        }
    }
    assert!(matches!(
        HubTransport::connect("wss://example.invalid/x", None, Duration::from_secs(1)),
        Err(SyncError::Hub(_))
    ));
}

#[test]
fn clients_reconnect_after_the_hub_restarts() {
    let base = base_snapshots();
    let hub = TestHub::start(Some("team-secret"));
    let mut clients: Vec<Client> = (0..2).map(|i| Client::new(i, &hub.url(), &base)).collect();
    for c in &mut clients {
        for _ in 0..20 {
            random_edit(&mut c.stage, &c.who, &mut c.rng, &c.tag, &mut c.n);
        }
    }
    wait_until("first convergence", Duration::from_secs(10), || {
        for c in &mut clients {
            c.pump().unwrap();
        }
        agreed(&clients)
    });

    // The hub goes away (and forgets everything). Clients notice, keep editing
    // offline, then reconnect to the new hub and catch up.
    drop(hub);
    wait_until("disconnect noticed", Duration::from_secs(10), || {
        clients
            .iter_mut()
            .all(|c| matches!(c.pump(), Err(SyncError::Closed)))
    });
    for c in &mut clients {
        for _ in 0..10 {
            random_edit(&mut c.stage, &c.who, &mut c.rng, &c.tag, &mut c.n);
        }
    }
    let hub = TestHub::start(Some("team-secret"));
    for c in &mut clients {
        c.link = connect(&hub.url(), Some("team-secret")).unwrap();
        c.session.reset();
    }
    wait_until("convergence after restart", Duration::from_secs(10), || {
        for c in &mut clients {
            c.pump().unwrap();
        }
        agreed(&clients)
    });
    // A newcomer gets everything from the new hub.
    let mut late = Client::new(7, &hub.url(), &base);
    wait_until("late joiner", Duration::from_secs(10), || {
        late.pump().unwrap();
        fingerprint(&late.stage) == fingerprint(&clients[0].stage)
    });
}

/// A raw client/server pair on loopback, without a relay.
fn raw_pair(server_limits: bae_sync::HubLimits) -> (HubTransport, HubTransport) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        HubTransport::accept_with(stream, None, server_limits).unwrap()
    });
    let client = HubTransport::connect(
        &format!("ws://{addr}/projects/film"),
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    (client, server.join().unwrap())
}

#[test]
fn messages_larger_than_one_default_frame_get_through() {
    use bae_sync::{DocKey, HubLimits, Message, Transport};
    // tungstenite refuses frames over 16 MiB by default, and a message is one frame.
    let (mut client, mut server) = raw_pair(HubLimits::default());
    let data: Vec<u8> = (0..20u32 << 20).map(|i| (i % 251) as u8).collect();
    let sent = Message::Snapshot {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: data.clone(),
    };
    client.send(sent).unwrap();
    let mut got = None;
    wait_until("the large snapshot", Duration::from_secs(20), || {
        client.recv().unwrap(); // drives the rest of the send
        got = server.recv().unwrap();
        got.is_some()
    });
    match got.unwrap() {
        Message::Snapshot { data: d, .. } => assert!(d == data),
        other => panic!("unexpected {other:?}"),
    }

    // Over the limit: refused when sending, not silently dropped by the peer.
    client.set_limits(HubLimits {
        max_message: 1 << 20,
        ..HubLimits::default()
    });
    assert_eq!(client.limits().max_message, 1 << 20);
    let too_big = Message::Updates {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: vec![0; 2 << 20],
    };
    assert!(client.send(too_big).is_err());
}

#[test]
fn a_peer_that_stops_reading_is_cut_off() {
    use bae_sync::{DocKey, HubLimits, Message, Transport};
    let limits = HubLimits {
        max_message: 1 << 20,
        max_backlog: 1 << 20,
        stall_timeout: Duration::from_millis(300),
    };
    // The client never reads; the server keeps sending and polling, as a relay
    // does.
    let (_client, mut server) = raw_pair(limits);
    let chunk = Message::Updates {
        doc: DocKey::sublayer(&SublayerId::root()),
        version: Default::default(),
        data: vec![7; 256 << 10],
    };
    let start = Instant::now();
    let mut sent = 0;
    let e = loop {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{sent} messages queued for a peer that does not read"
        );
        if let Err(e) = server
            .send(chunk.clone())
            .and_then(|()| server.recv().map(drop))
        {
            break e;
        }
        sent += 1;
        std::thread::sleep(Duration::from_millis(5));
    };
    // Socket buffers take a few MiB, then more than `max_backlog` waits with
    // nothing going out for `stall_timeout`.
    assert!(start.elapsed() >= limits.stall_timeout);
    assert!(sent >= 4, "failed after only {sent} messages: {e}");
    assert!(e.to_string().contains("not reading"), "{e}");
}

/// A peer that reads, however slowly, is not cut off because much waits for it:
/// a session sends everything a newcomer lacks back to back (on a first sync,
/// the whole project).
#[test]
fn a_slow_reader_gets_a_large_batch() {
    use bae_sync::{DocKey, HubLimits, Message, Transport};
    const N: usize = 10;
    let limits = HubLimits {
        max_message: 4 << 20,
        max_backlog: 1 << 20,
        ..HubLimits::default()
    };
    let (mut client, mut server) = raw_pair(limits);
    // About 120 MB/s.
    let reader = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut got = 0;
        while got < N && Instant::now() < deadline {
            match client.recv().unwrap() {
                Some(_) => {
                    got += 1;
                    std::thread::sleep(Duration::from_millis(25));
                }
                None => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        // Still connected: the server may be polling.
        (got, client)
    });
    let batch: Vec<Message> = (0..N)
        .map(|i| Message::Updates {
            doc: DocKey::sublayer(&SublayerId::root()),
            version: Default::default(),
            data: vec![i as u8; 3 << 20],
        })
        .collect();
    for (i, msg) in batch.into_iter().enumerate() {
        if let Err(e) = server.send(msg) {
            panic!("message {i} of {N}: {e}");
        }
    }
    while !reader.is_finished() {
        server.recv().unwrap(); // drives the rest of the sends
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(reader.join().unwrap().0, N);
}

/// The peer sent its last message and went away (a reset) while we were still
/// sending to it: the failed write is reported only after that message.
///
/// Linux only: Windows (and possibly other stacks) throw away received but
/// unread data when a reset arrives, so the message is gone before we could
/// read it. Clients therefore end with `Transport::close`, which waits for the
/// close handshake (`closing_a_hub_client_delivers_its_last_message`).
#[cfg(target_os = "linux")]
#[test]
fn a_message_that_arrived_before_a_reset_is_still_read() {
    use bae_sync::{DocKey, HubLimits, Message, Transport};
    let (mut client, mut server) = raw_pair(HubLimits::default());
    let doc = DocKey::sublayer(&SublayerId::root());
    // More than the socket buffers take: the rest waits in the server.
    server
        .send(Message::Updates {
            doc: doc.clone(),
            version: Default::default(),
            data: vec![0; 40 << 20],
        })
        .unwrap();
    client
        .send(Message::Updates {
            doc,
            version: Default::default(),
            data: vec![1, 2, 3],
        })
        .unwrap();
    // Closing a socket with unread data resets the connection.
    std::thread::sleep(Duration::from_millis(50));
    drop(client);
    std::thread::sleep(Duration::from_millis(50));
    match server.recv() {
        Ok(Some(Message::Updates { data, .. })) => assert_eq!(data, [1, 2, 3]),
        other => panic!("the last message was lost: {other:?}"),
    }
    assert!(server.recv().is_err());
}

/// `close` on the usual client stack returns once the hub has read everything:
/// a large last message is not cut off when the connection goes away.
#[test]
fn closing_a_hub_client_delivers_its_last_message() {
    use bae_sync::{DocKey, HubLimits, Message, Transport};
    let (client, mut server) = raw_pair(HubLimits::default());
    // The hub is busy for a moment: the message does not fit in the socket
    // buffers by the time `close` is called.
    let reader = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        let mut got = None;
        loop {
            match server.recv() {
                Ok(Some(m)) => got = Some(m),
                Ok(None) => std::thread::sleep(Duration::from_millis(1)),
                Err(e) => return (got, e),
            }
        }
    });
    let mut client = ThreadedTransport::spawn(client, Duration::from_millis(1), || {});
    client
        .send(Message::Updates {
            doc: DocKey::sublayer(&SublayerId::root()),
            version: Default::default(),
            data: vec![9; 20 << 20],
        })
        .unwrap();
    client.close().unwrap();
    let (got, e) = reader.join().unwrap();
    match got {
        Some(Message::Updates { data, .. }) => assert_eq!(data.len(), 20 << 20),
        other => panic!("the last message was lost ({e}): {other:?}"),
    }
    assert!(matches!(e, SyncError::Closed), "{e}");
}

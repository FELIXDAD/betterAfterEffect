//! N clients edit concurrently over lossy, duplicating, reordering links and must
//! end up with identical projects (workstream 09 acceptance).

mod common;

use std::time::{Duration, Instant};

use bae_model::{AuthorId, EventOrigin, Op, PrimType, Specifier, Stage, Sublayer, SublayerId};
use bae_sync::{ClientId, DocKey, DocSet, Relay, SessionConfig, SyncSession};
use common::*;

struct Client {
    stage: Stage,
    session: SyncSession,
    link: SimEnd,
    who: AuthorId,
    rng: Rng,
    n: u32,
    tag: String,
}

impl Client {
    fn new(i: usize, seed: u64, base: &[(SublayerId, Vec<u8>)], link: SimEnd) -> Self {
        let mut config = SessionConfig::new(ClientId::new(&format!("c{i}")).unwrap());
        config.hello_interval = Some(Duration::from_millis(500));
        Client {
            stage: client_stage(base),
            session: SyncSession::new(config),
            link,
            who: author(&format!("user:c{i}")),
            rng: Rng::new(seed * 1000 + i as u64),
            n: 0,
            tag: format!("c{i}"),
        }
    }

    fn pump(&mut self, now: Instant) {
        let out = self
            .session
            .pump_at(&mut self.stage, &mut self.link, now)
            .unwrap();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        for ev in &out.events {
            assert_eq!(ev.origin, EventOrigin::Remote);
            assert!(ev.author.is_none());
        }
    }

    fn edit(&mut self) {
        random_edit(
            &mut self.stage,
            &self.who,
            &mut self.rng,
            &self.tag,
            &mut self.n,
        );
    }
}

fn converged(clients: &[Client], relay: Option<&Relay>) -> bool {
    let first = fingerprint(&clients[0].stage);
    let versions = |s: &Stage| -> Vec<_> {
        s.layer_stack()
            .iter()
            .map(|id| s.sublayer(id).unwrap().version())
            .collect()
    };
    let v0 = versions(&clients[0].stage);
    clients
        .iter()
        .all(|c| fingerprint(&c.stage) == first && versions(&c.stage) == v0)
        && relay.is_none_or(|r| {
            clients[0]
                .stage
                .layer_stack()
                .iter()
                .zip(&v0)
                .all(|(id, v)| r.docs().version(&DocKey::sublayer(id)).as_ref() == Some(v))
        })
}

/// Edit for `steps` ticks over bad links, then keep pumping (links still bad)
/// until everyone agrees.
fn run_hub(
    seed: u64,
    n_clients: usize,
    steps: usize,
    base: &[(SublayerId, Vec<u8>)],
) -> (Vec<Client>, Relay) {
    let net = SimNet::new(seed, Conditions::BAD);
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    relay.set_hello_interval(Some(Duration::from_millis(500)));
    let mut clients: Vec<Client> = (0..n_clients)
        .map(|i| {
            let (client_end, hub_end) = net.link();
            relay.connect(Box::new(hub_end));
            Client::new(i, seed, base, client_end)
        })
        .collect();
    let t0 = Instant::now();
    for _ in 0..steps {
        for c in &mut clients {
            if c.rng.chance(60) {
                c.edit();
            }
            c.pump(net.instant(t0));
        }
        let out = relay.pump_at(net.instant(t0));
        assert!(out.disconnected.is_empty() && out.errors.is_empty());
        net.tick();
    }
    let mut rounds = 0;
    while !converged(&clients, Some(&relay)) {
        rounds += 1;
        assert!(rounds < 5000, "no convergence after {rounds} rounds");
        for c in &mut clients {
            c.pump(net.instant(t0));
        }
        relay.pump_at(net.instant(t0));
        net.tick();
    }
    let (delivered, dropped) = net.stats();
    assert!(dropped > 0, "the simulation must actually lose packets");
    eprintln!(
        "seed {seed}: {n_clients} clients, {steps} steps, converged after {rounds} extra rounds \
         ({delivered} delivered, {dropped} dropped)"
    );
    (clients, relay)
}

#[test]
fn hub_clients_converge_over_lossy_links() {
    for seed in 1..=4 {
        let (clients, relay) = run_hub(seed, 4, 150, &base_snapshots());
        // Sanity: the run did real work and every client announced itself.
        assert!(
            clients[0]
                .stage
                .sublayer(&SublayerId::root())
                .unwrap()
                .len()
                > 5
        );
        assert!(relay.peers().iter().all(|(_, id)| id.is_some()));
    }
}

#[test]
fn peers_converge_without_a_hub() {
    let net = SimNet::new(7, Conditions::BAD);
    let base = base_snapshots();
    let (a_end, b_end) = net.link();
    let mut clients = vec![
        Client::new(0, 7, &base, a_end),
        Client::new(1, 7, &base, b_end),
    ];
    let t0 = Instant::now();
    for _ in 0..200 {
        for c in &mut clients {
            if c.rng.chance(50) {
                c.edit();
            }
            c.pump(net.instant(t0));
        }
        net.tick();
    }
    let mut rounds = 0;
    while !converged(&clients, None) {
        rounds += 1;
        assert!(rounds < 5000);
        for c in &mut clients {
            c.pump(net.instant(t0));
        }
        net.tick();
    }
}

#[test]
fn hub_restart_and_late_joiner() {
    let base = base_snapshots();
    let (mut clients, relay) = run_hub(11, 3, 80, &base);
    let expected = fingerprint(&clients[0].stage);
    drop(relay);

    // The hub comes back empty; clients reconnect, a new client joins with only the
    // base project. Everyone ends up with the full project again.
    let net = SimNet::new(12, Conditions::BAD);
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    relay.set_hello_interval(Some(Duration::from_millis(500)));
    for c in &mut clients {
        let (client_end, hub_end) = net.link();
        relay.connect(Box::new(hub_end));
        c.link = client_end;
        c.session.reset();
    }
    let (client_end, hub_end) = net.link();
    relay.connect(Box::new(hub_end));
    clients.push(Client::new(9, 12, &base, client_end));

    let t0 = Instant::now();
    let mut rounds = 0;
    while !converged(&clients, Some(&relay)) {
        rounds += 1;
        assert!(rounds < 5000);
        for c in &mut clients {
            c.pump(net.instant(t0));
        }
        relay.pump_at(net.instant(t0));
        net.tick();
    }
    assert_eq!(fingerprint(&clients[3].stage), expected);
}

#[test]
fn sublayer_created_by_a_collaborator() {
    let net = SimNet::new(3, Conditions::PERFECT);
    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let mut clients: Vec<Client> = (0..2)
        .map(|i| {
            let (client_end, hub_end) = net.link();
            relay.connect(Box::new(hub_end));
            Client::new(i, 3, &base, client_end)
        })
        .collect();
    let t0 = Instant::now();
    let step = |clients: &mut Vec<Client>, relay: &mut Relay| {
        let mut unknown = Vec::new();
        for c in clients.iter_mut() {
            let out = c
                .session
                .pump_at(&mut c.stage, &mut c.link, net.instant(t0))
                .unwrap();
            unknown.push(out.unknown_docs);
        }
        relay.pump_at(net.instant(t0));
        net.tick();
        unknown
    };
    for _ in 0..3 {
        step(&mut clients, &mut relay);
    }

    // Client 0 adds a sublayer and edits it.
    let fx = sl("fx");
    let a = &mut clients[0];
    a.stage.create_sublayer(fx.clone(), 0).unwrap();
    a.stage.set_edit_target(&fx).unwrap();
    a.stage
        .transact(&a.who, "fx", |tx| {
            tx.apply(Op::CreatePrim {
                path: p("/fx"),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Folder")),
                index: None,
            })
        })
        .unwrap();

    // Client 1 is told about it but cannot import it yet.
    let mut told = false;
    for _ in 0..4 {
        told |= step(&mut clients, &mut relay)[1].contains(&DocKey::sublayer(&fx));
    }
    assert!(told);
    assert!(clients[1].stage.sublayer(&fx).is_none());

    // Once the app adds the sublayer, its contents arrive.
    clients[1]
        .stage
        .insert_sublayer(Sublayer::new(fx.clone()), 0)
        .unwrap();
    for _ in 0..4 {
        step(&mut clients, &mut relay);
    }
    assert!(clients[1].stage.sublayer(&fx).unwrap().has_spec(&p("/fx")));
    assert!(converged(&clients, Some(&relay)));
    assert!(clients[1].session.unsynced(&clients[1].stage).is_empty());
}

#[test]
fn nothing_is_sent_before_the_remote_says_hello() {
    let base = base_snapshots();
    let mut stage = client_stage(&base);
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
    let (mut mine, mut theirs) = bae_sync::MemoryTransport::pair();
    session.pump(&mut stage, &mut mine).unwrap();
    // Only the hello went out.
    use bae_sync::Transport;
    assert!(matches!(
        theirs.recv().unwrap(),
        Some(bae_sync::Message::Hello { .. })
    ));
    assert!(theirs.recv().unwrap().is_none());
    assert_eq!(session.unsynced(&stage).len(), 2);
    assert!(!session.is_connected());
    // The remote has nothing: everything goes out.
    theirs
        .send(bae_sync::Message::hello(
            ClientId::new("b").unwrap(),
            vec![],
        ))
        .unwrap();
    session.pump(&mut stage, &mut mine).unwrap();
    let mut updates = 0;
    while let Some(m) = theirs.recv().unwrap() {
        updates += usize::from(matches!(m, bae_sync::Message::Updates { .. }));
    }
    assert_eq!(updates, 2);
    assert!(session.unsynced(&stage).is_empty());
    assert_eq!(session.remote_client().unwrap().as_str(), "b");
    assert!(stage.version(&DocKey::History).is_none());
}

/// Delivers what is queued, then fails once `dead` is set: a WebSocket whose
/// client sent its last edits and closed in the same breath.
struct DyingLink {
    inner: bae_sync::MemoryTransport,
    dead: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl bae_sync::Transport for DyingLink {
    fn send(&mut self, msg: bae_sync::Message) -> Result<(), bae_sync::SyncError> {
        if self.dead.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(bae_sync::SyncError::Closed);
        }
        self.inner.send(msg)
    }

    fn recv(&mut self) -> Result<Option<bae_sync::Message>, bae_sync::SyncError> {
        match self.inner.recv()? {
            Some(m) => Ok(Some(m)),
            None if self.dead.load(std::sync::atomic::Ordering::Relaxed) => {
                Err(bae_sync::SyncError::Closed)
            }
            None => Ok(None),
        }
    }
}

#[test]
fn edits_received_just_before_a_link_fails_are_kept() {
    edit_sent_just_before_a_link_fails(false);
}

/// As above, but the relay's periodic `Hello` to the leaving client is due in
/// that pump: its send fails before anything is read.
#[test]
fn edits_received_just_before_a_link_fails_are_kept_when_a_hello_is_due() {
    edit_sent_just_before_a_link_fails(true);
}

fn edit_sent_just_before_a_link_fails(hello_due: bool) {
    use bae_sync::MemoryTransport;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    // The client that stays is pumped first in each relay round: it only gets
    // the edit if the relay goes round once more after the failure.
    let (hub_end, mut staying_end) = MemoryTransport::pair();
    relay.connect(Box::new(hub_end));
    let dead = Arc::new(AtomicBool::new(false));
    let (hub_end, mut leaving_end) = MemoryTransport::pair();
    relay.connect(Box::new(DyingLink {
        inner: hub_end,
        dead: dead.clone(),
    }));
    let mut leaving = client_stage(&base);
    let mut leaving_session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
    let mut staying = client_stage(&base);
    let mut staying_session = SyncSession::new(SessionConfig::new(ClientId::new("b").unwrap()));
    for _ in 0..3 {
        leaving_session
            .pump(&mut leaving, &mut leaving_end)
            .unwrap();
        staying_session
            .pump(&mut staying, &mut staying_end)
            .unwrap();
        relay.pump();
    }
    assert!(leaving_session.is_connected() && staying_session.is_connected());

    // The last edit goes out, and the link fails before the hub reads it.
    let who = author("user:a");
    leaving
        .transact(&who, "last edit", |tx| {
            tx.apply(Op::CreatePrim {
                path: p("/last"),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Folder")),
                index: None,
            })
        })
        .unwrap();
    leaving_session
        .pump(&mut leaving, &mut leaving_end)
        .unwrap();
    assert!(leaving_session.unsynced(&leaving).is_empty());
    dead.store(true, Ordering::Relaxed);

    // One relay pump: the hub imports the edit, drops the link and passes the
    // edit on to the client that stayed.
    let out = if hello_due {
        relay.pump_at(Instant::now() + Duration::from_secs(60))
    } else {
        relay.pump()
    };
    assert_eq!(out.disconnected.len(), 1);
    let root = DocKey::sublayer(&SublayerId::root());
    assert!(
        relay
            .docs()
            .version(&root)
            .unwrap()
            .includes_vv(&leaving.version(&root).unwrap()),
        "the hub lost the edit sent just before the link failed"
    );
    staying_session
        .pump(&mut staying, &mut staying_end)
        .unwrap();
    assert!(
        staying
            .sublayer(&SublayerId::root())
            .unwrap()
            .has_spec(&p("/last"))
    );
    assert_eq!(fingerprint(&staying), fingerprint(&leaving));
}

/// Everything waiting on `end`: `(hellos, updates)`; checks every hello's client.
fn drain(end: &mut bae_sync::MemoryTransport, hello_from: &str) -> (usize, usize) {
    use bae_sync::{Message, Transport};
    let (mut hellos, mut updates) = (0, 0);
    while let Some(m) = end.recv().unwrap() {
        match m {
            Message::Hello { client, .. } => {
                assert_eq!(client.as_str(), hello_from);
                hellos += 1;
            }
            Message::Updates { .. } => updates += 1,
            _ => {}
        }
    }
    (hellos, updates)
}

/// A read-only session (read-only hub token or folder) takes everything in
/// and sends no document data, not even when asked for a snapshot; its own
/// edits stay unsynced.
#[test]
fn a_read_only_session_receives_but_never_sends_edits() {
    use bae_sync::{MemoryTransport, Message, Transport};
    let base = base_snapshots();
    let (mut la, mut lb) = MemoryTransport::pair();
    let (mut sa, mut sb) = (client_stage(&base), client_stage(&base));
    let mut a = SyncSession::new(SessionConfig::new(ClientId::new("writer").unwrap()));
    let mut config = SessionConfig::new(ClientId::new("reader").unwrap());
    config.read_only = true;
    let mut b = SyncSession::new(config);
    let (wa, wb) = (author("user:writer"), author("user:reader"));
    let (mut ra, mut rb) = (Rng::new(1), Rng::new(2));
    let (mut na, mut nb) = (0, 0);
    for _ in 0..10 {
        random_edit(&mut sa, &wa, &mut ra, "w", &mut na);
        random_edit(&mut sb, &wb, &mut rb, "r", &mut nb);
    }
    for _ in 0..4 {
        a.pump(&mut sa, &mut la).unwrap();
        b.pump(&mut sb, &mut lb).unwrap();
    }
    let root = DocKey::sublayer(&SublayerId::root());
    for key in sa.doc_keys() {
        assert!(
            sb.version(&key)
                .unwrap()
                .includes_vv(&sa.version(&key).unwrap())
        );
    }
    assert!(
        !b.unsynced(&sb).is_empty(),
        "the reader's edits are not sent"
    );
    assert_eq!(b.stats().bytes_sent, 0);
    assert!(a.unsynced(&sa).is_empty());
    // Not even on request.
    la.send(Message::SnapshotRequest { doc: root }).unwrap();
    b.pump(&mut sb, &mut lb).unwrap();
    b.flush(&sb, &mut lb).unwrap();
    while let Some(msg) = la.recv().unwrap() {
        assert!(
            !matches!(msg, Message::Updates { .. } | Message::Snapshot { .. }),
            "{msg:?}"
        );
    }
    assert_eq!(b.stats().bytes_sent, 0);
}

/// `Relay::connect_with`: one peer (a store, as the hub's project directory) gets
/// its own send interval and no periodic hello; `flush` forces out what the
/// interval held back. The relay's client ID wins over the one in the config.
#[test]
fn a_peer_with_its_own_session_settings() {
    use bae_sync::{MemoryTransport, Message, Transport};

    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let defaults = relay.peer_config();
    assert_eq!(defaults.client.as_str(), "hub");
    assert_eq!(defaults.hello_interval, Some(Duration::from_secs(5)));
    assert_eq!(defaults.min_send_interval, Duration::ZERO);

    let mut config = relay.peer_config();
    config.client = ClientId::new("not-the-hub").unwrap();
    config.min_send_interval = Duration::from_secs(1);
    config.hello_interval = None;
    let (hub_end, mut store) = MemoryTransport::pair();
    let store_handle = relay.connect_with(Box::new(hub_end), config);
    let (hub_end, mut link) = MemoryTransport::pair();
    relay.connect(Box::new(hub_end));
    let mut editor = client_stage(&base);
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));

    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    // Before the store said hello the relay knows nothing to send it, flush
    // included.
    relay.pump_at(at(0));
    assert_eq!(relay.flush_at(at(0)).sent, 0);
    assert_eq!(drain(&mut store, "hub"), (1, 0));
    store
        .send(Message::hello(ClientId::new("disk").unwrap(), vec![]))
        .unwrap();
    // The editor uploads the base project; the store's first send is never held
    // back (and the relay announces the two new documents with a hello).
    for _ in 0..3 {
        session.pump_at(&mut editor, &mut link, at(0)).unwrap();
        relay.pump_at(at(0));
    }
    assert!(session.is_connected());
    assert_eq!(drain(&mut store, "hub"), (1, 2));
    assert!(
        relay
            .peers()
            .iter()
            .any(|(h, c)| *h == store_handle && c.as_ref().unwrap().as_str() == "disk")
    );

    let root = DocKey::sublayer(&SublayerId::root());
    let drag = |editor: &mut Stage, value: f64| {
        editor.set_edit_target(&SublayerId::root()).unwrap();
        editor
            .transact(&author("user:a"), "drag", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(bae_model::Value::Float(value)),
                })
            })
            .unwrap();
    };
    // A drag inside the store's send interval: the relay has every step, the
    // store gets nothing yet.
    for (i, ms) in [10, 20, 30].into_iter().enumerate() {
        drag(&mut editor, i as f64);
        session.pump_at(&mut editor, &mut link, at(ms)).unwrap();
        relay.pump_at(at(ms));
    }
    assert!(
        relay
            .docs()
            .version(&root)
            .unwrap()
            .includes_vv(&editor.version(&root).unwrap())
    );
    assert_eq!(drain(&mut store, "hub"), (0, 0));

    // `flush` sends it at once, as one update.
    let out = relay.flush_at(at(40));
    assert_eq!((out.sent, out.disconnected.len()), (1, 0));
    assert_eq!(drain(&mut store, "hub"), (0, 1));
    assert_eq!(relay.flush_at(at(41)).sent, 0, "nothing left to flush");

    // The flush restarted the interval: the next step waits for it, then goes
    // out on an ordinary pump.
    drag(&mut editor, 9.0);
    session.pump_at(&mut editor, &mut link, at(50)).unwrap();
    relay.pump_at(at(50));
    assert_eq!(drain(&mut store, "hub"), (0, 0));
    relay.pump_at(at(1039));
    assert_eq!(drain(&mut store, "hub"), (0, 0));
    relay.pump_at(at(1040));
    assert_eq!(drain(&mut store, "hub"), (0, 1));

    // No periodic hello for the store; the client connected with `connect`
    // keeps the relay's 5 s.
    drain(&mut link, "hub");
    relay.pump_at(at(6000));
    assert_eq!(drain(&mut store, "hub"), (0, 0));
    assert_eq!(drain(&mut link, "hub").0, 1);
}

/// Writes that block until `gate` closes, then fail: a store whose disk fails
/// while the relay is shutting down.
struct FailingDisk {
    gate: std::sync::mpsc::Receiver<()>,
}

impl bae_sync::Transport for FailingDisk {
    fn send(&mut self, _: bae_sync::Message) -> Result<(), bae_sync::SyncError> {
        let _ = self.gate.recv();
        Err(bae_sync::SyncError::Hub("disk full".into()))
    }

    fn recv(&mut self) -> Result<Option<bae_sync::Message>, bae_sync::SyncError> {
        Ok(None)
    }
}

/// A `ThreadedTransport`'s `send` only queues, so a write that fails after the
/// last pump is never seen by the session. `Transport::close` waits for
/// everything queued and reports it; `Relay::close_peer` closes one peer so.
#[test]
fn closing_a_threaded_peer_reports_its_last_writes() {
    use bae_sync::{MemoryTransport, Message, SyncError, ThreadedTransport, Transport};

    let hour = Duration::from_secs(3600);
    // Everything queued is delivered before `close` returns.
    let (inner, mut far) = MemoryTransport::pair();
    let mut t = ThreadedTransport::spawn(inner, hour, || {});
    for _ in 0..3 {
        t.send(Message::SnapshotRequest {
            doc: DocKey::History,
        })
        .unwrap();
    }
    t.close().unwrap();
    let mut delivered = 0;
    // `Closed` once the three are read: the worker dropped its end.
    while let Ok(Some(_)) = far.recv() {
        delivered += 1;
    }
    assert_eq!(delivered, 3);
    assert!(matches!(far.recv(), Err(SyncError::Closed)));
    assert!(matches!(
        t.send(Message::SnapshotRequest {
            doc: DocKey::History
        }),
        Err(SyncError::Closed)
    ));

    // A failed write after the relay's last pump.
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let (gate, rx) = std::sync::mpsc::channel();
    let store = relay.connect(Box::new(ThreadedTransport::spawn(
        FailingDisk { gate: rx },
        hour,
        || {},
    )));
    // The hello is queued; its write has not failed yet.
    let out = relay.pump();
    assert!(out.disconnected.is_empty());
    drop(gate);
    let closed = relay.close_peer(store);
    assert!(
        matches!(closed, Some(Err(SyncError::Hub(ref e))) if e == "disk full"),
        "{closed:?}"
    );
    assert!(relay.peers().is_empty());
    assert!(relay.close_peer(store).is_none());
}

/// One direction of a [`Slow`] link: messages in order, each arriving once
/// its last byte went through.
#[derive(Default)]
struct Pipe {
    queue: std::collections::VecDeque<(Instant, bae_sync::Message)>,
    free_at: Option<Instant>,
    /// `Updates` and `Snapshot`s sent, per document: `(messages, bytes)`.
    data: std::collections::BTreeMap<DocKey, (usize, usize)>,
}

impl Pipe {
    fn messages(&self, doc: &DocKey) -> usize {
        self.data.get(doc).map_or(0, |d| d.0)
    }

    fn bytes(&self, doc: &DocKey) -> usize {
        self.data.get(doc).map_or(0, |d| d.1)
    }
}

/// An in-order link that carries `rate` bytes per (simulated) second, like a
/// slow Wi-Fi or VPN: a large message is in transit for a while.
struct Slow {
    clock: std::sync::Arc<std::sync::Mutex<Instant>>,
    tx: std::sync::Arc<std::sync::Mutex<Pipe>>,
    rx: std::sync::Arc<std::sync::Mutex<Pipe>>,
    rate: f64,
}

type SharedPipe = std::sync::Arc<std::sync::Mutex<Pipe>>;

/// Both ends of a [`Slow`] link (client, hub) and its pipes (up, down).
fn slow_link(
    clock: &std::sync::Arc<std::sync::Mutex<Instant>>,
    rate: f64,
) -> (Slow, Slow, SharedPipe, SharedPipe) {
    let (up, down): (SharedPipe, SharedPipe) = Default::default();
    let client = Slow {
        clock: clock.clone(),
        tx: up.clone(),
        rx: down.clone(),
        rate,
    };
    let hub = Slow {
        clock: clock.clone(),
        tx: down.clone(),
        rx: up.clone(),
        rate,
    };
    (client, hub, up, down)
}

impl bae_sync::Transport for Slow {
    fn send(&mut self, msg: bae_sync::Message) -> Result<(), bae_sync::SyncError> {
        let now = *self.clock.lock().unwrap();
        let mut pipe = self.tx.lock().unwrap();
        let start = pipe.free_at.map_or(now, |t| t.max(now));
        let arrive = start + Duration::from_secs_f64(msg.encode().len() as f64 / self.rate);
        pipe.free_at = Some(arrive);
        if let bae_sync::Message::Updates { doc, data, .. }
        | bae_sync::Message::Snapshot { doc, data, .. } = &msg
        {
            let d = pipe.data.entry(doc.clone()).or_default();
            d.0 += 1;
            d.1 += data.len();
        }
        pipe.queue.push_back((arrive, msg));
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<bae_sync::Message>, bae_sync::SyncError> {
        let now = *self.clock.lock().unwrap();
        let mut pipe = self.rx.lock().unwrap();
        Ok(match pipe.queue.front() {
            Some((at, _)) if *at <= now => pipe.queue.pop_front().map(|(_, m)| m),
            _ => None,
        })
    }
}

/// [`bae_sync::MemDocs`] with one document it cannot take: missing (a
/// sublayer a collaborator created that the app did not add yet: what arrives
/// for it is dropped) or failing every import.
struct Without {
    docs: bae_sync::MemDocs,
    missing: Option<DocKey>,
    failing: Option<DocKey>,
}

impl DocSet for Without {
    fn doc_keys(&self) -> Vec<DocKey> {
        let mut keys = self.docs.doc_keys();
        keys.retain(|k| Some(k) != self.missing.as_ref());
        keys
    }
    fn version(&self, doc: &DocKey) -> Option<bae_model::loro::VersionVector> {
        if Some(doc) == self.missing.as_ref() {
            return None;
        }
        self.docs.version(doc)
    }
    fn export_updates(
        &self,
        doc: &DocKey,
        since: &bae_model::loro::VersionVector,
    ) -> Result<Vec<u8>, bae_sync::SyncError> {
        self.docs.export_updates(doc, since)
    }
    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, bae_sync::SyncError> {
        self.docs.export_snapshot(doc)
    }
    fn import(
        &mut self,
        doc: &DocKey,
        data: &[u8],
    ) -> Result<bae_sync::Imported, bae_sync::SyncError> {
        if Some(doc) == self.missing.as_ref() {
            return Ok(bae_sync::Imported::UnknownDoc);
        }
        if Some(doc) == self.failing.as_ref() {
            return Err(bae_sync::SyncError::Crdt("cannot import".into()));
        }
        self.docs.import(doc, data)
    }
}

/// 1 MB of text that does not compress.
fn big_text() -> String {
    let mut rng = Rng::new(5);
    (0..1_000_000)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect()
}

/// What the client does not take in [`a_large_transfer_over_a_slow_link_is_sent_once`].
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fx {
    /// The app never adds `fx`.
    Missing,
    /// The app adds it 3 s in, while the large transfer is on its way
    /// (spec 2.5).
    AddedAt3s,
    /// Every import of it fails.
    Failing,
}

/// A large transfer over a slow link (a new client joining a big project
/// over a VPN, or uploading one) goes out once, although both sides say
/// `Hello` every 5 s while it is in transit: such a `Hello` was written
/// before the transfer arrived, and must not make the sender queue it again.
/// Also while the client cannot take another document: one it does not have
/// (and adds meanwhile), or one whose imports fail.
#[test]
fn a_large_transfer_over_a_slow_link_is_sent_once() {
    use bae_model::loro::{ExportMode, LoroDoc};
    use bae_sync::MemDocs;
    use std::sync::{Arc, Mutex};

    let doc = DocKey::sublayer(&SublayerId::root());
    let fx = DocKey::sublayer(&sl("fx"));
    let small = LoroDoc::new();
    small.get_map("m").insert("k", 1).unwrap();
    small.commit();
    let small = small.export(ExportMode::Snapshot).unwrap();
    let empty = LoroDoc::new().export(ExportMode::Snapshot).unwrap();
    let big = LoroDoc::new();
    big.get_text("t").insert(0, &big_text()).unwrap();
    big.commit();
    let snapshot = big.export(ExportMode::Snapshot).unwrap();
    let full = || {
        let mut docs = MemDocs::new();
        docs.load(doc.clone(), &snapshot).unwrap();
        docs
    };

    for (upload, case) in [
        (true, Fx::Missing),
        (false, Fx::Missing),
        (false, Fx::AddedAt3s),
        (false, Fx::Failing),
    ] {
        let t0 = Instant::now();
        let clock = Arc::new(Mutex::new(t0));
        // 50 kB/s: the transfer takes about 20 s.
        let (mut link, hub_end, up, down) = slow_link(&clock, 50_000.0);
        let mut relay = if upload {
            Relay::new(ClientId::new("hub").unwrap())
        } else {
            let mut docs = full();
            docs.load(fx.clone(), &small).unwrap();
            Relay::with_docs(ClientId::new("hub").unwrap(), docs)
        };
        assert_eq!(
            relay.peer_config().hello_interval,
            Some(Duration::from_secs(5))
        );
        relay.connect(Box::new(hub_end));
        let mut docs = Without {
            docs: if upload { full() } else { MemDocs::new() },
            missing: None,
            failing: None,
        };
        match case {
            Fx::Missing | Fx::AddedAt3s => docs.missing = Some(fx.clone()),
            Fx::Failing => {
                docs.docs.load(fx.clone(), &empty).unwrap();
                docs.failing = Some(fx.clone());
            }
        }
        let config = SessionConfig::new(ClientId::new("a").unwrap());
        assert_eq!(config.hello_interval, Some(Duration::from_secs(5)));
        let mut session = SyncSession::new(config);

        let want = big.oplog_vv();
        let mut arrived = None;
        // 50 ms ticks for 60 s of simulated time.
        for tick in 0..1200u32 {
            let now = t0 + Duration::from_millis(50) * tick;
            *clock.lock().unwrap() = now;
            if case == Fx::AddedAt3s && tick == 60 {
                docs.missing = None;
                docs.docs.load(fx.clone(), &empty).unwrap();
            }
            session.pump_at(&mut docs, &mut link, now).unwrap();
            let out = relay.pump_at(now);
            assert!(out.disconnected.is_empty() && out.errors.is_empty());
            let have = if upload {
                relay.docs().version(&doc)
            } else {
                docs.version(&doc)
            };
            if arrived.is_none() && have.is_some_and(|v| v.includes_vv(&want)) {
                arrived = Some(now - t0);
            }
        }
        let arrived = arrived.expect("the transfer arrived");
        assert!(arrived > Duration::from_secs(15), "{arrived:?}");
        let pipe = if upload { &up } else { &down };
        assert_eq!(
            pipe.lock().unwrap().messages(&doc),
            1,
            "upload={upload} {case:?}: arrived after {arrived:?}"
        );
        assert!(session.unsynced(&docs).is_empty());
        if upload {
            continue;
        }
        let fx_sent = down.lock().unwrap().messages(&fx);
        match case {
            // The client does not take `fx`: the hub keeps offering it.
            Fx::Missing | Fx::Failing => assert!(fx_sent > 1, "{case:?}"),
            Fx::AddedAt3s => {
                assert_eq!(docs.version(&fx), relay.docs().version(&fx));
                assert_eq!(fx_sent, 2, "dropped once, then sent once more");
            }
        }
    }
}

/// The client drags (an edit every 50 ms) over the slow link and pastes 1 MB
/// in the middle of it, around one of its `Hello`s: the paste goes out once.
/// The hub's `Hello`s acknowledge the client's `Hello` before the paste while
/// the paste is on its way; what the drag sent before that `Hello` did arrive,
/// so the paste after it is still on its way, not lost.
#[test]
fn a_large_paste_during_a_drag_over_a_slow_link_is_sent_once() {
    use bae_model::loro::{ExportMode, LoroDoc};
    use bae_sync::MemDocs;
    use std::sync::{Arc, Mutex};

    let doc = DocKey::sublayer(&SublayerId::root());
    let start = LoroDoc::new();
    start.get_map("m").insert("x", 0).unwrap();
    start.commit();
    let start = start.export(ExportMode::Snapshot).unwrap();
    let text = big_text();
    // The client says `Hello` at tick 100 (5 s), the hub just after.
    for paste_at in 98..=102 {
        let t0 = Instant::now();
        let clock = Arc::new(Mutex::new(t0));
        let (mut link, hub_end, up, _down) = slow_link(&clock, 50_000.0);
        let mut docs = MemDocs::new();
        docs.load(doc.clone(), &start).unwrap();
        let mut relay = Relay::new(ClientId::new("hub").unwrap());
        relay.connect(Box::new(hub_end));
        let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
        let mut paste = 0;
        for tick in 0..1600u32 {
            let now = t0 + Duration::from_millis(50) * tick;
            *clock.lock().unwrap() = now;
            let d = docs.get(&doc).unwrap();
            // The drag goes on for 60 s.
            if tick < 1200 {
                d.get_map("m").insert("x", i64::from(tick)).unwrap();
            }
            d.commit();
            if tick == paste_at {
                let before = d.oplog_vv();
                d.get_text("t").insert(0, &text).unwrap();
                d.commit();
                paste = d.export(ExportMode::updates(&before)).unwrap().len();
            }
            session.pump_at(&mut docs, &mut link, now).unwrap();
            let out = relay.pump_at(now);
            assert!(out.disconnected.is_empty() && out.errors.is_empty());
        }
        assert_eq!(relay.docs().version(&doc), docs.version(&doc));
        assert!(session.unsynced(&docs).is_empty());
        let sent = up.lock().unwrap().bytes(&doc);
        assert!(paste > 900_000, "{paste}");
        assert!(
            (paste..paste * 3 / 2).contains(&sent),
            "pasted at tick {paste_at}: {sent} bytes sent for a paste of {paste}"
        );
    }
}

/// The client creates a sublayer and fills it with 1 MB right after: the hub
/// creates the document from the first message and lists it in a `Hello`
/// while the 1 MB is on its way. A document listed for the first time is sent
/// again at once only if the remote could have dropped what we sent (it does
/// not have our first message); the hub did not, so the 1 MB goes once.
#[test]
fn a_new_document_filled_right_after_it_was_created_is_sent_once() {
    use bae_model::loro::{ExportMode, LoroDoc};
    use bae_sync::MemDocs;
    use std::sync::{Arc, Mutex};

    let root = DocKey::sublayer(&SublayerId::root());
    let fx = DocKey::sublayer(&sl("fx"));
    let small = LoroDoc::new();
    small.get_map("m").insert("k", 1).unwrap();
    small.commit();
    let small = small.export(ExportMode::Snapshot).unwrap();
    let text = big_text();
    let t0 = Instant::now();
    let clock = Arc::new(Mutex::new(t0));
    let (mut link, hub_end, up, _down) = slow_link(&clock, 50_000.0);
    let mut docs = MemDocs::new();
    docs.load(root.clone(), &small).unwrap();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    relay.connect(Box::new(hub_end));
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
    let mut fill = 0;
    for tick in 0..1200u32 {
        let now = t0 + Duration::from_millis(50) * tick;
        *clock.lock().unwrap() = now;
        if tick == 20 {
            docs.load(fx.clone(), &small).unwrap();
        }
        if tick == 21 {
            let d = docs.get(&fx).unwrap();
            let before = d.oplog_vv();
            d.get_text("t").insert(0, &text).unwrap();
            d.commit();
            fill = d.export(ExportMode::updates(&before)).unwrap().len();
        }
        session.pump_at(&mut docs, &mut link, now).unwrap();
        let out = relay.pump_at(now);
        assert!(out.disconnected.is_empty() && out.errors.is_empty());
    }
    assert_eq!(relay.docs().version(&fx), docs.version(&fx));
    assert!(session.unsynced(&docs).is_empty());
    let sent = up.lock().unwrap().bytes(&fx);
    assert!(fill > 900_000, "{fill}");
    assert!(
        (fill..fill * 3 / 2).contains(&sent),
        "{sent} bytes sent for {fill}"
    );
}

/// Loses the first `Updates` it is given to send.
struct LoseFirstUpdates {
    inner: bae_sync::MemoryTransport,
    lost: bool,
}

impl bae_sync::Transport for LoseFirstUpdates {
    fn send(&mut self, msg: bae_sync::Message) -> Result<(), bae_sync::SyncError> {
        if !self.lost && matches!(msg, bae_sync::Message::Updates { .. }) {
            self.lost = true;
            return Ok(());
        }
        self.inner.send(msg)
    }

    fn recv(&mut self) -> Result<Option<bae_sync::Message>, bae_sync::SyncError> {
        self.inner.recv()
    }
}

/// A message lost on the way is sent again within a few hello intervals,
/// also while the sender keeps sending that document (a drag: one edit per
/// 100 ms tick, for 60 s). Its later sends cannot be applied without the
/// lost one (Loro keeps them pending), so they must not count as covering
/// it. Directly, through a hub (the hub's message to the other client is
/// lost), and from a session without periodic `Hello`s, whose remote's
/// `Hello`s cannot acknowledge a later one of ours.
#[test]
fn a_lost_update_is_sent_again_while_the_document_keeps_changing() {
    use bae_model::loro::{ExportMode, LoroDoc};
    use bae_sync::{MemDocs, MemoryTransport};

    let d0 = DocKey::parse("d0").unwrap();
    let base = LoroDoc::new();
    base.get_map("m").insert("base", 1).unwrap();
    base.commit();
    let base = base.export(ExportMode::Snapshot).unwrap();
    let docs = || {
        let mut docs = MemDocs::new();
        docs.load(d0.clone(), &base).unwrap();
        docs
    };
    for (through_hub, sender_hello) in [
        (false, Some(Duration::from_secs(5))),
        (true, Some(Duration::from_secs(5))),
        (false, None),
    ] {
        let mut a = docs();
        let mut b = docs();
        let mut ca = SessionConfig::new(ClientId::new("a").unwrap());
        ca.hello_interval = sender_hello;
        let mut sa = SyncSession::new(ca);
        let mut sb = SyncSession::new(SessionConfig::new(ClientId::new("b").unwrap()));
        let (mut la, mut lb): (Box<dyn bae_sync::Transport>, Box<dyn bae_sync::Transport>);
        let mut relay = None;
        if through_hub {
            let mut r = Relay::with_docs(ClientId::new("hub").unwrap(), docs());
            let (hub_a, a_end) = MemoryTransport::pair();
            let (hub_b, b_end) = MemoryTransport::pair();
            r.connect(Box::new(hub_a));
            r.connect(Box::new(LoseFirstUpdates {
                inner: hub_b,
                lost: false,
            }));
            (la, lb) = (Box::new(a_end), Box::new(b_end));
            relay = Some(r);
        } else {
            let (a_end, b_end) = MemoryTransport::pair();
            la = Box::new(LoseFirstUpdates {
                inner: a_end,
                lost: false,
            });
            lb = Box::new(b_end);
        }
        let t0 = Instant::now();
        let mut first = None;
        let mut arrived = None;
        for tick in 0..600u64 {
            let now = t0 + Duration::from_millis(100) * tick as u32;
            if tick >= 3 {
                let d = a.get(&d0).unwrap();
                d.get_map("m")
                    .insert(&format!("k{tick}"), tick as i64)
                    .unwrap();
                d.commit();
                first.get_or_insert_with(|| a.version(&d0).unwrap());
            }
            sa.pump_at(&mut a, &mut *la, now).unwrap();
            sb.pump_at(&mut b, &mut *lb, now).unwrap();
            if let Some(r) = &mut relay {
                let out = r.pump_at(now);
                assert!(out.disconnected.is_empty() && out.errors.is_empty());
            }
            if let Some(first) = &first
                && arrived.is_none()
                && b.version(&d0).unwrap().includes_vv(first)
            {
                arrived = Some(tick);
            }
        }
        let case = format!("through_hub={through_hub} sender_hello={sender_hello:?}");
        let arrived = arrived.unwrap_or_else(|| panic!("{case}: the first edit never arrived"));
        // Three hello intervals of 50 ticks.
        assert!(
            arrived < 3 + 150,
            "{case}: the first edit arrived at tick {arrived}"
        );
    }
}

/// Documents of the random schedules (`d0` .. `d3`).
const STRESS_DOCS: usize = 4;

fn stress_doc(i: usize) -> DocKey {
    DocKey::parse(&format!("d{i}")).unwrap()
}

/// A node's documents in [`random_schedules_converge_while_edits_go_on`]:
/// some may not be there yet (a sublayer the app did not add: what arrives
/// for it is dropped), and imports may fail.
struct StressDocs {
    docs: bae_sync::MemDocs,
    /// Loro peer of this node's edits.
    peer: u64,
    hidden: std::collections::BTreeSet<DocKey>,
    fail_percent: u32,
    rng: Rng,
}

impl StressDocs {
    /// The app adds `doc` (empty: its contents come from the remote).
    fn add(&mut self, doc: &DocKey) {
        use bae_model::loro::{ExportMode, LoroDoc};
        if self.hidden.remove(doc) {
            let empty = LoroDoc::new().export(ExportMode::Snapshot).unwrap();
            self.docs.load(doc.clone(), &empty).unwrap();
            self.docs.get(doc).unwrap().set_peer_id(self.peer).unwrap();
        }
    }
}

impl DocSet for StressDocs {
    fn doc_keys(&self) -> Vec<DocKey> {
        self.docs.doc_keys()
    }
    fn version(&self, doc: &DocKey) -> Option<bae_model::loro::VersionVector> {
        self.docs.version(doc)
    }
    fn export_updates(
        &self,
        doc: &DocKey,
        since: &bae_model::loro::VersionVector,
    ) -> Result<Vec<u8>, bae_sync::SyncError> {
        self.docs.export_updates(doc, since)
    }
    fn export_snapshot(&self, doc: &DocKey) -> Result<Vec<u8>, bae_sync::SyncError> {
        self.docs.export_snapshot(doc)
    }
    fn import(
        &mut self,
        doc: &DocKey,
        data: &[u8],
    ) -> Result<bae_sync::Imported, bae_sync::SyncError> {
        if self.hidden.contains(doc) {
            return Ok(bae_sync::Imported::UnknownDoc);
        }
        if self.rng.chance(self.fail_percent) {
            return Err(bae_sync::SyncError::Crdt("import failed".into()));
        }
        self.docs.import(doc, data)
    }
}

/// One random schedule: who talks to whom, how often each says `Hello`, how
/// bad the network is, and what the nodes do meanwhile.
#[derive(Debug)]
struct Schedule {
    seed: u64,
    /// A hub between the nodes, with its hello interval in ticks; `None`:
    /// two nodes linked directly.
    hub: Option<u64>,
    /// Per node, its hello interval in ticks (`None`: no periodic `Hello`).
    hello: Vec<Option<u64>>,
    max_messages: Vec<usize>,
    /// Per node, `min_send_interval` in ticks.
    min_send: Vec<u64>,
    /// Per node, the percentage of ticks with an edit.
    edit: Vec<u32>,
    /// Per node, the percentage of imports that fail while the network is bad.
    fail: Vec<u32>,
    /// Per node, documents it does not have at first and the tick the app
    /// adds them (while the network is bad).
    hidden: Vec<Vec<(usize, u64)>>,
    /// Percentage of ticks a node's link is replaced while the network is bad.
    reconnect: u32,
    bad: Conditions,
    bad_ticks: u64,
}

impl Schedule {
    fn new(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let hub = rng.chance(75).then(|| 3 + rng.below(28) as u64);
        let nodes = if hub.is_some() { 2 + rng.below(3) } else { 2 };
        let bad_ticks = 100 + rng.below(201) as u64;
        let mut hello: Vec<Option<u64>> = (0..nodes)
            .map(|_| (!rng.chance(15)).then(|| 3 + rng.below(28) as u64))
            .collect();
        if hub.is_none() && hello.iter().all(Option::is_none) {
            hello[1] = Some(10);
        }
        let max_messages = (0..nodes).map(|_| [24, 256][rng.below(2)]).collect();
        let min_send = (0..nodes)
            .map(|_| {
                if rng.chance(50) {
                    0
                } else {
                    1 + rng.below(5) as u64
                }
            })
            .collect();
        let edit = (0..nodes).map(|_| 40 + rng.below(61) as u32).collect();
        // A node without periodic `Hello`s never shows the remote what it
        // dropped (spec 2.4: both sides must say `Hello` periodically), so
        // its imports do not fail.
        let fail = hello
            .iter()
            .map(|h| if h.is_some() && rng.chance(25) { 10 } else { 0 })
            .collect();
        let mut hidden = vec![Vec::new(); nodes];
        for docs in &mut hidden {
            for d in 1..STRESS_DOCS {
                if rng.chance(10) {
                    docs.push((d, rng.below(bad_ticks as usize) as u64));
                }
            }
        }
        let reconnect = [0, 0, 1, 3][rng.below(4)];
        let bad = Conditions {
            drop_percent: 5 + rng.below(46) as u32,
            duplicate_percent: rng.below(51) as u32,
            max_delay: rng.below(7) as u64,
        };
        Schedule {
            seed,
            hub,
            hello,
            max_messages,
            min_send,
            edit,
            fail,
            hidden,
            reconnect,
            bad,
            bad_ticks,
        }
    }

    /// Ticks within which everything must get everywhere once the network
    /// is good: a few hello intervals.
    fn bound(&self) -> u64 {
        let hello = self
            .hello
            .iter()
            .chain([&self.hub])
            .flatten()
            .max()
            .unwrap();
        4 * hello + 2 * self.min_send.iter().max().unwrap() + 40
    }
}

struct StressNode {
    docs: StressDocs,
    session: SyncSession,
    link: SimEnd,
    hub_handle: Option<bae_sync::PeerHandle>,
    rng: Rng,
    edits: u64,
}

/// Runs one schedule: a bad network with edits, reconnects, documents the
/// app adds late and failing imports; then a good network while the nodes
/// keep editing, in which everything from the bad part must reach everyone
/// within [`Schedule::bound`] ticks; then no more edits, and everyone must
/// agree within the same bound. Returns what went wrong.
fn run_schedule(s: &Schedule) -> Result<(), String> {
    use bae_model::loro::{ExportMode, LoroDoc};
    use bae_sync::MemDocs;

    let net = SimNet::new(s.seed, s.bad);
    let base: Vec<Vec<u8>> = (0..STRESS_DOCS)
        .map(|i| {
            let d = LoroDoc::new();
            d.set_peer_id(900 + i as u64).unwrap();
            d.get_map("m").insert("base", 1).unwrap();
            d.commit();
            d.export(ExportMode::Snapshot).unwrap()
        })
        .collect();
    let load = |hidden: &[(usize, u64)], peer: Option<u64>| {
        let mut docs = MemDocs::new();
        for (i, snapshot) in base.iter().enumerate() {
            if hidden.iter().any(|(d, _)| *d == i) {
                continue;
            }
            docs.load(stress_doc(i), snapshot).unwrap();
            if let Some(peer) = peer {
                docs.get(&stress_doc(i)).unwrap().set_peer_id(peer).unwrap();
            }
        }
        docs
    };
    let mut relay = s.hub.map(|every| {
        let mut r = Relay::with_docs(ClientId::new("hub").unwrap(), load(&[], None));
        r.set_hello_interval(Some(Duration::from_millis(100 * every)));
        r
    });
    let nodes = s.hello.len();
    // The ends of a new link for node `i`: its own, and the other side's
    // (the hub's, or node 1's). A node without periodic `Hello`s cannot
    // repair what it does not receive, and the remote could not repair
    // what it sends if its `Hello`s got lost.
    let new_link = |i: usize| {
        let (mut mine, mut theirs) = net.link();
        if s.hello[i].is_none() {
            mine = mine.keep_hellos();
            theirs = theirs.lossless();
        }
        if s.hub.is_none() && s.hello[1 - i].is_none() {
            mine = mine.lossless();
            theirs = theirs.keep_hellos();
        }
        (mine, theirs)
    };
    let mut ns: Vec<StressNode> = Vec::new();
    let mut p2p_end = None;
    for i in 0..nodes {
        let mut config = SessionConfig::new(ClientId::new(&format!("n{i}")).unwrap());
        config.hello_interval = s.hello[i].map(|t| Duration::from_millis(100 * t));
        config.max_messages_per_pump = s.max_messages[i];
        config.min_send_interval = Duration::from_millis(100 * s.min_send[i]);
        let link = if let Some(relay) = &mut relay {
            let (mine, theirs) = new_link(i);
            let handle = relay.connect(Box::new(theirs));
            (mine, Some(handle))
        } else if i == 0 {
            let (mine, theirs) = new_link(0);
            p2p_end = Some(theirs);
            (mine, None)
        } else {
            (p2p_end.take().unwrap(), None)
        };
        ns.push(StressNode {
            docs: StressDocs {
                docs: load(&s.hidden[i], Some(i as u64 + 1)),
                peer: i as u64 + 1,
                hidden: s.hidden[i].iter().map(|(d, _)| stress_doc(*d)).collect(),
                fail_percent: s.fail[i],
                rng: Rng::new(s.seed * 7919 + i as u64),
            },
            session: SyncSession::new(config),
            link: link.0,
            hub_handle: link.1,
            rng: Rng::new(s.seed * 104_729 + i as u64),
            edits: 0,
        });
    }

    let t0 = Instant::now();
    let mut log = Vec::new();
    let step = |ns: &mut Vec<StressNode>,
                relay: &mut Option<Relay>,
                log: &mut Vec<String>,
                edits: bool,
                bad: bool| {
        let tick = net.now();
        let now = net.instant(t0);
        for i in 0..ns.len() {
            let n = &mut ns[i];
            for (d, at) in &s.hidden[i] {
                if *at == tick {
                    n.docs.add(&stress_doc(*d));
                    log.push(format!("t{tick}: node {i} adds d{d}"));
                }
            }
            if edits && n.rng.chance(s.edit[i]) {
                let keys = n.docs.doc_keys();
                let key = &keys[n.rng.below(keys.len())];
                let doc = n.docs.docs.get(key).unwrap();
                doc.get_map("m")
                    .insert(&format!("n{i}_{}", n.edits), n.edits as i64)
                    .unwrap();
                doc.commit();
                n.edits += 1;
            }
            if bad && n.rng.chance(s.reconnect) {
                log.push(format!("t{tick}: node {i} reconnects"));
                if let Some(relay) = relay.as_mut() {
                    relay.disconnect(n.hub_handle.unwrap());
                    let (mine, theirs) = new_link(i);
                    n.hub_handle = Some(relay.connect(Box::new(theirs)));
                    n.link = mine;
                    n.session.reset();
                } else {
                    let (mine, theirs) = new_link(i);
                    ns[i].link = mine;
                    ns[1 - i].link = theirs;
                    ns[0].session.reset();
                    ns[1].session.reset();
                }
            }
        }
        for n in ns.iter_mut() {
            n.session.pump_at(&mut n.docs, &mut n.link, now).unwrap();
        }
        if let Some(relay) = relay.as_mut() {
            let out = relay.pump_at(now);
            assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        }
        net.tick();
    };
    // Every document's version in every node (and the hub).
    let versions = |ns: &[StressNode], relay: &Option<Relay>| {
        let mut all: Vec<(String, Vec<Option<bae_model::loro::VersionVector>>)> = ns
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let vs = (0..STRESS_DOCS)
                    .map(|d| n.docs.version(&stress_doc(d)))
                    .collect();
                (format!("node {i}"), vs)
            })
            .collect();
        if let Some(r) = relay {
            let vs = (0..STRESS_DOCS)
                .map(|d| r.docs().version(&stress_doc(d)))
                .collect();
            all.push(("hub".into(), vs));
        }
        all
    };
    let report = |what: &str,
                  ns: &[StressNode],
                  relay: &Option<Relay>,
                  want: &[bae_model::loro::VersionVector],
                  log: &[String]| {
        let mut out = format!("{what}\n{s:#?}\n");
        for (who, vs) in versions(ns, relay) {
            for (d, v) in vs.iter().enumerate() {
                if !v.as_ref().is_some_and(|v| v.includes_vv(&want[d])) {
                    out += &format!("{who} d{d}: has {v:?}, want {:?}\n", want[d]);
                }
            }
        }
        out += &format!("log: {log:?}\n");
        for (i, n) in ns.iter().enumerate() {
            out += &format!("node {i}: {:?}\n", n.session);
        }
        out
    };
    let has_all =
        |ns: &[StressNode], relay: &Option<Relay>, want: &[bae_model::loro::VersionVector]| {
            versions(ns, relay).iter().all(|(_, vs)| {
                vs.iter()
                    .zip(want)
                    .all(|(v, w)| v.as_ref().is_some_and(|v| v.includes_vv(w)))
            })
        };
    let union = |ns: &[StressNode], relay: &Option<Relay>| {
        let mut want = vec![bae_model::loro::VersionVector::new(); STRESS_DOCS];
        for (_, vs) in versions(ns, relay) {
            for (w, v) in want.iter_mut().zip(vs) {
                if let Some(v) = v {
                    w.merge(&v);
                }
            }
        }
        want
    };

    for _ in 0..s.bad_ticks {
        step(&mut ns, &mut relay, &mut log, true, true);
    }
    // The network is good now; the nodes keep editing.
    net.set_conditions(Conditions::PERFECT);
    for n in &mut ns {
        n.docs.fail_percent = 0;
    }
    let want = union(&ns, &relay);
    let mut ticks = 0;
    while !has_all(&ns, &relay, &want) {
        ticks += 1;
        if ticks > s.bound() {
            return Err(report(
                &format!(
                    "what was there when the network got good still missing after {ticks} ticks of edits"
                ),
                &ns,
                &relay,
                &want,
                &log,
            ));
        }
        step(&mut ns, &mut relay, &mut log, true, false);
    }
    // No more edits: everyone agrees.
    let mut ticks = 0;
    loop {
        let want = union(&ns, &relay);
        if has_all(&ns, &relay, &want) {
            return Ok(());
        }
        ticks += 1;
        if ticks > s.bound() {
            return Err(report(
                &format!("no agreement {ticks} ticks after the last edit"),
                &ns,
                &relay,
                &want,
                &log,
            ));
        }
        step(&mut ns, &mut relay, &mut log, false, false);
    }
}

/// Random schedules (hub or direct, 2-4 nodes, loss, duplication, reorder,
/// reconnects, documents added late, failing imports, nodes without periodic
/// `Hello`s) converge, and repair what the bad network lost while the nodes
/// keep editing. `BAE_SYNC_STRESS_SEEDS=n` runs n schedules (default 24),
/// `BAE_SYNC_STRESS_SEED=s` only schedule s.
#[test]
fn random_schedules_converge_while_edits_go_on() {
    let seeds: Vec<u64> = match std::env::var("BAE_SYNC_STRESS_SEED") {
        Ok(seed) => vec![seed.parse().unwrap()],
        Err(_) => {
            let n = std::env::var("BAE_SYNC_STRESS_SEEDS").map_or(24, |n| n.parse().unwrap());
            (1..=n).collect()
        }
    };
    let mut failed = Vec::new();
    for seed in seeds {
        if let Err(e) = run_schedule(&Schedule::new(seed)) {
            if failed.is_empty() {
                eprintln!("=== seed {seed}: {e}");
            }
            failed.push(seed);
        }
    }
    assert!(failed.is_empty(), "failed schedules: {failed:?}");
}

/// Hands out `incoming` in order (`None` once empty) and records what is sent;
/// sending fails while `fail_sends` is above zero (counting down).
struct Scripted {
    incoming: std::collections::VecDeque<Result<bae_sync::Message, bae_sync::SyncError>>,
    sent: Vec<bae_sync::Message>,
    fail_sends: usize,
}

impl bae_sync::Transport for Scripted {
    fn send(&mut self, msg: bae_sync::Message) -> Result<(), bae_sync::SyncError> {
        if self.fail_sends > 0 {
            self.fail_sends -= 1;
            return Err(bae_sync::SyncError::Hub("write failed".into()));
        }
        self.sent.push(msg);
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<bae_sync::Message>, bae_sync::SyncError> {
        self.incoming.pop_front().transpose()
    }
}

/// Snapshot requests are answered after the pump's import, so they are taken
/// off the transport first. One taken before the link failed, or after one
/// whose answer failed, is in no queue any more, and the shared folder does
/// not ask twice for a compaction: it must still be answered.
#[test]
fn snapshot_requests_taken_before_a_failure_are_answered() {
    use bae_sync::{Message, SyncError};

    let base = base_snapshots();
    let root = DocKey::sublayer(&SublayerId::root());
    let anim = DocKey::sublayer(&sl("anim/a"));
    let hello = || {
        Ok(Message::hello(
            ClientId::new("shared-folder").unwrap(),
            vec![],
        ))
    };
    let request = |doc: &DocKey| Ok(Message::SnapshotRequest { doc: doc.clone() });
    let snapshots = |t: &Scripted| -> Vec<DocKey> {
        t.sent
            .iter()
            .filter_map(|m| match m {
                Message::Snapshot { doc, .. } => Some(doc.clone()),
                _ => None,
            })
            .collect()
    };

    // A request, then the link fails (a `ThreadedTransport` whose worker
    // queued the request ahead of a failed write).
    let mut stage = client_stage(&base);
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
    let mut t = Scripted {
        incoming: [hello(), request(&root), Err(SyncError::Closed)].into(),
        sent: Vec::new(),
        fail_sends: 0,
    };
    assert!(matches!(
        session.pump(&mut stage, &mut t),
        Err(SyncError::Closed)
    ));
    assert_eq!(snapshots(&t), vec![root.clone()]);

    // Two requests; the answer to the first fails (its log cannot be written).
    let mut stage = client_stage(&base);
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("a").unwrap()));
    let mut t = Scripted {
        incoming: [hello(), request(&anim), request(&root)].into(),
        sent: Vec::new(),
        fail_sends: 0,
    };
    session.pump(&mut stage, &mut t).unwrap();
    t.incoming = [request(&anim), request(&root)].into();
    t.fail_sends = 1;
    assert!(session.pump(&mut stage, &mut t).is_err());
    assert_eq!(snapshots(&t), vec![anim.clone(), root.clone(), root]);
}

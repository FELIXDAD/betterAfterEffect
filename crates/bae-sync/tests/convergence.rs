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
    use bae_sync::MemoryTransport;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let dead = Arc::new(AtomicBool::new(false));
    let (hub_end, mut leaving_end) = MemoryTransport::pair();
    relay.connect(Box::new(DyingLink {
        inner: hub_end,
        dead: dead.clone(),
    }));
    let (hub_end, mut staying_end) = MemoryTransport::pair();
    relay.connect(Box::new(hub_end));
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
    let out = relay.pump();
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

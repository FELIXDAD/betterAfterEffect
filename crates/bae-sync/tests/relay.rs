//! Relay connections with session settings of their own (`Relay::connect_with`)
//! and what a peer was not sent yet (`Relay::unsynced`).

mod common;

use std::time::{Duration, Instant};

use bae_model::loro::VersionVector;
use bae_model::{AuthorId, Op, Stage, SublayerId, Value};
use bae_sync::{ClientId, DocKey, DocSet, MemoryTransport, Relay, SessionConfig, SyncSession};
use common::*;

struct Client {
    stage: Stage,
    session: SyncSession,
    link: MemoryTransport,
}

impl Client {
    fn join(relay: &mut Relay, name: &str, base: &[(SublayerId, Vec<u8>)]) -> Self {
        let (link, hub_end) = MemoryTransport::pair();
        relay.connect(Box::new(hub_end));
        Self::new(name, base, link)
    }

    fn new(name: &str, base: &[(SublayerId, Vec<u8>)], link: MemoryTransport) -> Self {
        Client {
            stage: client_stage(base),
            session: SyncSession::new(SessionConfig::new(ClientId::new(name).unwrap())),
            link,
        }
    }

    fn set_opacity(&mut self, who: &AuthorId, value: f64) -> VersionVector {
        self.stage.set_edit_target(&SublayerId::root()).unwrap();
        self.stage
            .transact(who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: p("/comps/main/layers/title"),
                    attr: "opacity".into(),
                    value: Some(Value::Float(value)),
                })
            })
            .unwrap();
        self.stage.version(&root()).unwrap()
    }

    fn has(&self, version: &VersionVector) -> bool {
        self.stage
            .version(&root())
            .is_some_and(|v| v.includes_vv(version))
    }
}

fn root() -> DocKey {
    DocKey::sublayer(&SublayerId::root())
}

/// A few rounds of every client and the relay at the simulated time `now`.
fn settle(relay: &mut Relay, clients: &mut [&mut Client], now: Instant) {
    for _ in 0..4 {
        for c in clients.iter_mut() {
            c.session
                .pump_at(&mut c.stage, &mut c.link, now)
                .expect("pump");
        }
        relay.pump_at(now);
    }
}

#[test]
fn connect_with_gives_one_peer_its_own_send_interval() {
    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    relay.set_hello_interval(Some(Duration::from_secs(3)));
    let defaults = relay.session_config();
    assert_eq!(defaults.client.as_str(), "hub");
    assert_eq!(defaults.hello_interval, Some(Duration::from_secs(3)));
    assert_eq!(defaults.min_send_interval, Duration::ZERO);

    let mut editor = Client::join(&mut relay, "editor", &base);
    let mut fast = Client::join(&mut relay, "fast", &base);
    // Like the hub's project directory: sends to it are throttled.
    let (link, hub_end) = MemoryTransport::pair();
    let mut config = relay.session_config();
    config.min_send_interval = Duration::from_secs(10);
    let slow_handle = relay.connect_with(Box::new(hub_end), config);
    let mut slow = Client::new("slow", &base, link);

    let t0 = Instant::now();
    settle(&mut relay, &mut [&mut editor, &mut fast, &mut slow], t0);

    // Nothing was sent to the slow peer for a while: the first edit goes out at
    // once, to both.
    let who = author("user:editor");
    let t1 = t0 + Duration::from_secs(20);
    let v1 = editor.set_opacity(&who, 1.0);
    settle(&mut relay, &mut [&mut editor, &mut fast, &mut slow], t1);
    assert!(fast.has(&v1));
    assert!(slow.has(&v1));

    // The next one reaches the fast peer at once, the slow one only after its
    // interval.
    let v2 = editor.set_opacity(&who, 2.0);
    settle(
        &mut relay,
        &mut [&mut editor, &mut fast, &mut slow],
        t1 + Duration::from_millis(1),
    );
    assert!(fast.has(&v2));
    assert!(!slow.has(&v2), "the slow peer's send interval was ignored");
    settle(
        &mut relay,
        &mut [&mut editor, &mut fast, &mut slow],
        t1 + Duration::from_secs(9),
    );
    assert!(!slow.has(&v2), "the slow peer's send interval was ignored");
    settle(
        &mut relay,
        &mut [&mut editor, &mut fast, &mut slow],
        t1 + Duration::from_secs(10),
    );
    assert!(slow.has(&v2));

    // A connected peer's settings can be changed: without the throttle it gets
    // the next edit at once.
    relay
        .peer_config_mut(slow_handle)
        .expect("connected")
        .min_send_interval = Duration::ZERO;
    let v3 = editor.set_opacity(&who, 3.0);
    settle(
        &mut relay,
        &mut [&mut editor, &mut fast, &mut slow],
        t1 + Duration::from_secs(10) + Duration::from_millis(1),
    );
    assert!(slow.has(&v3));
}

/// `Relay::unsynced`: the documents a peer has not been sent yet, e.g. while
/// its send interval holds an edit back (the hub pumps on shutdown until its
/// project directory has nothing left); `None` once it is gone.
#[test]
fn unsynced_lists_what_a_peer_was_not_sent_yet() {
    let base = base_snapshots();
    let mut relay = Relay::new(ClientId::new("hub").unwrap());
    let mut editor = Client::join(&mut relay, "editor", &base);
    let (link, hub_end) = MemoryTransport::pair();
    let mut config = relay.session_config();
    config.min_send_interval = Duration::from_secs(10);
    let store = relay.connect_with(Box::new(hub_end), config);
    let mut store_client = Client::new("store", &base, link);

    let who = author("user:editor");
    let t0 = Instant::now();
    settle(&mut relay, &mut [&mut editor, &mut store_client], t0);
    assert_eq!(relay.unsynced(store), Some(Vec::new()));
    // The first edit goes out at once (nothing was sent before).
    let v1 = editor.set_opacity(&who, 1.0);
    settle(&mut relay, &mut [&mut editor, &mut store_client], t0);
    assert!(store_client.has(&v1));
    assert_eq!(relay.unsynced(store), Some(Vec::new()));

    // The next one waits for the send interval.
    let v2 = editor.set_opacity(&who, 2.0);
    let t1 = t0 + Duration::from_millis(1);
    settle(&mut relay, &mut [&mut editor, &mut store_client], t1);
    assert!(!store_client.has(&v2));
    assert_eq!(relay.unsynced(store), Some(vec![root()]));
    let t2 = t0 + Duration::from_secs(10);
    settle(&mut relay, &mut [&mut editor, &mut store_client], t2);
    assert!(store_client.has(&v2));
    assert_eq!(relay.unsynced(store), Some(Vec::new()));

    assert!(relay.disconnect(store));
    assert_eq!(relay.unsynced(store), None);
}

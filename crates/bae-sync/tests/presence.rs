//! Presence: who is online, selection, playhead, advisory locks; through the hub
//! relay and through the shared folder.

mod common;

use std::fs;
use std::time::{Duration, Instant};

use bae_model::project::Project;
use bae_model::{Stage, Time};
use bae_sync::{
    ClientId, DirConfig, DirTransport, MemoryTransport, PeerHandle, Playhead, PresenceState, Relay,
    SessionConfig, SyncSession, Transport,
};
use common::*;

fn state(name: &str) -> PresenceState {
    PresenceState {
        user: Some(author(&format!("user:{name}"))),
        name: name.to_owned(),
        color: [200, 80, 40],
        ..PresenceState::default()
    }
}

fn names(s: &SyncSession) -> Vec<String> {
    s.presence().into_iter().map(|p| p.state.name).collect()
}

struct Hub {
    relay: Relay,
    stages: Vec<Stage>,
    sessions: Vec<SyncSession>,
    links: Vec<MemoryTransport>,
    handles: Vec<PeerHandle>,
    alive: Vec<bool>,
    t0: Instant,
    now: Duration,
}

impl Hub {
    fn new(n: usize) -> Self {
        let mut hub = Hub {
            relay: Relay::new(ClientId::new("hub").unwrap()),
            stages: Vec::new(),
            sessions: Vec::new(),
            links: Vec::new(),
            handles: Vec::new(),
            alive: Vec::new(),
            t0: Instant::now(),
            now: Duration::ZERO,
        };
        for _ in 0..n {
            hub.join();
        }
        hub
    }

    fn join(&mut self) -> usize {
        let i = self.sessions.len();
        let (mine, theirs) = MemoryTransport::pair();
        self.handles.push(self.relay.connect(Box::new(theirs)));
        self.links.push(mine);
        self.stages.push(Stage::new());
        self.sessions.push(SyncSession::new(SessionConfig::new(
            ClientId::new(&format!("p{i}")).unwrap(),
        )));
        self.alive.push(true);
        i
    }

    /// Advance the simulated clock and pump every live client and the relay.
    fn step(&mut self, dt: Duration) -> Vec<bool> {
        self.now += dt;
        let now = self.t0 + self.now;
        let mut changed = vec![false; self.sessions.len()];
        for _ in 0..2 {
            for i in 0..self.sessions.len() {
                if self.alive[i] {
                    let out = self.sessions[i]
                        .pump_at(&mut self.stages[i], &mut self.links[i], now)
                        .unwrap();
                    changed[i] |= out.presence_changed;
                }
            }
            self.relay.pump_at(now);
        }
        changed
    }
}

#[test]
fn presence_through_the_hub() {
    let mut hub = Hub::new(3);
    let mut a = state("alice");
    a.selection = vec![p("/comps/main/layers/title")];
    a.playhead = Some(Playhead {
        comp: p("/comps/main"),
        time: Time::ONE_SECOND,
    });
    a.workspace = Some("motion".into());
    a.editing = vec![p("/comps/main")];
    hub.sessions[0].set_presence(a.clone());
    hub.sessions[1].set_presence(state("bob"));
    let changed = hub.step(Duration::from_millis(10));
    assert!(changed[2]);
    assert_eq!(names(&hub.sessions[2]), ["alice", "bob"]);
    assert_eq!(names(&hub.sessions[0]), ["bob"], "nobody sees themselves");
    let seen = &hub.sessions[2].presence()[0];
    assert_eq!(seen.client.as_str(), "p0");
    assert_eq!(seen.state, a);

    // Advisory lock: a hint for anything inside the comp, nothing elsewhere.
    let editing = hub.sessions[1].editing_by_others(&p("/comps/main/layers/title"));
    assert_eq!(editing.len(), 1);
    assert_eq!(editing[0].state.name, "alice");
    assert!(
        hub.sessions[1]
            .editing_by_others(&p("/comps/other"))
            .is_empty()
    );

    // Changes arrive; an unchanged state is not re-reported.
    a.selection.clear();
    hub.sessions[0].set_presence(a.clone());
    assert!(hub.step(Duration::from_millis(10))[2]);
    assert!(hub.sessions[2].presence()[0].state.selection.is_empty());
    assert!(!hub.step(Duration::from_millis(10))[2]);

    // A late joiner sees everyone immediately.
    let d = hub.join();
    hub.step(Duration::from_millis(10));
    assert_eq!(names(&hub.sessions[d]), ["alice", "bob"]);

    // Heartbeats keep people online well past the TTL...
    for _ in 0..12 {
        hub.step(Duration::from_secs(5));
    }
    assert_eq!(names(&hub.sessions[2]), ["alice", "bob"]);
    // ...until a client stops (frozen, network gone without a clean close).
    hub.alive[1] = false;
    let mut dropped = false;
    for _ in 0..6 {
        dropped |= hub.step(Duration::from_secs(5))[2];
    }
    assert!(dropped);
    assert_eq!(names(&hub.sessions[2]), ["alice"]);

    // Leaving is seen at once.
    hub.sessions[0].clear_presence();
    assert!(hub.step(Duration::from_millis(10))[2]);
    assert!(names(&hub.sessions[2]).is_empty());

    // A dropped connection: the relay tells the others.
    hub.sessions[2].set_presence(state("carol"));
    hub.step(Duration::from_millis(10));
    assert_eq!(names(&hub.sessions[d]), ["carol"]);
    let carol = hub.handles[2];
    hub.relay.disconnect(carol);
    hub.alive[2] = false;
    hub.step(Duration::from_millis(10));
    assert!(names(&hub.sessions[d]).is_empty());
}

#[test]
fn stale_or_garbled_presence_is_ignored() {
    let mut hub = Hub::new(2);
    hub.sessions[0].set_presence(state("alice"));
    hub.step(Duration::from_millis(10));
    // A replayed older heartbeat (seq 1) cannot resurrect or alter anyone.
    let (mut fake, theirs) = MemoryTransport::pair();
    hub.relay.connect(Box::new(theirs));
    fake.send(bae_sync::Message::Presence {
        client: ClientId::new("p0").unwrap(),
        seq: 1,
        ttl_ms: 20_000,
        state: Some(br#"{"name":"mallory"}"#.to_vec()),
    })
    .unwrap();
    fake.send(bae_sync::Message::Presence {
        client: ClientId::new("x").unwrap(),
        seq: u64::MAX,
        ttl_ms: 20_000,
        state: Some(b"not json".to_vec()),
    })
    .unwrap();
    hub.step(Duration::from_millis(10));
    assert_eq!(names(&hub.sessions[1]), ["alice"]);
}

#[test]
fn presence_through_the_shared_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("Film.bae");
    Project::from_stage(client_stage(&base_snapshots()))
        .save_as(&dir)
        .unwrap();
    let config = DirConfig {
        poll_interval: Duration::ZERO,
        watch: false,
        ..DirConfig::default()
    };
    let open = |id: &str| {
        let mut sc = SessionConfig::new(ClientId::new(id).unwrap());
        sc.hello_interval = None;
        sc.presence_heartbeat = Duration::from_millis(50);
        sc.presence_ttl = Duration::from_millis(400);
        (
            Project::open(&dir).unwrap().stage,
            SyncSession::new(sc),
            DirTransport::open(&dir, ClientId::new(id).unwrap(), config.clone()).unwrap(),
        )
    };
    let (mut sa, mut a, mut ta) = open("a");
    let (mut sb, mut b, mut tb) = open("b");
    let mut alice = state("alice");
    alice.editing = vec![p("/comps/main")];
    a.set_presence(alice);
    a.pump(&mut sa, &mut ta).unwrap();
    b.pump(&mut sb, &mut tb).unwrap();
    assert!(dir.join("presence/a.presence").is_file());
    assert_eq!(names(&b), ["alice"]);
    assert_eq!(b.editing_by_others(&p("/comps/main/layers")).len(), 1);

    // Heartbeats keep alice online; when she stops, she times out.
    let until = Instant::now() + Duration::from_millis(600);
    while Instant::now() < until {
        a.pump(&mut sa, &mut ta).unwrap();
        b.pump(&mut sb, &mut tb).unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(names(&b), ["alice"]);
    std::thread::sleep(Duration::from_millis(500));
    assert!(b.pump(&mut sb, &mut tb).unwrap().presence_changed);
    assert!(names(&b).is_empty());

    // Back, then a clean leave: seen on the next poll, no TTL wait.
    a.set_presence(state("alice"));
    a.pump(&mut sa, &mut ta).unwrap();
    b.pump(&mut sb, &mut tb).unwrap();
    assert_eq!(names(&b), ["alice"]);
    a.clear_presence();
    a.pump(&mut sa, &mut ta).unwrap();
    b.pump(&mut sb, &mut tb).unwrap();
    assert!(names(&b).is_empty());

    // A heartbeat file left by a crashed client long ago is ignored.
    let (mut sg, mut g, mut tg) = open("g");
    g.set_presence(state("ghost"));
    g.pump(&mut sg, &mut tg).unwrap();
    drop((g, tg));
    let ghost = dir.join("presence/g.presence");
    let old = std::time::SystemTime::now() - Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(&ghost)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let (mut sc, mut c, mut tc) = open("c");
    c.pump(&mut sc, &mut tc).unwrap();
    assert!(names(&c).is_empty());
    // A fresh one is seen.
    fs::File::options()
        .write(true)
        .open(&ghost)
        .unwrap()
        .set_modified(std::time::SystemTime::now())
        .unwrap();
    c.pump(&mut sc, &mut tc).unwrap();
    assert_eq!(names(&c), ["ghost"]);
}

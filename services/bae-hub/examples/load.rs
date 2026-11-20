//! Load test against a running hub (a NAS on the LAN, or a local one):
//! N clients, the app's own client code (`HubTransport` + `ThreadedTransport`).
//!
//! ```sh
//! cargo run --release -p bae-hub --example load -- \
//!     --url ws://nas.local:7341/projects/loadtest --token <write token> \
//!     --clients 10 --edits 200 --idle 30
//! ```
//!
//! Phase 1: one client after another changes a value; measures until all the
//! others have it (task book 10: < 100 ms on a LAN). Phase 2: everyone stays
//! connected and idle for `--idle` seconds (watch the hub's CPU and memory).

use std::time::{Duration, Instant};

use bae_model::{AuthorId, Op, PrimPath, PrimType, Specifier, Stage, Sublayer, SublayerId, Value};
use bae_sync::{
    ClientId, DocKey, DocSet, HubTransport, SessionConfig, SyncSession, ThreadedTransport,
};

struct Args {
    url: String,
    token: Option<String>,
    clients: usize,
    edits: usize,
    idle: u64,
}

fn args() -> Args {
    let mut a = Args {
        url: "ws://127.0.0.1:7341/projects/loadtest".into(),
        token: None,
        clients: 10,
        edits: 200,
        idle: 0,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("{k} needs a value"));
        match k.as_str() {
            "--url" => a.url = v(),
            "--token" => a.token = Some(v()),
            "--clients" => a.clients = v().parse().expect("--clients"),
            "--edits" => a.edits = v().parse().expect("--edits"),
            "--idle" => a.idle = v().parse().expect("--idle"),
            other => panic!("unknown argument {other}"),
        }
    }
    a
}

struct Client {
    stage: Stage,
    session: SyncSession,
    link: ThreadedTransport,
    who: AuthorId,
}

fn title() -> PrimPath {
    PrimPath::new("/title").unwrap()
}

fn main() {
    let a = args();
    // Every client starts from the same base: one layer in `root`.
    let mut base = Stage::from_sublayers(vec![Sublayer::new(SublayerId::root())]).unwrap();
    base.transact(&AuthorId::system(), "base", |tx| {
        tx.apply(Op::CreatePrim {
            path: title(),
            specifier: Specifier::Def,
            prim_type: Some(PrimType::new("Layer")),
            index: None,
        })
    })
    .unwrap();
    let snapshot = base
        .sublayer(&SublayerId::root())
        .unwrap()
        .export_snapshot()
        .unwrap();
    let run = uuid::Uuid::new_v4().simple().to_string();
    let mut clients: Vec<Client> = (0..a.clients)
        .map(|i| {
            let name = format!("load-{}-{i}", &run[..6]);
            let hub = HubTransport::connect(&a.url, a.token.as_deref(), Duration::from_secs(5))
                .expect("connect");
            Client {
                stage: Stage::from_sublayers(vec![
                    Sublayer::from_snapshot(SublayerId::root(), &snapshot).unwrap(),
                ])
                .unwrap(),
                session: SyncSession::new(SessionConfig::new(ClientId::new(&name).unwrap())),
                link: ThreadedTransport::spawn(hub, Duration::from_millis(1), || {}),
                who: AuthorId::new(&format!("user:{name}")).unwrap(),
            }
        })
        .collect();
    let pump = |clients: &mut [Client]| {
        for c in clients.iter_mut() {
            c.session
                .pump(&mut c.stage, &mut c.link)
                .expect("connection lost");
        }
    };
    let start = Instant::now();
    while !clients.iter().all(|c| c.session.is_connected()) {
        pump(&mut clients);
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "hub did not answer"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // Catch up with whatever the project already holds.
    for _ in 0..200 {
        pump(&mut clients);
        std::thread::sleep(Duration::from_millis(5));
    }

    let root = DocKey::sublayer(&SublayerId::root());
    let mut samples = Vec::with_capacity(a.edits);
    for round in 0..a.edits {
        let c = &mut clients[round % a.clients];
        let who = c.who.clone();
        c.stage
            .transact(&who, "edit", |tx| {
                tx.apply(Op::SetValue {
                    path: title(),
                    attr: "opacity".into(),
                    value: Some(Value::Float((round % 101) as f64)),
                })
            })
            .unwrap();
        let want = c.stage.version(&root).unwrap();
        let t0 = Instant::now();
        loop {
            pump(&mut clients);
            if clients
                .iter()
                .all(|c| c.stage.version(&root).is_some_and(|v| v.includes_vv(&want)))
            {
                break;
            }
            assert!(t0.elapsed() < Duration::from_secs(10), "edit {round} lost");
            std::thread::sleep(Duration::from_micros(200));
        }
        samples.push(t0.elapsed());
        // A human pace: a few edits per second per person, not a flood.
        std::thread::sleep(Duration::from_millis(20));
    }
    samples.sort();
    let pct = |p: usize| samples[(samples.len() * p / 100).min(samples.len() - 1)];
    println!(
        "{} clients, {} edits, edit → all others: median {:?}, p95 {:?}, max {:?}",
        a.clients,
        a.edits,
        pct(50),
        pct(95),
        samples.last().unwrap()
    );

    if a.idle > 0 {
        println!("idling {} s with {} clients connected", a.idle, a.clients);
        let until = Instant::now() + Duration::from_secs(a.idle);
        while Instant::now() < until {
            pump(&mut clients);
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

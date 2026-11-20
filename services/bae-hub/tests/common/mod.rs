//! Test helpers: a hub on a random port, clients that sync a real `Stage` over
//! `bae_sync::HubTransport` (the app's own client code), a base project and
//! random edits.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use bae_hub::{Access, HubConfig, RunningHub, TokenConfig};
use bae_model::{
    AuthorId, Keyframe, Op, PrimPath, PrimType, Specifier, Stage, Sublayer, SublayerId, Time, Value,
};
use bae_sync::{
    ClientId, HubTransport, PumpOutcome, SessionConfig, SyncError, SyncSession, ThreadedTransport,
};

pub const WRITE: &str = "write-token-0123456789";
pub const READ: &str = "read-token-0123456789";

pub fn token(name: &str, secret: &str, access: Access, projects: Option<&[&str]>) -> TokenConfig {
    TokenConfig {
        name: name.into(),
        token: secret.into(),
        access,
        projects: projects.map(|p| p.iter().map(|s| (*s).to_owned()).collect()),
    }
}

/// Hub settings for tests: loopback, random port, a write and a read token.
pub fn config(data: &Path) -> HubConfig {
    let mut c = HubConfig::new(data);
    c.listen = SocketAddr::from(([127, 0, 0, 1], 0));
    c.tokens = vec![
        token("team", WRITE, Access::Write, None),
        token("viewer", READ, Access::Read, None),
    ];
    c
}

pub fn start(config: HubConfig) -> RunningHub {
    bae_hub::start(config).expect("start hub")
}

pub fn url(addr: SocketAddr, project: &str) -> String {
    format!("ws://{addr}{}{project}", bae_hub::SYNC_PREFIX)
}

/// Connect the way the app does: `HubTransport` on a `ThreadedTransport`.
pub fn connect(
    addr: SocketAddr,
    project: &str,
    token: Option<&str>,
) -> Result<ThreadedTransport, SyncError> {
    let hub = HubTransport::connect(&url(addr, project), token, Duration::from_secs(5))?;
    Ok(ThreadedTransport::spawn(
        hub,
        Duration::from_millis(1),
        || {},
    ))
}

/// The error text of a refused connection (contains the HTTP status).
pub fn refused(addr: SocketAddr, project: &str, token: Option<&str>) -> String {
    match connect(addr, project, token) {
        Ok(_) => panic!("connection to {project} with {token:?} was accepted"),
        Err(e) => e.to_string(),
    }
}

/// An app instance: a stage synced with the hub.
pub struct Client {
    pub stage: Stage,
    pub session: SyncSession,
    pub link: ThreadedTransport,
    pub who: AuthorId,
    pub edits: u32,
}

impl Client {
    pub fn join(
        addr: SocketAddr,
        project: &str,
        token: &str,
        name: &str,
        base: &[(SublayerId, Vec<u8>)],
    ) -> Client {
        Client {
            stage: client_stage(base),
            session: SyncSession::new(SessionConfig::new(ClientId::new(name).unwrap())),
            link: connect(addr, project, Some(token)).expect("connect"),
            who: author(&format!("user:{name}")),
            edits: 0,
        }
    }

    pub fn pump(&mut self) -> PumpOutcome {
        self.session
            .pump(&mut self.stage, &mut self.link)
            .expect("pump")
    }

    pub fn edit(&mut self, rng: &mut Rng) {
        let tag = self.session.client().as_str().to_owned();
        random_edit(&mut self.stage, &self.who, rng, &tag, &mut self.edits);
    }

    pub fn fingerprint(&self) -> String {
        fingerprint(&self.stage)
    }
}

/// Pump every client until `done` holds (panics after `timeout`); returns how
/// long it took.
pub fn pump_until(
    clients: &mut [Client],
    timeout: Duration,
    mut done: impl FnMut(&[Client]) -> bool,
) -> Duration {
    let start = Instant::now();
    loop {
        for c in clients.iter_mut() {
            c.pump();
        }
        if done(clients) {
            return start.elapsed();
        }
        assert!(start.elapsed() < timeout, "timed out after {timeout:?}");
        std::thread::sleep(Duration::from_micros(500));
    }
}

/// Pump for a while regardless.
pub fn pump_for(clients: &mut [Client], d: Duration) {
    let start = Instant::now();
    while start.elapsed() < d {
        for c in clients.iter_mut() {
            c.pump();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub fn converged(clients: &[Client]) -> bool {
    let first = clients[0].fingerprint();
    clients[1..].iter().all(|c| c.fingerprint() == first)
}

// ---- project helpers (as in bae-sync's tests) ----

pub fn p(s: &str) -> PrimPath {
    PrimPath::new(s).unwrap()
}

pub fn sl(s: &str) -> SublayerId {
    SublayerId::new(s).unwrap()
}

pub fn author(s: &str) -> AuthorId {
    AuthorId::new(s).unwrap()
}

/// xorshift64*: deterministic, no dependencies.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }
}

pub const LAYERS: &str = "/comps/main/layers";

/// `root` defines /comps/main/layers with one layer; `anim/a` (stronger) holds
/// `over`s for the same chain.
pub fn base_snapshots() -> Vec<(SublayerId, Vec<u8>)> {
    let mut stage = Stage::from_sublayers(vec![
        Sublayer::new(sl("anim/a")),
        Sublayer::new(SublayerId::root()),
    ])
    .unwrap();
    let who = AuthorId::system();
    for (layer, spec) in [
        (SublayerId::root(), Specifier::Def),
        (sl("anim/a"), Specifier::Over),
    ] {
        stage.set_edit_target(&layer).unwrap();
        stage
            .transact(&who, "base", |tx| {
                for (path, t) in [
                    ("/comps", "Folder"),
                    ("/comps/main", "Comp"),
                    (LAYERS, "Folder"),
                ] {
                    tx.apply(Op::CreatePrim {
                        path: p(path),
                        specifier: spec,
                        prim_type: (spec == Specifier::Def).then(|| PrimType::new(t)),
                        index: None,
                    })?;
                }
                Ok::<_, bae_model::ModelError>(())
            })
            .unwrap();
    }
    stage.set_edit_target(&SublayerId::root()).unwrap();
    stage
        .transact(&who, "base", |tx| {
            tx.apply(Op::CreatePrim {
                path: p("/comps/main/layers/title"),
                specifier: Specifier::Def,
                prim_type: Some(PrimType::new("Layer")),
                index: None,
            })
        })
        .unwrap();
    stage
        .sublayers()
        .map(|s| (s.id().clone(), s.export_snapshot().unwrap()))
        .collect()
}

/// A stage loaded from the base snapshots (fresh Loro peer IDs).
pub fn client_stage(base: &[(SublayerId, Vec<u8>)]) -> Stage {
    Stage::from_sublayers(
        base.iter()
            .map(|(id, bytes)| Sublayer::from_snapshot(id.clone(), bytes).unwrap())
            .collect(),
    )
    .unwrap()
}

/// One random edit in a random sublayer; edits that no longer apply are ignored.
pub fn random_edit(stage: &mut Stage, who: &AuthorId, rng: &mut Rng, tag: &str, n: &mut u32) {
    let target = if rng.chance(50) {
        SublayerId::root()
    } else {
        sl("anim/a")
    };
    stage.set_edit_target(&target).unwrap();
    let children = stage.sublayer(&target).unwrap().children(&p(LAYERS));
    let pick = |rng: &mut Rng| -> Option<PrimPath> {
        (!children.is_empty()).then(|| children[rng.below(children.len())].clone())
    };
    *n += 1;
    let roll = rng.below(100);
    let op = if roll < 35 || children.is_empty() {
        Op::CreatePrim {
            path: p(&format!("{LAYERS}/{tag}_{n}")),
            specifier: Specifier::Def,
            prim_type: Some(PrimType::new("Layer")),
            index: None,
        }
    } else if roll < 70 {
        Op::SetValue {
            path: pick(rng).unwrap(),
            attr: "opacity".into(),
            value: Some(Value::Float(rng.below(101) as f64)),
        }
    } else if roll < 90 {
        Op::SetKeyframe {
            path: pick(rng).unwrap(),
            attr: "transform:position".into(),
            time: Time::from_flicks(rng.below(10) as i64 * Time::FLICKS_PER_SECOND),
            keyframe: Keyframe::linear(Value::Vec2([rng.below(1920) as f64, 540.0])),
        }
    } else {
        Op::DeletePrim {
            path: pick(rng).unwrap(),
        }
    };
    let _ = stage.transact(who, "edit", |tx| tx.apply(op));
}

/// Everything that must be identical on converged clients.
pub fn fingerprint(stage: &Stage) -> String {
    let mut out = String::new();
    for layer in stage.sublayers() {
        out.push_str(layer.id().as_str());
        out.push('\n');
        out.push_str(&serde_json::to_string(&layer.to_trees()).unwrap());
        out.push('\n');
    }
    out
}

/// `GET path` on the hub.
pub fn http_get(addr: SocketAddr, path: &str, token: Option<&str>) -> bae_hub::probe::Response {
    bae_hub::probe::get(addr, path, token, Duration::from_secs(5)).expect("http")
}

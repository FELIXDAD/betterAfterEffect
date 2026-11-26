//! Test helpers: a hub on a random port, clients that sync a real `Stage` over
//! `bae_sync::HubTransport` (the app's own client code), a base project and
//! random edits.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use bae_hub::{Access, HubConfig, RunningHub, TokenConfig};
use bae_model::loro::{ExportMode, LoroDoc};
use bae_model::{
    AuthorId, Keyframe, Op, PrimPath, PrimType, Specifier, Stage, Sublayer, SublayerId, Time, Value,
};
use bae_sync::{
    ClientId, DirConfig, DirTransport, DocKey, DocSet, HubTransport, MemDocs, PumpOutcome,
    SessionConfig, SyncError, SyncSession, ThreadedTransport,
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

/// A TCP proxy to `target` on a random loopback port that passes at most
/// `bytes_per_sec` from the client to `target` (the other way is not limited):
/// a slow uplink.
pub fn throttled_proxy(target: SocketAddr, bytes_per_sec: usize) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for client in listener.incoming() {
            let Ok(client) = client else {
                return;
            };
            let Ok(server) = TcpStream::connect(target) else {
                return;
            };
            let (client2, server2) = (client.try_clone().unwrap(), server.try_clone().unwrap());
            std::thread::spawn(move || relay_bytes(client, server, Some(bytes_per_sec)));
            std::thread::spawn(move || relay_bytes(server2, client2, None));
        }
    });
    addr
}

fn relay_bytes(mut from: TcpStream, mut to: TcpStream, rate: Option<usize>) {
    let mut buf = vec![0u8; 16 << 10];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if to.write_all(&buf[..n]).is_err() {
            break;
        }
        if let Some(rate) = rate {
            std::thread::sleep(Duration::from_secs_f64(n as f64 / rate as f64));
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

/// Set `attr` of the title layer to `len` random letters (Loro cannot compress
/// them much). Returns the version that includes the edit.
pub fn paste(c: &mut Client, attr: &str, len: usize, seed: u64) -> bae_model::loro::VersionVector {
    let mut rng = Rng::new(seed);
    let text: String = (0..len)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect();
    c.stage.set_edit_target(&SublayerId::root()).unwrap();
    let who = c.who.clone();
    c.stage
        .transact(&who, "paste", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: attr.into(),
                value: Some(Value::String(text)),
            })
        })
        .unwrap();
    c.stage
        .version(&DocKey::sublayer(&SublayerId::root()))
        .unwrap()
}

/// Pump `c` (not panicking on a link error) until its link fails; `others`
/// keep pumping meanwhile. Returns the error, or `None` after `timeout`.
pub fn pump_until_dropped(
    c: &mut Client,
    others: &mut [Client],
    timeout: Duration,
) -> Option<SyncError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Err(e) = c.session.pump(&mut c.stage, &mut c.link) {
            return Some(e);
        }
        for o in others.iter_mut() {
            o.pump();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    None
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

// ---- plain documents (no stage), for tests that need many large ones ----

/// `n` documents `doc0`… of `len` random letters each (Loro cannot compress
/// them much).
pub fn big_docs(n: usize, len: usize, seed: u64) -> MemDocs {
    let mut rng = Rng::new(seed);
    let mut docs = MemDocs::new();
    for i in 0..n {
        let mut text = String::with_capacity(len + 8);
        while text.len() < len {
            for b in rng.next().to_le_bytes() {
                text.push(char::from(b'a' + b % 26));
            }
        }
        text.truncate(len);
        let doc = LoroDoc::new();
        doc.get_text("t").insert(0, &text).unwrap();
        doc.commit();
        let snapshot = doc.export(ExportMode::Snapshot).unwrap();
        docs.load(DocKey::parse(&format!("doc{i}")).unwrap(), &snapshot)
            .unwrap();
    }
    docs
}

/// A client syncing plain documents with the hub, linked like the app.
pub struct DocsClient {
    pub docs: MemDocs,
    pub session: SyncSession,
    pub link: ThreadedTransport,
}

impl DocsClient {
    pub fn join(addr: SocketAddr, project: &str, name: &str, docs: MemDocs) -> DocsClient {
        DocsClient {
            docs,
            session: SyncSession::new(SessionConfig::new(ClientId::new(name).unwrap())),
            link: connect(addr, project, Some(WRITE)).expect("connect"),
        }
    }

    pub fn pump(&mut self) {
        self.session
            .pump(&mut self.docs, &mut self.link)
            .expect("pump");
    }

    /// Has every document of `want` at least at its version.
    pub fn has(&self, want: &MemDocs) -> bool {
        missing(want, &self.docs).is_empty()
    }
}

/// Documents of `want` that `got` lacks or has at an older version.
pub fn missing(want: &MemDocs, got: &MemDocs) -> Vec<DocKey> {
    want.doc_keys()
        .into_iter()
        .filter(|k| {
            let w = want.version(k).unwrap();
            !got.version(k).is_some_and(|g| g.includes_vv(&w))
        })
        .collect()
}

/// What the project directory `dir` holds, read the way the hub loads it
/// (everything before the directory's `Hello`).
pub fn read_project_dir(dir: &Path) -> MemDocs {
    let config = DirConfig {
        watch: false,
        log_gc_after: None,
        ..DirConfig::default()
    };
    let mut link = DirTransport::open(dir, ClientId::new("check").unwrap(), config).unwrap();
    let mut session = SyncSession::new(SessionConfig::new(ClientId::new("check").unwrap()));
    let mut docs = MemDocs::new();
    let start = Instant::now();
    while !session.is_connected() {
        session.pump(&mut docs, &mut link).unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the project directory did not finish loading"
        );
    }
    docs
}

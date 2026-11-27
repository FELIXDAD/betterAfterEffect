//! Shared test helpers: a deterministic RNG, a lossy simulated network, a base
//! project and random edits.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bae_model::{
    AuthorId, Keyframe, Op, PrimPath, PrimType, Specifier, Stage, Sublayer, SublayerId, Time, Value,
};
use bae_sync::{Message, SyncError, Transport};

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

    /// True with probability `percent`/100.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }
}

/// Link conditions.
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub drop_percent: u32,
    pub duplicate_percent: u32,
    /// Each packet is delayed by 0..=max_delay ticks, which also reorders them.
    pub max_delay: u64,
}

impl Conditions {
    pub const PERFECT: Conditions = Conditions {
        drop_percent: 0,
        duplicate_percent: 0,
        max_delay: 0,
    };
    pub const BAD: Conditions = Conditions {
        drop_percent: 20,
        duplicate_percent: 10,
        max_delay: 6,
    };
}

struct Packet {
    to: usize,
    at: u64,
    seq: u64,
    bytes: Vec<u8>,
}

struct NetState {
    rng: Rng,
    now: u64,
    seq: u64,
    conditions: Conditions,
    in_flight: Vec<Packet>,
    inbox: Vec<VecDeque<Vec<u8>>>,
    delivered: u64,
    dropped: u64,
}

/// A simulated network: endpoints exchange encoded messages subject to loss,
/// duplication and delay. Time advances only through [`SimNet::tick`].
#[derive(Clone)]
pub struct SimNet(Arc<Mutex<NetState>>);

impl SimNet {
    pub fn new(seed: u64, conditions: Conditions) -> Self {
        SimNet(Arc::new(Mutex::new(NetState {
            rng: Rng::new(seed),
            now: 0,
            seq: 0,
            conditions,
            in_flight: Vec::new(),
            inbox: Vec::new(),
            delivered: 0,
            dropped: 0,
        })))
    }

    pub fn set_conditions(&self, c: Conditions) {
        self.0.lock().unwrap().conditions = c;
    }

    /// A bidirectional link: two transports.
    pub fn link(&self) -> (SimEnd, SimEnd) {
        let mut s = self.0.lock().unwrap();
        let a = s.inbox.len();
        s.inbox.push(VecDeque::new());
        s.inbox.push(VecDeque::new());
        (
            SimEnd {
                net: self.clone(),
                me: a,
                peer: a + 1,
                lossless: false,
                keep_hellos: false,
            },
            SimEnd {
                net: self.clone(),
                me: a + 1,
                peer: a,
                lossless: false,
                keep_hellos: false,
            },
        )
    }

    /// Advance one tick: deliver due packets (in delay order, so reordered).
    pub fn tick(&self) {
        let mut s = self.0.lock().unwrap();
        s.now += 1;
        let now = s.now;
        let mut due: Vec<Packet> = Vec::new();
        let mut i = 0;
        while i < s.in_flight.len() {
            if s.in_flight[i].at <= now {
                due.push(s.in_flight.swap_remove(i));
            } else {
                i += 1;
            }
        }
        due.sort_by_key(|p| (p.at, p.seq));
        for p in due {
            s.delivered += 1;
            s.inbox[p.to].push_back(p.bytes);
        }
    }

    pub fn now(&self) -> u64 {
        self.0.lock().unwrap().now
    }

    pub fn idle(&self) -> bool {
        let s = self.0.lock().unwrap();
        s.in_flight.is_empty() && s.inbox.iter().all(VecDeque::is_empty)
    }

    pub fn stats(&self) -> (u64, u64) {
        let s = self.0.lock().unwrap();
        (s.delivered, s.dropped)
    }

    /// Simulated wall clock for `pump_at`: 100 ms per tick.
    pub fn instant(&self, base: Instant) -> Instant {
        base + Duration::from_millis(100 * self.now())
    }
}

pub struct SimEnd {
    net: SimNet,
    me: usize,
    peer: usize,
    /// What this end sends is never dropped (still duplicated and delayed).
    lossless: bool,
    /// The `Hello`s this end sends are never dropped.
    keep_hellos: bool,
}

impl SimEnd {
    /// What this end sends is never dropped, whatever the conditions.
    pub fn lossless(mut self) -> Self {
        self.lossless = true;
        self
    }

    /// The `Hello`s this end sends are never dropped, whatever the conditions.
    pub fn keep_hellos(mut self) -> Self {
        self.keep_hellos = true;
        self
    }
}

impl Transport for SimEnd {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let keep = self.lossless || (self.keep_hellos && matches!(msg, Message::Hello { .. }));
        let bytes = msg.encode();
        let mut s = self.net.0.lock().unwrap();
        let c = s.conditions;
        if s.rng.chance(c.drop_percent) && !keep {
            s.dropped += 1;
            return Ok(());
        }
        let copies = if s.rng.chance(c.duplicate_percent) {
            2
        } else {
            1
        };
        for _ in 0..copies {
            let delay = if c.max_delay == 0 {
                0
            } else {
                s.rng.next() % (c.max_delay + 1)
            };
            let at = s.now + delay;
            s.seq += 1;
            let seq = s.seq;
            let to = self.peer;
            s.in_flight.push(Packet {
                to,
                at,
                seq,
                bytes: bytes.clone(),
            });
        }
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        let mut s = self.net.0.lock().unwrap();
        let Some(bytes) = s.inbox[self.me].pop_front() else {
            return Ok(None);
        };
        Message::decode(&bytes).map(Some)
    }
}

pub const LAYERS: &str = "/comps/main/layers";

/// The project every simulated client starts from: `root` defines
/// /comps/main/layers with one layer; `anim/a` (stronger) holds `over`s for the
/// same chain so edits can target either sublayer.
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

/// A client stage loaded from the base snapshots (fresh Loro peer IDs).
pub fn client_stage(base: &[(SublayerId, Vec<u8>)]) -> Stage {
    Stage::from_sublayers(
        base.iter()
            .map(|(id, bytes)| Sublayer::from_snapshot(id.clone(), bytes).unwrap())
            .collect(),
    )
    .unwrap()
}

/// One random edit (create, set value, keyframe, delete, rename or undo) by
/// `who` in a random sublayer. Edits that no longer apply are ignored.
pub fn random_edit(stage: &mut Stage, who: &AuthorId, rng: &mut Rng, tag: &str, n: &mut u32) {
    random_edit_with(stage, who, rng, tag, n, true);
}

/// [`random_edit`]; with `collide` false, clients never create the same name
/// concurrently (the random sequence is the same either way).
pub fn random_edit_with(
    stage: &mut Stage,
    who: &AuthorId,
    rng: &mut Rng,
    tag: &str,
    n: &mut u32,
    collide: bool,
) {
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
    let op = if roll < 30 || children.is_empty() {
        // Sometimes several clients create the same name concurrently.
        let name = if rng.chance(15) && collide {
            "shared".to_owned()
        } else {
            format!("{tag}_{n}")
        };
        Op::CreatePrim {
            path: p(&format!("{LAYERS}/{name}")),
            specifier: Specifier::Def,
            prim_type: Some(PrimType::new("Layer")),
            index: None,
        }
    } else if roll < 60 {
        Op::SetValue {
            path: pick(rng).unwrap(),
            attr: "opacity".into(),
            value: Some(Value::Float(rng.below(101) as f64)),
        }
    } else if roll < 78 {
        Op::SetKeyframe {
            path: pick(rng).unwrap(),
            attr: "transform:position".into(),
            time: Time::from_flicks(rng.below(10) as i64 * Time::FLICKS_PER_SECOND),
            keyframe: Keyframe::linear(Value::Vec2([rng.below(1920) as f64, 540.0])),
        }
    } else if roll < 86 {
        Op::DeletePrim {
            path: pick(rng).unwrap(),
        }
    } else if roll < 94 {
        Op::MovePrim {
            from: pick(rng).unwrap(),
            to: p(&format!("{LAYERS}/{tag}_mv{n}")),
            index: None,
        }
    } else {
        let _ = stage.undo(who);
        return;
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

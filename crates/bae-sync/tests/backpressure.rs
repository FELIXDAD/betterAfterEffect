//! The session on a slow link, simulated: messages leave the sender's queue at a
//! fixed rate (`Transport::backlog` reports what is still queued) and arrive a
//! while after their last byte left. A large update is sent once, even though
//! the receiver's `Hello`s, sent before it arrived, keep saying it lacks it; a
//! first sync of many documents goes out one document at a time.

mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bae_model::loro::{ExportMode, LoroDoc, VersionVector};
use bae_model::{Op, SublayerId, Value};
use bae_sync::{
    ClientId, DocKey, DocSet, MemDocs, Message, SessionConfig, SyncError, SyncSession, Transport,
};
use common::*;

/// One direction of the link.
#[derive(Default)]
struct Pipe {
    /// Messages not fully sent, with their bytes still to go.
    queue: VecDeque<(Message, usize)>,
    /// Sent, arriving at the given tick.
    wire: VecDeque<(u64, Message)>,
    arrived: VecDeque<Message>,
    /// Bytes per tick (`usize::MAX`: no limit).
    rate: usize,
    /// Ticks from a message's last byte leaving to its arrival.
    lag: u64,
}

impl Pipe {
    fn tick(&mut self, now: u64) {
        let mut budget = self.rate;
        while budget > 0 {
            let Some((_, left)) = self.queue.front_mut() else {
                break;
            };
            let n = (*left).min(budget);
            *left -= n;
            budget -= n;
            if *left == 0 {
                let (msg, _) = self.queue.pop_front().unwrap();
                self.wire.push_back((now + self.lag, msg));
            }
        }
        while self.wire.front().is_some_and(|(at, _)| *at <= now) {
            let (_, msg) = self.wire.pop_front().unwrap();
            self.arrived.push_back(msg);
        }
    }

    fn backlog(&self) -> usize {
        self.queue.iter().map(|(_, left)| left).sum()
    }
}

#[derive(Default)]
struct LinkState {
    now: u64,
    /// `pipes[i]` carries what end `i` sends.
    pipes: [Pipe; 2],
}

#[derive(Clone)]
struct Link(Arc<Mutex<LinkState>>);

impl Link {
    /// End 0 sends at `rate` bytes per tick with `lag`; end 1 without limits.
    fn new(rate: usize, lag: u64) -> (Link, End, End) {
        let mut state = LinkState::default();
        state.pipes[0].rate = rate;
        state.pipes[0].lag = lag;
        state.pipes[1].rate = usize::MAX;
        let link = Link(Arc::new(Mutex::new(state)));
        let a = End {
            link: link.clone(),
            me: 0,
        };
        let b = End {
            link: link.clone(),
            me: 1,
        };
        (link, a, b)
    }

    fn tick(&self) {
        let mut s = self.0.lock().unwrap();
        s.now += 1;
        let now = s.now;
        for p in &mut s.pipes {
            p.tick(now);
        }
    }
}

struct End {
    link: Link,
    me: usize,
}

impl Transport for End {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let len = msg.encode().len();
        self.link.0.lock().unwrap().pipes[self.me]
            .queue
            .push_back((msg, len));
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        Ok(self.link.0.lock().unwrap().pipes[1 - self.me]
            .arrived
            .pop_front())
    }

    fn backlog(&self) -> usize {
        self.link.0.lock().unwrap().pipes[self.me].backlog()
    }
}

/// 100 ms of simulated time per tick.
const TICK: Duration = Duration::from_millis(100);

fn config(name: &str) -> SessionConfig {
    let mut c = SessionConfig::new(ClientId::new(name).unwrap());
    c.hello_interval = Some(Duration::from_millis(500));
    c
}

fn letters(len: usize, seed: u64) -> String {
    let mut rng = Rng::new(seed);
    (0..len)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect()
}

/// An 8 MiB edit over a link that takes 32 ticks to send it: the receiver says
/// hello every 5 ticks, each time without the edit. The sender holds back while
/// its queue is full and counts the edit as on its way until a hello shows it
/// or long after it left, so it goes out once. Before, every hello made the
/// session export and queue the whole edit again.
#[test]
fn a_large_update_is_sent_once_while_hellos_arrive() {
    let base = base_snapshots();
    let mut a = client_stage(&base);
    let mut b = client_stage(&base);
    let (link, mut a_end, mut b_end) = Link::new(256 << 10, 5);
    let mut sa = SyncSession::new(config("a"));
    let mut sb = SyncSession::new(config("b"));
    let t0 = Instant::now();
    let at = |t: u64| t0 + TICK * u32::try_from(t).unwrap();
    // Connect: both know what the other has.
    for t in 0..3 {
        sa.pump_at(&mut a, &mut a_end, at(t)).unwrap();
        sb.pump_at(&mut b, &mut b_end, at(t)).unwrap();
        link.tick();
    }

    let root = DocKey::sublayer(&SublayerId::root());
    let before = a.version(&root).unwrap();
    let big = letters(8 << 20, 1);
    a.set_edit_target(&SublayerId::root()).unwrap();
    a.transact(&author("user:a"), "paste", |tx| {
        tx.apply(Op::SetValue {
            path: p("/comps/main/layers/title"),
            attr: "notes".into(),
            value: Some(Value::String(big.clone())),
        })
    })
    .unwrap();
    let size = a.export_updates(&root, &before).unwrap().len() as u64;
    let want = a.version(&root).unwrap();
    let sent_before = sa.stats().bytes_sent;

    let mut most = 0;
    let mut arrived = None;
    for t in 3..200 {
        sa.pump_at(&mut a, &mut a_end, at(t)).unwrap();
        most = most.max(a_end.backlog());
        sb.pump_at(&mut b, &mut b_end, at(t)).unwrap();
        link.tick();
        if arrived.is_none() && b.version(&root).is_some_and(|v| v.includes_vv(&want)) {
            arrived = Some(t);
        }
    }
    let arrived = arrived.expect("the edit never arrived");
    let sent = sa.stats().bytes_sent - sent_before;
    eprintln!(
        "{} KiB arrived after {} ticks; {} KiB sent, largest queue {} KiB",
        size >> 10,
        arrived - 3,
        sent >> 10,
        most >> 10
    );
    assert!(sent < size + size / 10, "sent {sent} bytes for {size}");
    assert!(
        most as u64 <= size + (1 << 20),
        "queue reached {most} bytes"
    );
    let got = b
        .prim(&p("/comps/main/layers/title"))
        .and_then(|t| t.value("notes").cloned());
    assert!(got == Some(Value::String(big)), "b got a different value");
}

/// A first sync of 12 documents of 1 MiB each: the sender queues the next one
/// only when the queue is under `SessionConfig::max_backlog`, so it never holds
/// more than one, and sends each once.
#[test]
fn a_first_sync_goes_out_one_document_at_a_time() {
    let mut a = MemDocs::new();
    for i in 0..12 {
        let doc = LoroDoc::new();
        doc.get_text("t").insert(0, &letters(1 << 20, i)).unwrap();
        doc.commit();
        a.load(
            DocKey::parse(&format!("doc{i}")).unwrap(),
            &doc.export(ExportMode::Snapshot).unwrap(),
        )
        .unwrap();
    }
    let total: u64 = a
        .doc_keys()
        .iter()
        .map(|k| {
            a.export_updates(k, &VersionVector::default())
                .unwrap()
                .len() as u64
        })
        .sum();
    let mut b = MemDocs::new();
    let (link, mut a_end, mut b_end) = Link::new(256 << 10, 5);
    let mut sa = SyncSession::new(config("a"));
    let mut sb = SyncSession::new(config("b"));
    let t0 = Instant::now();
    let mut most = 0;
    let mut t = 0u32;
    while !a
        .doc_keys()
        .iter()
        .all(|k| b.version(k).is_some() && b.version(k) == a.version(k))
    {
        assert!(t < 1000, "no convergence");
        sa.pump_at(&mut a, &mut a_end, t0 + TICK * t).unwrap();
        most = most.max(a_end.backlog());
        sb.pump_at(&mut b, &mut b_end, t0 + TICK * t).unwrap();
        link.tick();
        t += 1;
    }
    let sent = sa.stats().bytes_sent;
    eprintln!(
        "{} KiB in {t} ticks; {} KiB sent, largest queue {} KiB",
        total >> 10,
        sent >> 10,
        most >> 10
    );
    assert!(
        most <= SessionConfig::DEFAULT_MAX_BACKLOG + (1 << 20) + (64 << 10),
        "queue reached {most} bytes"
    );
    assert!(sent < total + total / 10, "sent {sent} bytes for {total}");
}

//! The session on a slow link, simulated: messages leave the sender's queue at a
//! fixed rate (`Transport::backlog` reports what is still queued) and arrive a
//! while after their last byte left. A large update is sent once, even though
//! the receiver's `Hello`s, sent before it arrived, keep saying it lacks it; a
//! first sync of many documents goes out one document at a time; an update lost
//! on the way is sent again while the session keeps the queue busy.

mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bae_model::loro::{ExportMode, LoroDoc, VersionVector};
use bae_model::{Op, SublayerId, Value};
use bae_sync::{
    ClientId, DocKey, DocSet, MemDocs, MemoryTransport, Message, PresenceState, Relay,
    SessionConfig, SyncError, SyncSession, Transport,
};
use common::*;

/// One direction of the link.
#[derive(Default)]
struct Pipe {
    /// Messages not fully sent: the tick they were queued in, the message, its
    /// bytes still to go.
    queue: VecDeque<(u64, Message, usize)>,
    /// Sent, arriving at the given tick.
    wire: VecDeque<(u64, Message)>,
    arrived: VecDeque<Message>,
    /// Bytes per tick (`usize::MAX`: no limit).
    rate: usize,
    /// Ticks from a message's last byte leaving to its arrival.
    lag: u64,
    /// Extra ticks a message waits in the queue before it starts to leave (a
    /// busy I/O thread).
    hold: u64,
    /// The next this many `Updates` are lost once they left the queue, without
    /// an error: the link stays up.
    lose: usize,
}

impl Pipe {
    fn tick(&mut self, now: u64) {
        let mut budget = self.rate;
        while budget > 0 {
            let Some((queued, _, left)) = self.queue.front_mut() else {
                break;
            };
            if now <= *queued + self.hold {
                break;
            }
            let n = (*left).min(budget);
            *left -= n;
            budget -= n;
            if *left == 0 {
                let (_, msg, _) = self.queue.pop_front().unwrap();
                if self.lose > 0 && matches!(msg, Message::Updates { .. }) {
                    self.lose -= 1;
                    continue;
                }
                self.wire.push_back((now + self.lag, msg));
            }
        }
        while self.wire.front().is_some_and(|(at, _)| *at <= now) {
            let (_, msg) = self.wire.pop_front().unwrap();
            self.arrived.push_back(msg);
        }
    }

    fn backlog(&self) -> usize {
        self.queue.iter().map(|(_, _, left)| left).sum()
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
        let mut s = self.link.0.lock().unwrap();
        let now = s.now;
        s.pipes[self.me].queue.push_back((now, msg, len));
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
    large_update_is_sent_once(false);
}

/// The same after a small edit of the same document that arrives at once: the
/// receiver's hellos confirm the small edit long before the large one has left
/// the sender's queue, and the large one is still counted as on its way. Its
/// grace for lost data starts when it left, not when the small edit did.
#[test]
fn a_large_update_after_a_small_one_is_sent_once() {
    large_update_is_sent_once(true);
}

fn large_update_is_sent_once(small_first: bool) {
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
    a.set_edit_target(&SublayerId::root()).unwrap();
    let mut start = 3;
    if small_first {
        a.transact(&author("user:a"), "rename", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: "label".into(),
                value: Some(Value::String("small".into())),
            })
        })
        .unwrap();
        sa.pump_at(&mut a, &mut a_end, at(start)).unwrap();
        sb.pump_at(&mut b, &mut b_end, at(start)).unwrap();
        link.tick();
        start += 1;
    }
    let before = a.version(&root).unwrap();
    let big = letters(8 << 20, 1);
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
    for t in start..200 {
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
        arrived - start,
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
/// only when the queue is not over `SessionConfig::max_backlog`, so it holds at
/// most about that plus one document (about 1.75 MiB here, not 12 MiB), and
/// sends each once.
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

/// A small edit is lost on the way while the link stays up. The receiver's
/// `Hello`s keep lacking it; once it left the sender's queue two hello
/// intervals ago it no longer counts as on its way, and the next of those
/// `Hello`s makes the sender send it again.
///
/// The sender's own later messages keep its queue busy, as with
/// `ThreadedTransport`, which counts each message until its I/O thread wrote
/// it: first only its `Hello`s (the queue empties on every tick), then also a
/// presence heartbeat on every pump into a queue that holds each message one
/// extra tick, so the backlog is never zero when the session looks. Neither
/// may put off the repair of the earlier edit. Before, any backlog restarted
/// the clock, and the edit was not sent again while the link stayed up.
#[test]
fn a_lost_update_is_sent_again_while_the_queue_stays_busy() {
    for chatty in [false, true] {
        let base = base_snapshots();
        let mut a = client_stage(&base);
        let mut b = client_stage(&base);
        let (link, mut a_end, mut b_end) = Link::new(usize::MAX, 0);
        let mut ca = config("a");
        if chatty {
            link.0.lock().unwrap().pipes[0].hold = 1;
            ca.presence_heartbeat = TICK;
        }
        let mut sa = SyncSession::new(ca);
        if chatty {
            sa.set_presence(PresenceState {
                name: "a".into(),
                ..PresenceState::default()
            });
        }
        let mut sb = SyncSession::new(config("b"));
        let t0 = Instant::now();
        let at = |t: u64| t0 + TICK * u32::try_from(t).unwrap();
        for t in 0..3 {
            sa.pump_at(&mut a, &mut a_end, at(t)).unwrap();
            sb.pump_at(&mut b, &mut b_end, at(t)).unwrap();
            link.tick();
        }

        let root = DocKey::sublayer(&SublayerId::root());
        link.0.lock().unwrap().pipes[0].lose = 1;
        a.set_edit_target(&SublayerId::root()).unwrap();
        a.transact(&author("user:a"), "rename", |tx| {
            tx.apply(Op::SetValue {
                path: p("/comps/main/layers/title"),
                attr: "notes".into(),
                value: Some(Value::String("lost once".into())),
            })
        })
        .unwrap();
        let want = a.version(&root).unwrap();

        let edit = 3;
        let mut arrived = None;
        let mut busy = 0;
        for t in edit..edit + 100 {
            let a_out = sa.pump_at(&mut a, &mut a_end, at(t)).unwrap();
            busy += usize::from(a_end.backlog() > 0 && a_out.sent > 0);
            sb.pump_at(&mut b, &mut b_end, at(t)).unwrap();
            link.tick();
            if b.version(&root).is_some_and(|v| v.includes_vv(&want)) {
                arrived = Some(t);
                break;
            }
        }
        assert_eq!(link.0.lock().unwrap().pipes[0].lose, 0, "nothing was lost");
        assert!(busy > 1, "the sender's queue was never busy");
        let ticks = arrived.unwrap_or_else(|| {
            panic!("chatty {chatty}: the lost edit was not sent again in 100 ticks")
        }) - edit;
        eprintln!("chatty {chatty}: the lost edit arrived {ticks} ticks after it was made");
        // It leaves within two ticks, is overdue two hello intervals (10
        // ticks) later and is sent again on the receiver's next `Hello` (every
        // 5 ticks), arriving within two more ticks.
        assert!(ticks <= 20, "chatty {chatty}: arrived after {ticks} ticks");
    }
}

/// The same on the hub: the relay's link to one client loses the first update
/// for it, while the relay forwards a large presence heartbeat from another
/// client on every pump into that link's queue. Those forwarded messages wait
/// in the same queue as the session's own; counted as the session's, they do
/// not hold back the repair. Not counted, the lost update looked as if still
/// queued until the session's own later messages outweighed them, about 480
/// ticks here.
#[test]
fn a_lost_update_is_sent_again_while_the_relay_forwards_presence() {
    let doc = DocKey::parse("d").unwrap();
    let mut hub_docs = MemDocs::new();
    {
        let d = LoroDoc::new();
        d.get_text("t").insert(0, "from the hub").unwrap();
        d.commit();
        hub_docs
            .load(doc.clone(), &d.export(ExportMode::Snapshot).unwrap())
            .unwrap();
    }
    let want = hub_docs.version(&doc).unwrap();
    let mut relay = Relay::with_docs(ClientId::new("hub").unwrap(), hub_docs);
    relay.set_hello_interval(Some(Duration::from_millis(500)));
    // End 0 is the relay's: it holds each message one extra tick and loses
    // the first update.
    let (link, hub_end, mut x_end) = Link::new(usize::MAX, 0);
    {
        let mut s = link.0.lock().unwrap();
        s.pipes[0].hold = 1;
        s.pipes[0].lose = 1;
    }
    relay.connect(Box::new(hub_end));
    let (mut y_end, hub_y) = MemoryTransport::pair();
    relay.connect(Box::new(hub_y));
    let mut x = SyncSession::new(config("x"));
    let mut x_docs = MemDocs::new();
    let mut cy = config("y");
    cy.presence_heartbeat = TICK;
    let mut y = SyncSession::new(cy);
    y.set_presence(PresenceState {
        name: "y".repeat(2000),
        ..PresenceState::default()
    });
    let mut y_docs = MemDocs::new();

    let t0 = Instant::now();
    let at = |t: u64| t0 + TICK * u32::try_from(t).unwrap();
    let mut arrived = None;
    for t in 0..1000 {
        x.pump_at(&mut x_docs, &mut x_end, at(t)).unwrap();
        y.pump_at(&mut y_docs, &mut y_end, at(t)).unwrap();
        let out = relay.pump_at(at(t));
        assert!(out.disconnected.is_empty(), "{:?}", out.disconnected);
        link.tick();
        if x_docs.version(&doc).is_some_and(|v| v.includes_vv(&want)) {
            arrived = Some(t);
            break;
        }
    }
    assert_eq!(link.0.lock().unwrap().pipes[0].lose, 0, "nothing was lost");
    assert!(
        x.presence().iter().any(|p| p.state.name.len() == 2000),
        "x never saw y's presence"
    );
    let ticks = arrived.expect("the lost update was not sent again in 1000 ticks");
    eprintln!("the lost update arrived after {ticks} ticks");
    // Lost on the first ticks, overdue two hello intervals (10 ticks) after
    // it left, sent again on x's next `Hello` (every 5 ticks).
    assert!(ticks <= 25, "arrived after {ticks} ticks");
}

//! [`ThreadedTransport`]: runs a blocking transport on its own thread.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::{LinkNotes, Transport, check_size};

type Notify = Box<dyn Fn() + Send>;

/// How far the worker got with what `send` queued, in messages, for
/// [`ThreadedTransport::stalled`].
#[derive(Debug, Default)]
struct Stall {
    /// Messages `send` queued.
    queued: u64,
    /// Messages the worker handed to the inner transport.
    handed: u64,
    /// Since when messages wait without the worker handing any of them over
    /// (`None`: nothing waits).
    since: Option<Instant>,
}

/// What the stage thread hands the worker.
enum Out {
    /// A message and its size as counted in [`Backlog::queued`].
    Msg(Message, usize),
    /// [`Transport::close`] the inner transport once everything before this
    /// was sent.
    Close,
}

/// What waits, for [`Transport::backlog`]: shared by the stage thread and the
/// worker.
#[derive(Debug, Default)]
struct Backlog {
    /// About how many bytes `send` queued that the worker has not handed to
    /// the inner transport yet.
    queued: AtomicUsize,
    /// The inner transport's own backlog, as the worker last saw it.
    inner: AtomicUsize,
    /// [`Transport::backlog`] reported more than 0 since the worker last saw
    /// everything gone: a session may be holding back for it, so the worker
    /// wakes the stage thread once it all left.
    watched: AtomicBool,
    stall: Mutex<Stall>,
    /// The inner transport's [`Transport::notes`] as the worker last copied
    /// them, and how often they changed ([`Transport::notes_seq`]).
    notes: Mutex<LinkNotes>,
    notes_seq: AtomicU64,
}

impl Backlog {
    fn total(&self) -> usize {
        self.queued
            .load(Ordering::Acquire)
            .saturating_add(self.inner.load(Ordering::Acquire))
    }

    fn stall(&self) -> std::sync::MutexGuard<'_, Stall> {
        self.stall.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn notes(&self) -> std::sync::MutexGuard<'_, LinkNotes> {
        self.notes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Copies the inner transport's notes when its [`Transport::notes_seq`]
/// moved: an integer comparison per call of the inner transport.
struct NotesCopier {
    seen: u64,
}

impl NotesCopier {
    fn sync<T: Transport>(&mut self, inner: &T, backlog: &Backlog) {
        let seq = inner.notes_seq();
        if seq != self.seen {
            self.seen = seq;
            *backlog.notes() = inner.notes();
            backlog.notes_seq.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// Moves a transport with slow I/O (the shared folder on a NAS, a socket) to a
/// background thread. `send` and `recv` only touch in-memory queues, so
/// [`crate::SyncSession::pump`] never waits for the disk or the network.
///
/// The worker polls the inner transport every `tick`, and calls `on_receive`
/// after queuing new messages: hook it to "request a repaint" or "pump soon" so the
/// stage thread picks them up. It also calls it once everything sent has left
/// (the queue and the inner transport's [`Transport::backlog`] are empty) if
/// `backlog` reported more than 0 since it last did: a session that held back
/// sends for it ([`crate::SessionConfig::max_backlog`]) goes on. That is at
/// the end of the worker's round that emptied it, or at most a `tick` later.
///
/// [`Transport::backlog`] is what `send` queued that the worker has not handed
/// over yet plus the inner transport's backlog as the worker last saw it.
///
/// A failed `send` of the inner transport surfaces through a later `recv`.
/// `send` itself fails only for a message over the inner transport's
/// [`Transport::max_message_size`] ([`SyncError::MessageTooLarge`]: not
/// queued, the link goes on), and with [`SyncError::Closed`] after `close`
/// or once the worker has ended (`recv` reports why).
/// [`Transport::close`] waits until everything queued was handed to the inner
/// transport, then closes that (which may wait until the messages arrived, see
/// its `close`), and returns the first error that `recv` did not report yet, so
/// a last write that failed is not lost silently. Dropping also waits until
/// everything queued was handed over, but does not close the inner transport
/// (it is dropped) and reports nothing.
///
/// [`stalled`](Self::stalled) tells when the worker is stuck in one call of the
/// inner transport (a NAS that does not answer) while messages wait.
///
/// [`Transport::notes`] are the inner transport's as the worker last saw them:
/// it copies them whenever they changed, after every call of the inner
/// transport and before it hands on a message that call received (so notes
/// from the shared folder's first scan are here once its `Hello` is).
///
/// [`Transport::max_message_size`] is the inner transport's, read once by
/// [`spawn`](Self::spawn). `send` refuses a larger message itself, at once
/// ([`SyncError::MessageTooLarge`]; nothing is queued or counted in the
/// backlog), instead of queuing it for the worker, whose failed send would
/// come back through `recv` as if the link had failed.
pub struct ThreadedTransport {
    tx: Option<Sender<Out>>,
    rx: Receiver<Result<Message, SyncError>>,
    worker: Option<JoinHandle<()>>,
    backlog: Arc<Backlog>,
    max_message: Option<usize>,
}

impl std::fmt::Debug for ThreadedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadedTransport").finish_non_exhaustive()
    }
}

impl ThreadedTransport {
    pub fn spawn<T>(inner: T, tick: Duration, on_receive: impl Fn() + Send + 'static) -> Self
    where
        T: Transport + Send + 'static,
    {
        let max_message = inner.max_message_size();
        let (out_tx, out_rx) = mpsc::channel::<Out>();
        let (in_tx, in_rx) = mpsc::channel();
        let on_receive: Notify = Box::new(on_receive);
        let backlog = Arc::new(Backlog::default());
        let shared = backlog.clone();
        let worker = std::thread::Builder::new()
            .name("bae-sync-io".into())
            .spawn(move || run(inner, tick, out_rx, in_tx, on_receive, &shared))
            .expect("spawn sync I/O thread");
        ThreadedTransport {
            tx: Some(out_tx),
            rx: in_rx,
            worker: Some(worker),
            backlog,
            max_message,
        }
    }

    /// How long messages queued by `send` have been waiting without the
    /// worker handing any of them to the inner transport; zero when nothing
    /// waits. Long means the worker is stuck in one read or write (a NAS or
    /// network that does not answer): what was sent is not on its way.
    pub fn stalled(&self) -> Duration {
        self.backlog
            .stall()
            .since
            .map_or(Duration::ZERO, |t| t.elapsed())
    }

    /// Stop the worker once it handled everything queued (`close`: and closed
    /// the inner transport); the first error it reported that `recv` has not
    /// returned yet.
    fn finish(&mut self, close: bool) -> Result<(), SyncError> {
        // Dropping the sender ends the worker after it sent what was queued.
        if let Some(tx) = self.tx.take()
            && close
        {
            let _ = tx.send(Out::Close);
        }
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let panicked = worker.join().is_err();
        let mut first = None;
        while let Ok(r) = self.rx.try_recv() {
            if let Err(e) = r {
                first.get_or_insert(e);
            }
        }
        match first {
            Some(e) => Err(e),
            None if panicked => Err(SyncError::Closed),
            None => Ok(()),
        }
    }
}

fn run<T: Transport>(
    mut inner: T,
    tick: Duration,
    out_rx: Receiver<Out>,
    in_tx: Sender<Result<Message, SyncError>>,
    on_receive: Notify,
    backlog: &Backlog,
) {
    let mut notes = NotesCopier { seen: 0 };
    notes.sync(&inner, backlog);
    loop {
        // Wait for outgoing messages or the next tick.
        let mut outgoing = Vec::new();
        match out_rx.recv_timeout(tick) {
            Ok(m) => outgoing.push(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        outgoing.extend(out_rx.try_iter());
        for m in outgoing {
            let r = match m {
                Out::Msg(m, len) => {
                    let r = inner.send(m);
                    notes.sync(&inner, backlog);
                    backlog.queued.fetch_sub(len, Ordering::AcqRel);
                    let mut b = backlog.stall();
                    b.handed += 1;
                    b.since = (b.handed < b.queued).then(Instant::now);
                    r
                }
                Out::Close => {
                    if let Err(e) = inner.close() {
                        let _ = in_tx.send(Err(e));
                    }
                    return;
                }
            };
            if let Err(e) = r {
                if in_tx.send(Err(e)).is_err() {
                    return;
                }
                on_receive();
            }
        }
        let mut got = false;
        loop {
            let r = inner.recv();
            notes.sync(&inner, backlog);
            match r {
                Ok(Some(m)) => {
                    got = true;
                    if in_tx.send(Ok(m)).is_err() {
                        return;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    got = true;
                    let closed = matches!(e, SyncError::Closed);
                    if in_tx.send(Err(e)).is_err() || closed {
                        on_receive();
                        return;
                    }
                    break;
                }
            }
        }
        backlog.inner.store(inner.backlog(), Ordering::Release);
        // Whoever saw a backlog may be holding back for it, however short it
        // lasted (one round, the common case: one large message, the rest held
        // back). `backlog` sets the flag after it read the total, so one that
        // raced with this check is seen a round later.
        let drained = backlog.total() == 0 && backlog.watched.swap(false, Ordering::AcqRel);
        if got || drained {
            on_receive();
        }
    }
}

impl Transport for ThreadedTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let tx = self.tx.as_ref().ok_or(SyncError::Closed)?;
        check_size(&msg, self.max_message)?;
        let len = msg.approx_len();
        self.backlog.queued.fetch_add(len, Ordering::AcqRel);
        {
            let mut b = self.backlog.stall();
            if b.handed >= b.queued {
                b.since = Some(Instant::now());
            }
            b.queued += 1;
        }
        tx.send(Out::Msg(msg, len)).map_err(|_| {
            self.backlog.queued.fetch_sub(len, Ordering::AcqRel);
            let mut b = self.backlog.stall();
            b.queued -= 1;
            if b.handed >= b.queued {
                b.since = None;
            }
            SyncError::Closed
        })
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        match self.rx.try_recv() {
            Ok(r) => r.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(SyncError::Closed),
        }
    }

    fn close(&mut self) -> Result<(), SyncError> {
        self.finish(true)
    }

    fn backlog(&self) -> usize {
        let total = self.backlog.total();
        if total > 0 {
            self.backlog.watched.store(true, Ordering::Release);
        }
        total
    }

    fn notes(&self) -> LinkNotes {
        self.backlog.notes().clone()
    }

    fn notes_seq(&self) -> u64 {
        self.backlog.notes_seq.load(Ordering::Acquire)
    }

    fn max_message_size(&self) -> Option<usize> {
        self.max_message
    }
}

impl Drop for ThreadedTransport {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::protocol::DocKey;

    /// Takes 50 ms per message; its own backlog is whatever the test sets.
    struct Slow {
        backlog: Arc<AtomicUsize>,
    }

    impl Transport for Slow {
        fn send(&mut self, _: Message) -> Result<(), SyncError> {
            std::thread::sleep(Duration::from_millis(50));
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            Ok(None)
        }

        fn backlog(&self) -> usize {
            self.backlog.load(Ordering::Acquire)
        }
    }

    fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn the_backlog_is_what_waits_in_the_queue_and_below() {
        let inner = Arc::new(AtomicUsize::new(0));
        let wakes = Arc::new(AtomicUsize::new(0));
        let woken = wakes.clone();
        let mut t = ThreadedTransport::spawn(
            Slow {
                backlog: inner.clone(),
            },
            Duration::from_millis(1),
            move || {
                woken.fetch_add(1, Ordering::AcqRel);
            },
        );
        assert_eq!(t.backlog(), 0);
        let msg = Message::Updates {
            doc: DocKey::parse("root").unwrap(),
            version: Default::default(),
            data: vec![0; 100_000],
        };
        for _ in 0..5 {
            t.send(msg.clone()).unwrap();
        }
        // Counted until the inner transport took them.
        assert!(t.backlog() >= 4 * 100_000, "{}", t.backlog());
        wait_until("the queue to drain", || t.backlog() == 0);
        // Let the worker finish that round (it may wake us for it).
        std::thread::sleep(Duration::from_millis(50));
        // What waits in the inner transport counts too, and once that drained
        // (after `backlog` reported it), the stage thread is woken so a
        // session that held back sends goes on.
        let before = wakes.load(Ordering::Acquire);
        inner.store(12_345, Ordering::Release);
        wait_until("the inner backlog", || t.backlog() == 12_345);
        assert_eq!(wakes.load(Ordering::Acquire), before);
        inner.store(0, Ordering::Release);
        wait_until("the wake-up", || wakes.load(Ordering::Acquire) > before);
        assert_eq!(t.backlog(), 0);
        t.close().unwrap();
    }

    /// Hands out one `Hello` (a peer that has nothing), takes 30 ms per
    /// message, never has a backlog of its own (a shared folder): records the
    /// documents of the data it was sent.
    struct Folder {
        hello: Option<Message>,
        data: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Transport for Folder {
        fn send(&mut self, msg: Message) -> Result<(), SyncError> {
            std::thread::sleep(Duration::from_millis(30));
            if let Message::Updates { doc, .. } = msg {
                self.data.lock().unwrap().push(doc.to_string());
            }
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            Ok(self.hello.take())
        }
    }

    /// A session that held back documents for the queue (`max_backlog`) and
    /// is pumped only when woken goes on once the queue drained, also when
    /// the worker took everything in one round, so the backlog never lasted
    /// a whole round (the common case: one large document, then the rest
    /// held back). The rest used to wait for some unrelated pump.
    #[test]
    fn a_session_held_back_by_the_queue_is_woken_when_it_drained() {
        use crate::{ClientId, MemDocs, SessionConfig, SyncSession};
        use bae_model::loro::{ExportMode, LoroDoc};

        let mut docs = MemDocs::new();
        let names = ["a", "b", "c"];
        for (i, name) in names.into_iter().enumerate() {
            let d = LoroDoc::new();
            let value: String = (0..4000)
                .map(|j| char::from(b'a' + ((i * 7 + j * 13) % 26) as u8))
                .collect();
            d.get_map("m").insert("v", value).unwrap();
            d.commit();
            docs.load(
                DocKey::parse(name).unwrap(),
                &d.export(ExportMode::Snapshot).unwrap(),
            )
            .unwrap();
        }
        let data = Arc::new(std::sync::Mutex::new(Vec::new()));
        let folder = Folder {
            hello: Some(Message::hello(ClientId::new("folder").unwrap(), Vec::new())),
            data: data.clone(),
        };
        let (wake, woken) = mpsc::channel();
        let mut t = ThreadedTransport::spawn(folder, Duration::from_millis(1), move || {
            let _ = wake.send(());
        });
        let mut config = SessionConfig::new(ClientId::new("app").unwrap());
        config.hello_interval = None;
        config.max_backlog = Some(1000);
        let mut session = SyncSession::new(config);
        session.pump(&mut docs, &mut t).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        // Pumped only when woken.
        while data.lock().unwrap().len() < names.len() {
            match woken.recv_timeout(Duration::from_millis(10)) {
                Ok(()) => {
                    session.pump(&mut docs, &mut t).unwrap();
                }
                Err(_) => assert!(
                    Instant::now() < deadline,
                    "not woken; sent {:?}, held back {} times",
                    data.lock().unwrap(),
                    session.stats().held_back
                ),
            }
        }
        assert!(session.stats().held_back >= 2, "{:?}", session.stats());
        t.close().unwrap();
        assert_eq!(data.lock().unwrap().len(), names.len());
    }

    /// Takes messages up to 1000 bytes and fails larger ones, as a hub link
    /// does; records what it got.
    struct Capped {
        got: Arc<std::sync::Mutex<Vec<usize>>>,
    }

    impl Transport for Capped {
        fn send(&mut self, msg: Message) -> Result<(), SyncError> {
            check_size(&msg, self.max_message_size())?;
            self.got.lock().unwrap().push(msg.encoded_len());
            Ok(())
        }

        fn recv(&mut self) -> Result<Option<Message>, SyncError> {
            Ok(None)
        }

        fn max_message_size(&self) -> Option<usize> {
            Some(1000)
        }
    }

    /// A message over the inner transport's limit is refused by `send` at
    /// once and nothing is queued: it used to be queued, fail on the worker
    /// and come back through `recv` as a link error, so the link was dropped
    /// and, after the reconnect, the same message failed again.
    #[test]
    fn a_message_over_the_limit_fails_at_once_and_the_link_goes_on() {
        let got = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut t =
            ThreadedTransport::spawn(Capped { got: got.clone() }, Duration::from_millis(1), || {});
        assert_eq!(t.max_message_size(), Some(1000));
        let msg = |len| Message::Updates {
            doc: DocKey::parse("root").unwrap(),
            version: Default::default(),
            data: vec![0; len],
        };
        let big = msg(2000);
        let size = big.encoded_len();
        match t.send(big) {
            Err(SyncError::MessageTooLarge { doc, size: s, max }) => {
                assert_eq!(doc, Some(DocKey::parse("root").unwrap()));
                assert_eq!((s, max), (size, 1000));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(t.backlog(), 0);
        assert_eq!(t.stalled(), Duration::ZERO);
        // The link goes on.
        t.send(msg(500)).unwrap();
        wait_until("the small message", || got.lock().unwrap().len() == 1);
        std::thread::sleep(Duration::from_millis(20));
        assert!(t.recv().unwrap().is_none());
        t.close().unwrap();
        assert_eq!(got.lock().unwrap().len(), 1);
    }
}

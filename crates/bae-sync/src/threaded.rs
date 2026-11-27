//! [`ThreadedTransport`]: runs a blocking transport on its own thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

type Notify = Box<dyn Fn() + Send>;

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
}

impl Backlog {
    fn total(&self) -> usize {
        self.queued
            .load(Ordering::Acquire)
            .saturating_add(self.inner.load(Ordering::Acquire))
    }
}

/// Moves a transport with slow I/O (the shared folder on a NAS, a socket) to a
/// background thread. `send` and `recv` only touch in-memory queues, so
/// [`crate::SyncSession::pump`] never waits for the disk or the network.
///
/// The worker polls the inner transport every `tick`, and calls `on_receive`
/// after queuing new messages: hook it to "request a repaint" or "pump soon" so the
/// stage thread picks them up. It also calls it when what was sent had to wait
/// (it was still queued, or in the inner transport's [`Transport::backlog`],
/// after a whole round of the worker) and has now all left: a session that held
/// back sends for it ([`crate::SessionConfig::max_backlog`]) goes on.
///
/// [`Transport::backlog`] is what `send` queued that the worker has not handed
/// over yet plus the inner transport's backlog as the worker last saw it.
///
/// A failed `send` of the inner transport surfaces through a later `recv`.
/// [`Transport::close`] waits until everything queued was handed to the inner
/// transport, then closes that (which may wait until the messages arrived, see
/// its `close`), and returns the first error that `recv` did not report yet, so
/// a last write that failed is not lost silently. Dropping also waits until
/// everything queued was handed over, but does not close the inner transport
/// (it is dropped) and reports nothing.
pub struct ThreadedTransport {
    tx: Option<Sender<Out>>,
    rx: Receiver<Result<Message, SyncError>>,
    worker: Option<JoinHandle<()>>,
    backlog: Arc<Backlog>,
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
        }
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
    // Something waited at the end of the last round.
    let mut waited = false;
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
                    backlog.queued.fetch_sub(len, Ordering::AcqRel);
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
            match inner.recv() {
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
        let waits = backlog.total() > 0;
        if got || (waited && !waits) {
            on_receive();
        }
        waited = waits;
    }
}

impl Transport for ThreadedTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let tx = self.tx.as_ref().ok_or(SyncError::Closed)?;
        let len = msg.approx_len();
        self.backlog.queued.fetch_add(len, Ordering::AcqRel);
        tx.send(Out::Msg(msg, len)).map_err(|_| {
            self.backlog.queued.fetch_sub(len, Ordering::AcqRel);
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
        self.backlog.total()
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
        // after waiting a whole round, the stage thread is woken so a session
        // that held back sends goes on.
        let before = wakes.load(Ordering::Acquire);
        inner.store(12_345, Ordering::Release);
        wait_until("the inner backlog", || t.backlog() == 12_345);
        assert_eq!(wakes.load(Ordering::Acquire), before);
        inner.store(0, Ordering::Release);
        wait_until("the wake-up", || wakes.load(Ordering::Acquire) > before);
        assert_eq!(t.backlog(), 0);
        t.close().unwrap();
    }
}

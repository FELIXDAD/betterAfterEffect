//! [`ThreadedTransport`]: runs a blocking transport on its own thread.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

type Notify = Box<dyn Fn() + Send>;

/// How far the worker got with what `send` queued.
#[derive(Debug, Default)]
struct Backlog {
    /// Messages `send` queued.
    queued: u64,
    /// Messages the worker handed to the inner transport.
    handed: u64,
    /// Since when messages wait without the worker handing any of them over
    /// (`None`: nothing waits).
    since: Option<Instant>,
}

fn lock(b: &Mutex<Backlog>) -> std::sync::MutexGuard<'_, Backlog> {
    b.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the stage thread hands the worker.
enum Out {
    Msg(Message),
    /// [`Transport::close`] the inner transport once everything before this
    /// was sent.
    Close,
}

/// Moves a transport with slow I/O (the shared folder on a NAS, a socket) to a
/// background thread. `send` and `recv` only touch in-memory queues, so
/// [`crate::SyncSession::pump`] never waits for the disk or the network.
///
/// The worker polls the inner transport every `tick`, and calls `on_receive`
/// after queuing new messages: hook it to "request a repaint" or "pump soon" so the
/// stage thread picks them up.
///
/// A failed `send` of the inner transport surfaces through a later `recv`.
/// [`Transport::close`] waits until everything queued was handed to the inner
/// transport, then closes that (which may wait until the messages arrived, see
/// its `close`), and returns the first error that `recv` did not report yet, so
/// a last write that failed is not lost silently. Dropping also waits until
/// everything queued was handed over, but does not close the inner transport
/// (it is dropped) and reports nothing.
///
/// [`stalled`](Self::stalled) tells when the worker is stuck in one call of the
/// inner transport (a NAS that does not answer) while messages wait.
pub struct ThreadedTransport {
    tx: Option<Sender<Out>>,
    rx: Receiver<Result<Message, SyncError>>,
    worker: Option<JoinHandle<()>>,
    backlog: Arc<Mutex<Backlog>>,
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
        let backlog = Arc::new(Mutex::new(Backlog::default()));
        let handed = backlog.clone();
        let worker = std::thread::Builder::new()
            .name("bae-sync-io".into())
            .spawn(move || run(inner, tick, out_rx, in_tx, on_receive, &handed))
            .expect("spawn sync I/O thread");
        ThreadedTransport {
            tx: Some(out_tx),
            rx: in_rx,
            worker: Some(worker),
            backlog,
        }
    }

    /// How long messages queued by `send` have been waiting without the
    /// worker handing any of them to the inner transport; zero when nothing
    /// waits. Long means the worker is stuck in one read or write (a NAS or
    /// network that does not answer): what was sent is not on its way.
    pub fn stalled(&self) -> Duration {
        lock(&self.backlog)
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
    backlog: &Mutex<Backlog>,
) {
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
                Out::Msg(m) => {
                    let r = inner.send(m);
                    let mut b = lock(backlog);
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
        if got {
            on_receive();
        }
    }
}

impl Transport for ThreadedTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let tx = self.tx.as_ref().ok_or(SyncError::Closed)?;
        {
            let mut b = lock(&self.backlog);
            if b.handed >= b.queued {
                b.since = Some(Instant::now());
            }
            b.queued += 1;
        }
        tx.send(Out::Msg(msg)).map_err(|_| {
            let mut b = lock(&self.backlog);
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
}

impl Drop for ThreadedTransport {
    fn drop(&mut self) {
        let _ = self.finish(false);
    }
}

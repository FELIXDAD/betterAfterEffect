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

/// Moves a transport with slow I/O (the shared folder on a NAS, a socket) to a
/// background thread. `send` and `recv` only touch in-memory queues, so
/// [`crate::SyncSession::pump`] never waits for the disk or the network.
///
/// The worker polls the inner transport every `tick`, and calls `on_receive`
/// after queuing new messages: hook it to "request a repaint" or "pump soon" so the
/// stage thread picks them up.
///
/// [`Transport::backlog`] counts what waits for the worker plus the inner
/// transport's own backlog, so the session's backpressure sees through the queue.
/// A message over the inner transport's [`Transport::max_message_size`] is
/// refused by `send` right away, as the inner transport would. Other errors of
/// the inner `send` come back from `recv`, in order with the messages; the
/// session skips the document of a [`SyncError::NewerLogVersion`] and keeps
/// the link.
pub struct ThreadedTransport {
    /// Messages for the worker, with their encoded size.
    tx: Option<Sender<(Message, usize)>>,
    rx: Receiver<Result<Message, SyncError>>,
    backlog: Arc<Backlog>,
    max_message_size: Option<usize>,
    worker: Option<JoinHandle<()>>,
}

/// What is sent but not written yet, shared with the worker.
#[derive(Debug, Default)]
struct Backlog {
    /// Encoded bytes of the messages in the channel to the worker.
    queued: AtomicUsize,
    /// The inner transport's backlog, as of the worker's last call.
    inner: AtomicUsize,
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
        let (out_tx, out_rx) = mpsc::channel::<(Message, usize)>();
        let (in_tx, in_rx) = mpsc::channel();
        let on_receive: Notify = Box::new(on_receive);
        let backlog = Arc::new(Backlog::default());
        let shared = backlog.clone();
        let max_message_size = inner.max_message_size();
        let worker = std::thread::Builder::new()
            .name("bae-sync-io".into())
            .spawn(move || run(inner, tick, out_rx, in_tx, on_receive, &shared))
            .expect("spawn sync I/O thread");
        ThreadedTransport {
            tx: Some(out_tx),
            rx: in_rx,
            backlog,
            max_message_size,
            worker: Some(worker),
        }
    }
}

fn run<T: Transport>(
    mut inner: T,
    tick: Duration,
    out_rx: Receiver<(Message, usize)>,
    in_tx: Sender<Result<Message, SyncError>>,
    on_receive: Notify,
    backlog: &Backlog,
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
        for (m, len) in outgoing {
            let sent = inner.send(m);
            // Counted by the inner transport before it leaves the channel's
            // count: the total never dips below what is really waiting.
            backlog.inner.store(inner.backlog(), Ordering::Relaxed);
            backlog.queued.fetch_sub(len, Ordering::Relaxed);
            if let Err(e) = sent {
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
        // `recv` also writes out what the inner transport still holds.
        backlog.inner.store(inner.backlog(), Ordering::Relaxed);
        if got {
            on_receive();
        }
    }
}

impl Transport for ThreadedTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        let tx = self.tx.as_ref().ok_or(SyncError::Closed)?;
        let len = msg.encoded_len();
        if let Some(max) = self.max_message_size.filter(|&max| len > max) {
            return Err(SyncError::MessageTooLarge { size: len, max });
        }
        self.backlog.queued.fetch_add(len, Ordering::Relaxed);
        tx.send((msg, len)).map_err(|_| {
            self.backlog.queued.fetch_sub(len, Ordering::Relaxed);
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

    fn backlog(&self) -> usize {
        self.backlog
            .queued
            .load(Ordering::Relaxed)
            .saturating_add(self.backlog.inner.load(Ordering::Relaxed))
    }

    fn max_message_size(&self) -> Option<usize> {
        self.max_message_size
    }
}

impl Drop for ThreadedTransport {
    fn drop(&mut self) {
        // Dropping the sender ends the worker after it flushed what was queued.
        self.tx = None;
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

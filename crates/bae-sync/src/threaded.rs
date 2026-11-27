//! [`ThreadedTransport`]: runs a blocking transport on its own thread.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::error::SyncError;
use crate::protocol::Message;
use crate::transport::Transport;

type Notify = Box<dyn Fn() + Send>;

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
pub struct ThreadedTransport {
    tx: Option<Sender<Out>>,
    rx: Receiver<Result<Message, SyncError>>,
    worker: Option<JoinHandle<()>>,
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
        let worker = std::thread::Builder::new()
            .name("bae-sync-io".into())
            .spawn(move || run(inner, tick, out_rx, in_tx, on_receive))
            .expect("spawn sync I/O thread");
        ThreadedTransport {
            tx: Some(out_tx),
            rx: in_rx,
            worker: Some(worker),
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
                Out::Msg(m) => inner.send(m),
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
        self.tx
            .as_ref()
            .ok_or(SyncError::Closed)?
            .send(Out::Msg(msg))
            .map_err(|_| SyncError::Closed)
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

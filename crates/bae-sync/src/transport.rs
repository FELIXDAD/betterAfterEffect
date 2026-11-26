//! [`Transport`]: how protocol messages reach the other side.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::error::SyncError;
use crate::protocol::Message;

/// Moves [`Message`]s between this client and one remote (the shared folder, the
/// hub, or a peer). Both calls must not block: [`crate::SyncSession::pump`] runs on
/// the thread that owns the stage. Transports with slow I/O (SMB, network) do it on
/// a background thread and hand messages over through queues.
///
/// Delivery may be lossy, duplicated or reordered; the session repairs that with
/// periodic `Hello`s. Problems a transport can recover from itself (a corrupt log
/// record, a dropped frame) should not be returned as errors; an error means the
/// link is unusable until the caller reconnects, except
/// [`SyncError::MessageTooLarge`] from `send`, which refuses only that message,
/// and [`SyncError::NewerLogVersion`] (from `send`, or from `recv` when the write
/// happens in the background), which refuses only that document.
pub trait Transport {
    /// Queue `msg` for the remote.
    fn send(&mut self, msg: Message) -> Result<(), SyncError>;
    /// The next received message, or `None` if nothing is waiting.
    fn recv(&mut self) -> Result<Option<Message>, SyncError>;
    /// Bytes accepted by `send` that have not left this process yet (roughly),
    /// counted as [`Message::encoded_len`]; messages leave in the order they
    /// were sent. The session sends no more local changes while this is over
    /// [`crate::SessionConfig::max_backlog`], so a slow link gets one large
    /// message at a time instead of all of them at once. It also tells the
    /// session when a message has left: everything it handed over minus this.
    /// Counting more than that only makes the session wait longer before it
    /// takes unconfirmed data for lost. Default: 0 (`send` writes everything
    /// before it returns).
    fn backlog(&self) -> usize {
        0
    }
    /// Largest encoded message `send` accepts ([`Message::encoded_len`]); a
    /// larger one fails with [`SyncError::MessageTooLarge`] and nothing is sent.
    /// `None`: no limit.
    fn max_message_size(&self) -> Option<usize> {
        None
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        (**self).send(msg)
    }
    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        (**self).recv()
    }
    fn backlog(&self) -> usize {
        (**self).backlog()
    }
    fn max_message_size(&self) -> Option<usize> {
        (**self).max_message_size()
    }
}

/// One end of an in-process, lossless link (tests, in-process agents).
#[derive(Debug)]
pub struct MemoryTransport {
    tx: Sender<Message>,
    rx: Receiver<Message>,
}

impl MemoryTransport {
    /// Two connected ends.
    pub fn pair() -> (MemoryTransport, MemoryTransport) {
        let (a_tx, b_rx) = mpsc::channel();
        let (b_tx, a_rx) = mpsc::channel();
        (
            MemoryTransport { tx: a_tx, rx: a_rx },
            MemoryTransport { tx: b_tx, rx: b_rx },
        )
    }
}

impl Transport for MemoryTransport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        self.tx.send(msg).map_err(|_| SyncError::Closed)
    }

    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        match self.rx.try_recv() {
            Ok(m) => Ok(Some(m)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(SyncError::Closed),
        }
    }
}

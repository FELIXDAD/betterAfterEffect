//! [`Transport`]: how protocol messages reach the other side.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::error::SyncError;
use crate::protocol::Message;

/// What this client may do with the project, as the remote grants it (a hub
/// token's rights, a shared folder's permissions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Access {
    /// Not known: never connected yet, or a hub that does not say. Edits are
    /// sent (the remote decides).
    #[default]
    Unknown,
    /// The remote does not take our edits (a read-only hub token, a shared
    /// folder this user cannot write). Grey out editing; local edits are not
    /// sent.
    Read,
    /// Edits are accepted.
    Write,
}

impl Access {
    /// `unknown`, `read` or `write`.
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Unknown => "unknown",
            Access::Read => "read",
            Access::Write => "write",
        }
    }

    /// `read` / `write` (any letter case); anything else is
    /// [`Access::Unknown`].
    pub fn parse(s: &str) -> Access {
        if s.trim().eq_ignore_ascii_case("read") {
            Access::Read
        } else if s.trim().eq_ignore_ascii_case("write") {
            Access::Write
        } else {
            Access::Unknown
        }
    }

    /// Whether local edits are sent: everything but [`Access::Read`].
    pub fn can_write(self) -> bool {
        self != Access::Read
    }
}

/// Moves [`Message`]s between this client and one remote (the shared folder, the
/// hub, or a peer). Both calls must not block: [`crate::SyncSession::pump`] runs on
/// the thread that owns the stage. Transports with slow I/O (SMB, network) do it on
/// a background thread and hand messages over through queues.
///
/// Delivery may be lossy, duplicated or reordered; the session repairs that with
/// periodic `Hello`s. Problems a transport can recover from itself (a corrupt log
/// record, a dropped frame) should not be returned as errors; an error means the
/// link is unusable until the caller reconnects.
pub trait Transport {
    fn send(&mut self, msg: Message) -> Result<(), SyncError>;
    /// The next received message, or `None` if nothing is waiting.
    fn recv(&mut self) -> Result<Option<Message>, SyncError>;
    /// Finish delivering what was sent, then close the link: the place to see
    /// whether the last messages arrived when `send` only queues them (as in
    /// [`crate::ThreadedTransport`], whose thread writes them later, or a hub
    /// connection, whose socket takes them as it can). Returns the first error
    /// the transport hit that [`recv`](Self::recv) has not returned yet. May
    /// block until the I/O is done; the transport is unusable after.
    ///
    /// The default does nothing: right for transports whose `send` finishes
    /// its I/O before returning.
    fn close(&mut self) -> Result<(), SyncError> {
        Ok(())
    }
    /// About how many bytes were handed to [`send`](Self::send) and have not
    /// left yet: they wait in a write buffer (a hub connection whose peer reads
    /// slowly) or a queue ([`crate::ThreadedTransport`], whose thread has not
    /// written them yet). A session sends no document data while this is over
    /// [`crate::SessionConfig::max_backlog`] and goes on once it drained, so a
    /// slow link is not handed the same data again and again (flow control).
    ///
    /// The default, 0, is right for transports whose `send` finishes its I/O
    /// before returning.
    fn backlog(&self) -> usize {
        0
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn send(&mut self, msg: Message) -> Result<(), SyncError> {
        (**self).send(msg)
    }
    fn recv(&mut self) -> Result<Option<Message>, SyncError> {
        (**self).recv()
    }
    fn close(&mut self) -> Result<(), SyncError> {
        (**self).close()
    }
    fn backlog(&self) -> usize {
        (**self).backlog()
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

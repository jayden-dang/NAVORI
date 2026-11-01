//! Watch a process until it exits.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use navori_macos::{ExitSink, ExitSource, PlatformError, ProcessSource, ReadError, WatchError};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::error::ProcsError;
use crate::snapshot::ProcId;

/// One watched identity has exited. The pid may already belong to someone else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exited {
    /// The identity that was watched.
    pub id: ProcId,
}

/// Delivers one [`Exited`] per watched identity.
///
/// A pid that is reused, or an exit that arrives after the caller stops watching,
/// does not produce a second event. `Debug` omits the exit source.
pub struct ProcWatcher {
    source: Arc<dyn ProcessSource>,
    exits: Box<dyn ExitSource>,
    watched: HashMap<i32, (ProcId, u64)>,
    next_token: u64,
    pending: VecDeque<Exited>,
    events: UnboundedReceiver<(i32, u64)>,
}

impl fmt::Debug for ProcWatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProcWatcher")
            .field("watched", &self.watched)
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl ProcWatcher {
    /// Start watching through `exits`. The closure receives the sink the source calls on exit.
    ///
    /// # Errors
    ///
    /// [`ProcsError::Platform`] when the exit source cannot start.
    pub fn new(
        source: Arc<dyn ProcessSource>,
        exits: impl FnOnce(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError>,
    ) -> Result<Self, ProcsError> {
        let (tx, events) = tokio::sync::mpsc::unbounded_channel();
        let sink: ExitSink = Box::new(move |pid, token| {
            let _ = tx.send((pid, token));
        });
        let exits = exits(sink).map_err(|source| ProcsError::Platform { source })?;
        Ok(Self {
            source,
            exits,
            watched: HashMap::new(),
            next_token: 1,
            pending: VecDeque::new(),
            events,
        })
    }

    /// Watch `id` until that process exits.
    ///
    /// A process that is already gone, or whose pid was reused before registration sticks,
    /// is queued for [`Self::next`] and is not an error. A kqueue failure leaves nothing watched.
    ///
    /// # Errors
    ///
    /// [`ProcsError::Watch`] when kqueue rejects the registration. The entry is removed.
    pub fn watch(&mut self, id: ProcId) -> Result<(), ProcsError> {
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1);
        self.watched.insert(id.pid, (id, token));
        match self.exits.watch(id.pid, token) {
            Ok(()) => {}
            Err(WatchError::Gone) => {
                self.watched.remove(&id.pid);
                self.pending.push_back(Exited { id });
                return Ok(());
            }
            Err(WatchError::Kqueue { errno }) => {
                self.watched.remove(&id.pid);
                return Err(ProcsError::Watch { errno });
            }
        }
        match self.source.entry(id.pid) {
            Ok(entry) if entry.start == id.start => Ok(()),
            Ok(_) | Err(ReadError::Gone) => {
                self.watched.remove(&id.pid);
                self.exits.unwatch(id.pid);
                self.pending.push_back(Exited { id });
                Ok(())
            }
            Err(ReadError::Denied | ReadError::Malformed) => Ok(()),
        }
    }

    /// Stop delivering exits for `id`. A late exit for its token is ignored.
    pub fn unwatch(&mut self, id: ProcId) {
        let watched = self
            .watched
            .get(&id.pid)
            .is_some_and(|(watched, _)| *watched == id);
        if !watched {
            return;
        }
        self.watched.remove(&id.pid);
        self.exits.unwatch(id.pid);
    }

    /// The next exit for an identity that is still watched.
    ///
    /// Cancel-safe: a queued exit stays queued if this future is dropped before it completes.
    /// `None` means the exit source has shut down.
    pub async fn next(&mut self) -> Option<Exited> {
        loop {
            if let Some(exited) = self.pending.pop_front() {
                return Some(exited);
            }
            let (pid, token) = self.events.recv().await?;
            let matched = self
                .watched
                .get(&pid)
                .copied()
                .filter(|(_, watched)| *watched == token);
            if let Some((id, _)) = matched {
                self.watched.remove(&pid);
                return Some(Exited { id });
            }
        }
    }
}

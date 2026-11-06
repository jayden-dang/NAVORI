//! In-memory power source.

use std::io;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::{PowerKind, PowerSource, PowerSourceError, PowerWatch};

struct State {
    kind: PowerKind,
    fail_current: bool,
    fail_watch: bool,
    writer: Option<UnixStream>,
}

/// Scripted [`PowerSource`](crate::PowerSource) that never calls the system.
#[derive(Clone)]
pub struct FakePowerSource {
    state: Arc<Mutex<State>>,
    reads: Arc<AtomicUsize>,
    watches: Arc<AtomicUsize>,
    drains: Arc<AtomicUsize>,
}

impl std::fmt::Debug for FakePowerSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakePowerSource").finish_non_exhaustive()
    }
}

impl FakePowerSource {
    /// `kind` is providing power, and reads and watches succeed.
    pub fn new(kind: PowerKind) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                kind,
                fail_current: false,
                fail_watch: false,
                writer: None,
            })),
            reads: Arc::default(),
            watches: Arc::default(),
            drains: Arc::default(),
        }
    }

    /// `kind` is what [`PowerSource::current`] returns from now on.
    pub fn set(&self, kind: PowerKind) {
        lock(&self.state).kind = kind;
    }

    /// Later reads fail.
    pub fn fail_current(&self) {
        lock(&self.state).fail_current = true;
    }

    /// Later watches fail.
    pub fn fail_watch(&self) {
        lock(&self.state).fail_watch = true;
    }

    /// Make the latest watch readable, as a power-source change does.
    pub fn fire(&self) {
        if let Some(writer) = lock(&self.state).writer.as_mut() {
            let _ = writer.write_all(&[0; 4]);
        }
    }

    /// Close the latest watch, so it reads zero.
    pub fn kill(&self) {
        lock(&self.state).writer = None;
    }

    /// How many times [`PowerSource::current`] was called.
    pub fn read_count(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    /// How many times [`PowerSource::watch`] was called.
    pub fn watch_count(&self) -> usize {
        self.watches.load(Ordering::SeqCst)
    }

    /// How many times `read_tokens` was called on any watch this fake returned.
    pub fn drain_count(&self) -> usize {
        self.drains.load(Ordering::SeqCst)
    }
}

impl PowerSource for FakePowerSource {
    fn current(&self) -> Result<PowerKind, PowerSourceError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let state = lock(&self.state);
        if state.fail_current {
            Err(PowerSourceError::Unavailable)
        } else {
            Ok(state.kind)
        }
    }

    fn watch(&self) -> Result<PowerWatch, PowerSourceError> {
        self.watches.fetch_add(1, Ordering::SeqCst);
        let mut state = lock(&self.state);
        if state.fail_watch {
            return Err(PowerSourceError::Register { status: 1 });
        }
        let (writer, reader) = UnixStream::pair().map_err(nonblocking)?;
        let watch =
            PowerWatch::from_stream(reader, Arc::clone(&self.drains)).map_err(nonblocking)?;
        state.writer = Some(writer);
        Ok(watch)
    }
}

fn nonblocking(err: io::Error) -> PowerSourceError {
    PowerSourceError::Nonblocking {
        errno: err.raw_os_error().unwrap_or(0),
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

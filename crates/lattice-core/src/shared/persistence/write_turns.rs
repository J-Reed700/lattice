//! The queue every unit of work that writes waits in.
//!
//! SQLite lets one connection write at a time. A writer that finds the lock
//! taken is left to SQLite's busy handler, which sleeps and retries with no
//! queue: a steady run of short writes can keep it out until its busy timeout
//! fails it. Here writers wait first come, first served, and a waiting writer
//! holds no connection, so readers never wait behind writers.
use std::panic::Location;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::error::PersistenceError;

/// A unit that holds the turn this long delays a click enough to notice (a
/// tenth of a second is the limit for a response to feel immediate), so the
/// log names where the unit began.
const NOTICEABLE_HOLD: Duration = Duration::from_millis(100);

/// How often a writer still waiting names the unit ahead of it in the log. It
/// keeps waiting: this only puts a hang in the log.
const STILL_WAITING: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(super) struct WriteTurns {
    queue: Arc<Semaphore>,
    holder: Mutex<Option<Holder>>,
}

#[derive(Debug, Clone, Copy)]
struct Holder {
    at: &'static Location<'static>,
    since: Instant,
}

/// The turn a unit of work holds from its begin until it commits, rolls back
/// or is dropped. Dropping it lets the next writer in.
#[derive(Debug)]
pub(super) struct WriteTurn {
    _permit: OwnedSemaphorePermit,
    turns: Arc<WriteTurns>,
    holder: Holder,
}

impl WriteTurns {
    pub(super) fn new() -> Self {
        Self {
            queue: Arc::new(Semaphore::new(1)),
            holder: Mutex::new(None),
        }
    }

    /// Waits behind every writer that asked earlier, then holds the turn.
    /// `at` is where the unit of work began, for the log.
    pub(super) async fn take(
        self: &Arc<Self>,
        at: &'static Location<'static>,
    ) -> Result<WriteTurn, PersistenceError> {
        let asked = Instant::now();
        let mut acquire = std::pin::pin!(Arc::clone(&self.queue).acquire_owned());
        let permit = loop {
            match tokio::time::timeout(STILL_WAITING, acquire.as_mut()).await {
                Ok(permit) => break permit.map_err(|_closed| PersistenceError::closed())?,
                Err(_still_waiting) => self.report_wait(at, asked),
            }
        };
        let holder = Holder {
            at,
            since: Instant::now(),
        };
        *self.holder.lock().unwrap_or_else(PoisonError::into_inner) = Some(holder);
        Ok(WriteTurn {
            _permit: permit,
            turns: Arc::clone(self),
            holder,
        })
    }

    /// Ends every writer still waiting with `Closed`. The unit holding the
    /// turn carries on until it commits or is dropped.
    pub(super) fn close(&self) {
        self.queue.close();
    }

    fn report_wait(&self, at: &'static Location<'static>, asked: Instant) {
        let ahead = *self.holder.lock().unwrap_or_else(PoisonError::into_inner);
        match ahead {
            Some(ahead) => tracing::warn!(
                waiting_at = %at,
                waited = ?asked.elapsed(),
                holder_at = %ahead.at,
                held = ?ahead.since.elapsed(),
                "a write is still waiting for its turn"
            ),
            None => tracing::warn!(
                waiting_at = %at,
                waited = ?asked.elapsed(),
                "a write is still waiting for its turn"
            ),
        }
    }
}

impl Drop for WriteTurn {
    fn drop(&mut self) {
        // The permit is released after this body, so the holder cleared here
        // is still this unit's.
        *self
            .turns
            .holder
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        let held = self.holder.since.elapsed();
        if held >= NOTICEABLE_HOLD {
            tracing::warn!(
                at = %self.holder.at,
                held = ?held,
                "a write held the database long enough to notice"
            );
        }
    }
}

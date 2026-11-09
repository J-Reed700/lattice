//! Whether a job's model calls are waiting for a slot, for the schedulers
//! that serve them.
//!
//! A job's handler may run inside an observer scope, held in a task-local like
//! the job's rank, so every call it makes can reach the observer with
//! [`current_observer`]. A scheduler tells it when a request had to queue and
//! how its wait ended; it knows nothing of what the job is.
use std::future::Future;
use std::sync::Arc;

/// How a queued request's wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueExit {
    /// It got a slot.
    Granted,
    /// It was cancelled or dropped before it got one.
    Abandoned,
}

/// Told by a scheduler when a request of the observed scope waits.
pub trait AdmissionObserver: Send + Sync {
    /// A request could not be granted at once and is now queued.
    fn queued(&self);
    /// A request that was reported queued is no longer.
    fn left_queue(&self, exit: QueueExit);
    /// A request of the observed scope holds a slot, granted at once or after waiting.
    fn holding(&self);
    /// A slot the observed scope held was given back.
    fn released(&self);
}

/// The observer of the job whose handler is running this future, if any.
pub fn current_observer() -> Option<Arc<dyn AdmissionObserver>> {
    None
}

/// Runs `work` inside a scope observed by `observer`.
pub async fn observing<F: Future>(_observer: Arc<dyn AdmissionObserver>, work: F) -> F::Output {
    work.await
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;

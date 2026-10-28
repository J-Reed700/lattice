//! The order workers queue for their slots in.
use std::{
    collections::BTreeSet,
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    task::Poll,
};
use tokio::sync::watch;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Workers by the order they were dispatched in, until each queues for its
/// slot or waits for its subject: a worker queues for a slot only after every
/// worker dispatched before it, so a job queued again goes behind the work
/// already waiting, however long that work takes to read its job.
#[derive(Default)]
pub(super) struct Arrivals {
    next: AtomicU64,
    arriving: Mutex<BTreeSet<u64>>,
    left: watch::Sender<u64>,
}

impl Arrivals {
    pub(super) fn arrive(self: &Arc<Self>) -> Arrival {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        lock(&self.arriving).insert(ticket);
        Arrival {
            arrivals: Arc::clone(self),
            ticket,
        }
    }
}

/// A worker's place in [`Arrivals`]; dropping it lets later workers queue.
pub(super) struct Arrival {
    arrivals: Arc<Arrivals>,
    ticket: u64,
}

impl Arrival {
    /// Waits until every worker dispatched before this one has queued for its
    /// slot or is waiting for its subject.
    pub(super) async fn turn(&self) {
        let mut left = self.arrivals.left.subscribe();
        while lock(&self.arrivals.arriving)
            .range(..self.ticket)
            .next()
            .is_some()
        {
            if left.changed().await.is_err() {
                return;
            }
        }
    }
}

impl Drop for Arrival {
    fn drop(&mut self) {
        lock(&self.arrivals.arriving).remove(&self.ticket);
        self.arrivals
            .left
            .send_modify(|count| *count = count.wrapping_add(1));
    }
}

/// `work` polled once, without waiting for it.
pub(super) async fn poll_once<F: Future>(mut work: std::pin::Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_worker_queues_only_after_every_worker_dispatched_before_it() {
        let arrivals = Arc::new(Arrivals::default());
        let first = arrivals.arrive();
        let second = arrivals.arrive();
        let third = arrivals.arrive();
        drop(third);
        let waiting = tokio::spawn(async move { second.turn().await });
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert!(!waiting.is_finished(), "the second waits for the first");
        drop(first);
        waiting.await.unwrap();
    }
}

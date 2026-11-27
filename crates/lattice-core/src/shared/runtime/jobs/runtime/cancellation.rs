use super::super::JobRecord;
use super::{lock, JobRuntime};
use crate::shared::error::Result;

impl JobRuntime {
    /// Signals a running worker to stop and blocks dispatch until settled.
    pub fn begin_cancel(&self, id: &str) {
        let token = {
            let active = lock(&self.active);
            let mut cancelling = lock(&self.cancelling);
            *cancelling.entry(id.to_string()).or_default() += 1;
            active.get(id).cloned()
        };
        lock(&self.requeued).remove(id);
        if let Some(token) = token {
            token.cancel();
        }
    }

    pub(super) fn release_cancel(&self, id: &str) -> bool {
        let mut cancelling = lock(&self.cancelling);
        match cancelling.get_mut(id) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => {
                cancelling.remove(id);
                true
            }
            None => true,
        }
    }

    /// Completes an in-flight cancellation after the database transition commits.
    pub async fn finish_cancel(&self, id: &str) {
        self.release_cancel(id);
        self.cancelled(id).await;
    }

    /// Clears a failed cancellation and wakes the dispatcher for suspended work.
    pub fn abort_cancel(&self, id: &str) {
        if self.release_cancel(id) {
            self.wake.notify_one();
        }
    }

    /// Trips in-memory tokens and blocks dispatch for every job in `ids`.
    pub fn begin_cancel_many<S: AsRef<str>>(&self, ids: &[S]) {
        for id in ids {
            self.begin_cancel(id.as_ref());
        }
    }

    /// Releases batch cancellation guards and publishes cancelled events.
    pub async fn finish_cancel_many<S: AsRef<str>>(&self, ids: &[S]) {
        for id in ids {
            self.finish_cancel(id.as_ref()).await;
        }
    }

    /// Releases batch cancellation guards on failure and wakes the dispatcher once.
    pub fn abort_cancel_many<S: AsRef<str>>(&self, ids: &[S]) {
        let mut wake = false;
        for id in ids {
            wake |= self.release_cancel(id.as_ref());
        }
        if wake {
            self.wake.notify_one();
        }
    }

    /// Signals the worker immediately, commits the cancellation on the urgent
    /// write lane, and publishes the cancelled state.
    pub async fn cancel(&self, id: &str) -> Result<JobRecord> {
        self.begin_cancel(id);
        if let Err(error) = self.store().cancel(id).await {
            self.abort_cancel(id);
            return Err(error);
        }
        self.finish_cancel(id).await;
        self.store().get_summary(id).await
    }

    /// For a cancellation a feature committed in its own transaction: signal
    /// the worker, whose later writes can no longer change the saved status.
    /// A job cancelled while it waits out a deferral has no worker to forget
    /// its last failure, so it is forgotten here.
    pub async fn cancelled(&self, id: &str) {
        if let Some(token) = lock(&self.active).get(id) {
            token.cancel();
        }
        lock(&self.deferred).remove(id);
        self.events.publish(id).await;
    }

    /// Gives up a job's turn without ending it: its worker stops, while
    /// waiting or at its next cancellation point, and the job queues again
    /// behind the work already waiting. A running job of a kind that
    /// interrupts on recovery ends as interrupted instead. A job with no
    /// worker is left alone.
    pub fn requeue(&self, id: &str) {
        if let Some(token) = lock(&self.active).get(id) {
            lock(&self.requeued).insert(id.to_string());
            token.cancel();
        }
    }
}

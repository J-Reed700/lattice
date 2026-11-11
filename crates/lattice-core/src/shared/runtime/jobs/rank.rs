//! Where a job stands in its course's backlog, for the schedulers that serve
//! its model calls.
//!
//! A ranked job's handler runs inside a [`RankScope`], held in a task-local, so
//! every call it makes, through any nested future, can read its rank with
//! [`current_rank`]. A scheduler keeps a [`RankTicket`], never the scope: the
//! scope is alive exactly while the handler runs, and its end tells the
//! schedulers that saw it.
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

tokio::task_local! { static CURRENT: Arc<RankScope>; }

/// Told when a rank's scope ends.
pub trait RankWatcher: Send + Sync {
    fn rank_ended(&self);
}

/// The lifetime of one ranked job's handler. Lower ranks run first.
pub struct RankScope {
    rank: i64,
    watchers: Mutex<Vec<Weak<dyn RankWatcher>>>,
}

impl RankScope {
    pub fn new(rank: i64) -> Arc<Self> {
        Arc::new(Self {
            rank,
            watchers: Mutex::default(),
        })
    }

    fn watchers(&self) -> MutexGuard<'_, Vec<Weak<dyn RankWatcher>>> {
        self.watchers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for RankScope {
    fn drop(&mut self) {
        let watchers = std::mem::take(&mut *self.watchers());
        for watcher in watchers.iter().filter_map(Weak::upgrade) {
            watcher.rank_ended();
        }
    }
}

/// A rank and a way to tell whether its scope is still alive, without keeping
/// it alive.
#[derive(Debug, Clone)]
pub struct RankTicket {
    rank: i64,
    scope: Weak<RankScope>,
}

impl RankTicket {
    pub fn new(scope: &Arc<RankScope>) -> Self {
        Self {
            rank: scope.rank,
            scope: Arc::downgrade(scope),
        }
    }

    pub fn rank(&self) -> i64 {
        self.rank
    }

    pub fn is_alive(&self) -> bool {
        self.scope.strong_count() > 0
    }

    pub fn is_scope_of(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.scope, &other.scope)
    }

    /// Have `watcher` told when the scope ends. A scope that has ended is not
    /// watched. Call this without holding a lock the watcher takes: if the
    /// scope's owner lets go meanwhile, this is where its end is announced.
    pub fn watch(&self, watcher: Weak<dyn RankWatcher>) {
        let Some(scope) = self.scope.upgrade() else {
            return;
        };
        let mut watchers = scope.watchers();
        watchers.retain(|known| known.strong_count() > 0);
        if !watchers.iter().any(|known| Weak::ptr_eq(known, &watcher)) {
            watchers.push(watcher);
        }
    }
}

/// The rank of the job whose handler is running this future, if it has one.
pub fn current_rank() -> Option<RankTicket> {
    CURRENT.try_with(RankTicket::new).ok()
}

/// Runs `work` inside `scope`, when there is one.
pub(super) async fn within<F: Future>(scope: Option<Arc<RankScope>>, work: F) -> F::Output {
    match scope {
        Some(scope) => CURRENT.scope(scope, work).await,
        None => work.await,
    }
}

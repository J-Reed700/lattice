//! Sharing of live child processes, keyed by the configuration that started
//! them.
//!
//! The chat, router and utility roles each own an independent model cache, and
//! each cache loads through the factory. Nothing between them knew what the
//! others were running, so three roles pointing at one GGUF started three
//! `llama-server` processes and loaded the same weights three times — on an
//! 8 GB machine the second load is what turns a working app into an OOM.
//!
//! This pool is the missing shared fact. Callers ask for a configuration
//! instead of a process: an identical request that is still alive gets an
//! `Arc` to the running one, and only a genuinely new configuration starts
//! anything.
//!
//! # Lifetime
//!
//! The pool holds `Weak` references, never strong ones, so it cannot keep a
//! process alive past its last user. Reference counting *is* the shutdown
//! rule: the value's `Drop` runs when the last caller releases it, which for
//! a sidecar means the process is killed. Swapping the chat model therefore
//! leaves the server up if the utility role is still on it, and tears it down
//! if not — with no lifetime bookkeeping of its own.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, Weak};

use parking_lot::Mutex as SyncMutex;
use tokio::sync::Mutex as AsyncMutex;

/// A pooled value that can stop working without being dropped — a child
/// process that exits, crashes, or is killed from outside.
///
/// The pool needs this because reference counting alone cannot answer "is
/// this usable". A crashed server's handle stays perfectly alive as an `Arc`
/// for as long as any role holds it, and without this check a role loading
/// afterwards would be handed that corpse instead of starting a server —
/// worse, invalidating a role's cache to recover would hand it straight back,
/// so a single crash would be unrecoverable short of a restart.
pub trait Liveness {
    /// `false` once the thing behind this value is gone for good.
    fn is_running(&self) -> bool;
}

/// Where a value handed back by [`SharedProcesses::get_or_start`] came from.
/// Callers log the difference; without it, sharing is invisible in the log and
/// a regression back to duplicate loads would be silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Nothing matching was alive, so the starter ran.
    Started,
    /// An already-running value answered the request.
    Reused,
}

/// Live values keyed by what they were started from. See the module docs.
pub struct SharedProcesses<K, V> {
    slots: SyncMutex<HashMap<K, Slot<V>>>,
}

struct Slot<V> {
    /// The running value, if it is still running. `Weak` by design — see
    /// the module docs on lifetime.
    live: Weak<V>,

    /// Held across the start for this key so concurrent misses start one
    /// value rather than one each. Boot is exactly this case: the roles
    /// prewarm in parallel, so without the gate the first request from each
    /// role misses simultaneously and every one of them spawns.
    gate: Arc<AsyncMutex<()>>,
}

impl<K: Clone + Eq + Hash, V: Liveness> SharedProcesses<K, V> {
    pub fn new() -> Self {
        Self {
            slots: SyncMutex::new(HashMap::new()),
        }
    }

    /// Return the value running for `key`, starting one if none is.
    ///
    /// `start` runs at most once per key across concurrent callers, and only
    /// while no value for that key is alive. A failed start is not recorded:
    /// the next caller tries again, because the reason a model failed to load
    /// is usually transient (a port race, a busy GPU) and caching the failure
    /// would strand the role until restart.
    ///
    /// Cancellation is safe. Dropping this future while it holds the gate
    /// releases it, and a future cancelled mid-`start` publishes nothing —
    /// whatever `start` allocated is cleaned up by its own drop guard.
    pub async fn get_or_start<E, F, Fut>(&self, key: K, start: F) -> Result<(Arc<V>, Origin), E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        if let Some(live) = self.live(&key) {
            return Ok((live, Origin::Reused));
        }

        let gate = self.gate(&key);
        let _permit = gate.lock().await;

        // Whoever held the gate before us may have been starting this very
        // key. Re-checking under it is what makes the wait worth anything.
        if let Some(live) = self.live(&key) {
            return Ok((live, Origin::Reused));
        }

        let value = Arc::new(start().await?);
        self.publish(key, &value);
        Ok((value, Origin::Started))
    }

    /// Number of keys with something running. Diagnostics and tests.
    pub fn live_count(&self) -> usize {
        self.slots
            .lock()
            .values()
            .filter_map(|slot| slot.live.upgrade())
            .filter(|value| value.is_running())
            .count()
    }

    /// The running value for `key`, if there is one. A value that exists but
    /// has stopped is not a hit — see [`Liveness`].
    fn live(&self, key: &K) -> Option<Arc<V>> {
        self.slots
            .lock()
            .get(key)
            .and_then(|slot| slot.live.upgrade())
            .filter(|value| value.is_running())
    }

    /// The gate for `key`, created on first use. Deliberately *not* removed
    /// when the value behind it dies: two callers must agree on one gate, and
    /// a gate that can vanish between their lookups lets both start.
    fn gate(&self, key: &K) -> Arc<AsyncMutex<()>> {
        let mut slots = self.slots.lock();
        Arc::clone(
            &slots
                .entry(key.clone())
                .or_insert_with(|| Slot {
                    live: Weak::new(),
                    gate: Arc::new(AsyncMutex::new(())),
                })
                .gate,
        )
    }

    fn publish(&self, key: K, value: &Arc<V>) {
        let mut slots = self.slots.lock();

        // Drop slots that can no longer serve or block anyone: the value is
        // gone and the map holds the only reference to the gate, so — checked
        // under this lock, which `gate()` also takes — nobody is waiting on
        // it or about to. Keeps a long session's map to the configurations
        // actually in use rather than every one it has ever seen.
        slots.retain(|_, slot| slot.live.strong_count() > 0 || Arc::strong_count(&slot.gate) > 1);

        let entry = slots.entry(key).or_insert_with(|| Slot {
            live: Weak::new(),
            gate: Arc::new(AsyncMutex::new(())),
        });
        entry.live = Arc::downgrade(value);
    }
}

impl<K: Clone + Eq + Hash, V: Liveness> Default for SharedProcesses<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// Stands in for a sidecar: counts how many were started, how many are
    /// still held, and whether the process behind it is still up — so a test
    /// can tell sharing from duplication, and a live server from a corpse.
    struct Proc {
        held: Arc<AtomicUsize>,
        running: Arc<AtomicBool>,
    }

    impl Proc {
        /// What a crash looks like: the process is gone but every holder
        /// still has its handle.
        fn crash(&self) {
            self.running.store(false, Ordering::SeqCst);
        }
    }

    impl Liveness for Proc {
        fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
    }

    impl Drop for Proc {
        fn drop(&mut self) {
            self.held.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct Spawner {
        started: Arc<AtomicUsize>,
        held: Arc<AtomicUsize>,
    }

    impl Spawner {
        fn new() -> Self {
            Self {
                started: Arc::new(AtomicUsize::new(0)),
                held: Arc::new(AtomicUsize::new(0)),
            }
        }

        async fn start(&self) -> Result<Proc, &'static str> {
            self.started.fetch_add(1, Ordering::SeqCst);
            // Yield so a concurrent caller gets a chance to race us.
            tokio::task::yield_now().await;
            self.held.fetch_add(1, Ordering::SeqCst);
            Ok(Proc {
                held: Arc::clone(&self.held),
                running: Arc::new(AtomicBool::new(true)),
            })
        }
    }

    #[tokio::test]
    async fn the_same_configuration_gets_the_running_process() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        let (first, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Started);
        let (second, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Reused);

        assert!(
            Arc::ptr_eq(&first, &second),
            "both callers share one process"
        );
        assert_eq!(spawner.started.load(Ordering::SeqCst), 1);
        assert_eq!(spawner.held.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_different_configuration_starts_its_own_process() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        let (_a, _) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        let (_b, origin) = pool.get_or_start("b", || spawner.start()).await.unwrap();

        assert_eq!(origin, Origin::Started);
        assert_eq!(spawner.started.load(Ordering::SeqCst), 2);
        assert_eq!(pool.live_count(), 2);
    }

    #[tokio::test]
    async fn concurrent_first_requests_start_one_process() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        // What boot does: every role misses at the same moment.
        let (first, second, third) = tokio::join!(
            pool.get_or_start("a", || spawner.start()),
            pool.get_or_start("a", || spawner.start()),
            pool.get_or_start("a", || spawner.start()),
        );
        let first = first.unwrap().0;
        let second = second.unwrap().0;
        let third = third.unwrap().0;

        assert_eq!(
            spawner.started.load(Ordering::SeqCst),
            1,
            "the gate should have collapsed three simultaneous misses into one start"
        );
        assert!(Arc::ptr_eq(&first, &second) && Arc::ptr_eq(&second, &third));
    }

    #[tokio::test]
    async fn the_process_dies_with_its_last_user_and_the_next_request_restarts_it() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        let (first, _) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        let (second, _) = pool.get_or_start("a", || spawner.start()).await.unwrap();

        drop(first);
        assert_eq!(
            spawner.held.load(Ordering::SeqCst),
            1,
            "one role releasing must not stop a process another role is using"
        );

        drop(second);
        assert_eq!(spawner.held.load(Ordering::SeqCst), 0);
        assert_eq!(pool.live_count(), 0);

        let (_restarted, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Started, "a dead slot must not be served");
        assert_eq!(spawner.started.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_failed_start_is_not_remembered() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        let failed = pool
            .get_or_start("a", || async { Err::<Proc, &str>("no GPU today") })
            .await;
        assert_eq!(failed.err(), Some("no GPU today"));

        let (_retried, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Started, "failure must stay retryable");
    }

    /// A crash is not a drop: the handle is still held, so reference counting
    /// alone would report the server as available forever.
    #[tokio::test]
    async fn a_crashed_process_is_not_handed_out() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        let (crashed, _) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        crashed.crash();

        assert_eq!(pool.live_count(), 0, "a crashed server is not a live one");

        let (replacement, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Started);
        assert!(!Arc::ptr_eq(&crashed, &replacement));
        assert!(replacement.is_running());

        // And the role still holding the corpse recovers by reloading, rather
        // than being handed the same dead server back.
        let (recovered, origin) = pool.get_or_start("a", || spawner.start()).await.unwrap();
        assert_eq!(origin, Origin::Reused);
        assert!(Arc::ptr_eq(&recovered, &replacement));
    }

    #[tokio::test]
    async fn dead_slots_do_not_accumulate() {
        let pool: SharedProcesses<&str, Proc> = SharedProcesses::new();
        let spawner = Spawner::new();

        // A session that switches models repeatedly, releasing each one.
        for key in ["a", "b", "c", "d"] {
            let (proc, _) = pool.get_or_start(key, || spawner.start()).await.unwrap();
            drop(proc);
        }
        let (_held, _) = pool.get_or_start("e", || spawner.start()).await.unwrap();

        assert_eq!(pool.live_count(), 1);
        assert_eq!(
            pool.slots.lock().len(),
            1,
            "publishing should have swept the slots nothing can reach"
        );
    }
}

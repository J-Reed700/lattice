//! One admission point in front of each inference backend: every model call
//! is admitted here, by a scheduler that knows the backend's concurrency and,
//! for a llama-server, the window its slots share.
//!
//! Highest [`InferencePriority`] first, arrival order within one; with several
//! slots one is kept for interactive work. On a shared window a request
//! reserves its estimated prompt plus output and waits while reservations
//! would overflow, though a request alone is always admitted. Admission is
//! strictly in order, so a large interactive prompt is never starved, and a
//! cancelled queued request leaves with an error. On llama-server a keyed
//! request is pinned back to the slot that last served its key, so the server
//! reuses the cached prefix, and a held key's slot is kept for interactive
//! work while the hold lasts ([`InferenceScheduler::hold`]).

mod estimator;
mod registry;
pub(crate) mod rejection;
mod scheduled;
mod tokenize;

pub(crate) use estimator::{PaceEstimator, TokenEstimator};
pub(crate) use registry::{llama_server_capacity, SchedulerRegistry};
pub(crate) use scheduled::ScheduledLlm;
pub(crate) use tokenize::ServerTokenizer;

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use lattice_core::application::ports::llm_port::{InferencePriority, RequestLoad};
use lattice_core::shared::error::{AppError, Result};
use tokio::sync::watch;

/// A wait longer than this is worth a line in the log: it is the contention
/// this scheduler exists to arbitrate, and otherwise invisible.
const SLOW_ADMISSION: std::time::Duration = std::time::Duration::from_secs(1);

/// What one backend can run at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackendCapacity {
    /// Requests the backend serves side by side.
    pub slots: usize,
    /// Tokens the in-flight requests share, when they share one window
    /// (llama-server's unified KV cache). `None` when each request has its
    /// own.
    pub shared_window: Option<usize>,
    /// Whether admitted requests are pinned to a llama-server slot.
    pub pins_slots: bool,
}

impl BackendCapacity {
    /// A backend that runs `slots` requests, each with its own context.
    pub fn concurrent(slots: usize) -> Self {
        Self {
            slots: slots.max(1),
            shared_window: None,
            pins_slots: false,
        }
    }

    /// A llama-server: `slots` slots over one `window`-token KV cache.
    pub fn llama_server(slots: usize, window: usize) -> Self {
        Self {
            slots: slots.max(1),
            shared_window: Some(window.max(1)),
            pins_slots: true,
        }
    }
}

/// What one request asks the scheduler for.
#[derive(Debug, Clone)]
pub(crate) struct AdmissionRequest {
    pub priority: InferencePriority,
    /// Estimated prompt tokens plus the output allowance.
    pub tokens: usize,
    pub cache_key: Option<String>,
}

/// Queues and admits requests for one backend. See the module docs.
pub(crate) struct InferenceScheduler {
    label: String,
    capacity: BackendCapacity,
    state: Mutex<State>,
    /// Moved whenever a request joins, leaves or is admitted, so whoever
    /// shows what the backends are doing looks again. Shared by every
    /// scheduler one registry made.
    changes: Option<Arc<watch::Sender<u64>>>,
}

struct State {
    next_ticket: u64,
    /// Monotonic counter stamped onto a slot each time it is assigned.
    clock: u64,
    /// Waiting requests, highest priority first, then oldest first.
    queue: BTreeMap<(Reverse<InferencePriority>, u64), Waiter>,
    /// Admitted requests whose callers have not yet collected the grant.
    granted: HashMap<u64, Grant>,
    slots: Vec<Slot>,
    /// Tokens reserved by everything in flight.
    reserved: usize,
    /// Cache keys held, by how many holds each.
    held: HashMap<String, usize>,
}

#[derive(Default)]
struct Slot {
    /// Tokens the request running here reserved; `None` when the slot is free.
    running: Option<usize>,
    /// The cache key of the last request this slot served. Its prefix is what
    /// the slot's KV cache still holds.
    last_key: Option<String>,
    /// Monotonic timestamp of the last admission to this slot (`0` when never used).
    last_used: u64,
}

struct Waiter {
    priority: InferencePriority,
    tokens: usize,
    cache_key: Option<String>,
    wake: oneshot::Sender<()>,
}

#[derive(Debug, Clone, Copy)]
struct Grant {
    slot: usize,
    tokens: usize,
}

/// An admitted request's hold on its slot and its share of the window.
/// Dropping it releases both and admits whatever can now run.
pub(crate) struct Permit {
    scheduler: Arc<InferenceScheduler>,
    grant: Grant,
}

impl Permit {
    /// The llama-server slot to send as `id_slot`, when this backend pins.
    pub fn slot(&self) -> Option<u32> {
        self.scheduler
            .capacity
            .pins_slots
            .then(|| u32::try_from(self.grant.slot).ok())
            .flatten()
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.scheduler.state.lock();
        self.scheduler.release(&mut state, self.grant);
    }
}

/// A key's hold on the slot that serves it; see [`InferenceScheduler::hold`].
/// Dropping it gives the slot back to whatever waits.
pub(crate) struct SlotHold {
    scheduler: Arc<InferenceScheduler>,
    key: String,
}

impl Drop for SlotHold {
    fn drop(&mut self) {
        let mut state = self.scheduler.state.lock();
        if let Some(count) = state.held.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                state.held.remove(&self.key);
            }
        }
        self.scheduler.dispatch(&mut state);
        drop(state);
        self.scheduler.moved();
    }
}

/// The error a cancelled request ends with, queued or in flight.
pub(crate) fn cancelled_error() -> AppError {
    AppError::InvalidState("The model request was cancelled.".into())
}

impl InferenceScheduler {
    pub fn new(label: impl Into<String>, capacity: BackendCapacity) -> Self {
        let slots = (0..capacity.slots.max(1))
            .map(|_| Slot::default())
            .collect();
        Self {
            label: label.into(),
            capacity,
            state: Mutex::new(State {
                next_ticket: 0,
                clock: 0,
                queue: BTreeMap::new(),
                granted: HashMap::new(),
                slots,
                reserved: 0,
                held: HashMap::new(),
            }),
            changes: None,
        }
    }

    /// A scheduler that moves `changes` on every change to its queue.
    pub fn observed(
        label: impl Into<String>,
        capacity: BackendCapacity,
        changes: Arc<watch::Sender<u64>>,
    ) -> Self {
        Self {
            changes: Some(changes),
            ..Self::new(label, capacity)
        }
    }

    pub fn capacity(&self) -> BackendCapacity {
        self.capacity
    }

    /// The requests running and waiting now.
    pub fn load(&self) -> RequestLoad {
        let state = self.state.lock();
        RequestLoad {
            backend: self.label.clone(),
            slots: state.slots.len(),
            running: state
                .slots
                .iter()
                .filter(|slot| slot.running.is_some())
                .count(),
            queued: state.queue.len(),
            queued_interactive: state
                .queue
                .values()
                .filter(|waiter| waiter.priority == InferencePriority::Interactive)
                .count(),
        }
    }

    fn moved(&self) {
        if let Some(changes) = &self.changes {
            changes.send_modify(|count| *count = count.wrapping_add(1));
        }
    }

    /// Wait for a slot. Fails only when `cancel` fires first.
    pub async fn admit(
        self: &Arc<Self>,
        request: AdmissionRequest,
        cancel: Option<&CancellationToken>,
    ) -> Result<Permit> {
        let tokens = match self.capacity.shared_window {
            // A reservation larger than the window could never fit beside
            // anything; capping it keeps the arithmetic meaningful.
            Some(window) => request.tokens.min(window),
            None => request.tokens,
        };
        let queued_at = std::time::Instant::now();
        let (ticket, woken) = {
            let mut state = self.state.lock();
            let ticket = state.next_ticket;
            state.next_ticket += 1;
            let (wake, woken) = oneshot::channel();
            state.queue.insert(
                (Reverse(request.priority), ticket),
                Waiter {
                    priority: request.priority,
                    tokens,
                    cache_key: request.cache_key,
                    wake,
                },
            );
            self.dispatch(&mut state);
            (ticket, woken)
        };
        self.moved();
        let mut waiting = Waiting {
            scheduler: self,
            ticket,
            priority: request.priority,
            settled: false,
        };
        let admitted = match cancel {
            Some(token) => tokio::select! {
                biased;
                _ = token.cancelled() => false,
                _ = woken => true,
            },
            None => woken.await.is_ok(),
        };
        match waiting.settle(admitted) {
            Some(grant) => {
                let waited = queued_at.elapsed();
                if waited >= SLOW_ADMISSION {
                    tracing::info!(
                        backend = %self.label,
                        priority = ?request.priority,
                        waited_ms = u64::try_from(waited.as_millis()).unwrap_or(u64::MAX),
                        "Model request waited for a slot"
                    );
                }
                Ok(Permit {
                    scheduler: Arc::clone(self),
                    grant,
                })
            }
            None => {
                tracing::debug!(
                    backend = %self.label,
                    priority = ?request.priority,
                    "Queued model request cancelled before it ran"
                );
                Err(cancelled_error())
            }
        }
    }

    /// Keeps the slot holding `key`'s prompt for interactive work until the
    /// hold drops, so a turn's prefix survives between its requests: other
    /// work waits for it or runs on another slot. Every request the holder
    /// sends while holding must be interactive, or it waits for its own hold.
    pub fn hold(self: &Arc<Self>, key: &str) -> SlotHold {
        *self.state.lock().held.entry(key.to_owned()).or_default() += 1;
        SlotHold {
            scheduler: Arc::clone(self),
            key: key.to_owned(),
        }
    }

    /// Admit from the head of the queue for as long as the head can run.
    fn dispatch(&self, state: &mut State) {
        loop {
            let Some((&key, head)) = state.queue.iter().next() else {
                return;
            };
            let free: Vec<usize> = state
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.running.is_none())
                .map(|(index, _)| index)
                .collect();
            if free.is_empty() {
                return;
            }
            let interactive = head.priority == InferencePriority::Interactive;
            if !interactive && state.slots.len() > 1 && free.len() < 2 {
                return;
            }
            let open: Vec<usize> = free
                .iter()
                .copied()
                .filter(|&index| {
                    interactive
                        || !state.slots.get(index).is_some_and(|slot| {
                            slot.last_key
                                .as_ref()
                                .is_some_and(|key| state.held.contains_key(key))
                        })
                })
                .collect();
            if open.is_empty() {
                return;
            }
            let in_flight = state.slots.len() - free.len();
            if let Some(window) = self.capacity.shared_window {
                if in_flight > 0 && state.reserved.saturating_add(head.tokens) > window {
                    return;
                }
            }
            let slot = choose_slot(&state.slots, &open, head.cache_key.as_deref());
            let Some(waiter) = state.queue.remove(&key) else {
                return;
            };
            let previous_owner = state.slots.iter().position(|candidate| {
                waiter.cache_key.is_some() && candidate.last_key == waiter.cache_key
            });
            if let Some(previous) = previous_owner.filter(|previous| *previous != slot) {
                if let Some(stale) = state.slots.get_mut(previous) {
                    stale.last_key = None;
                }
            }
            state.clock = state.clock.saturating_add(1);
            let now = state.clock;
            if let Some(chosen) = state.slots.get_mut(slot) {
                chosen.running = Some(waiter.tokens);
                chosen.last_key = waiter.cache_key;
                chosen.last_used = now;
            }
            state.reserved = state.reserved.saturating_add(waiter.tokens);
            let (_, ticket) = key;
            state.granted.insert(
                ticket,
                Grant {
                    slot,
                    tokens: waiter.tokens,
                },
            );
            // A caller that has gone away collects nothing; its guard releases
            // the grant under this same lock.
            let _ = waiter.wake.send(());
        }
    }

    fn release(&self, state: &mut State, grant: Grant) {
        if let Some(slot) = state.slots.get_mut(grant.slot) {
            slot.running = None;
        }
        state.reserved = state.reserved.saturating_sub(grant.tokens);
        self.dispatch(state);
        self.moved();
    }

    #[cfg(test)]
    fn queued(&self) -> usize {
        self.state.lock().queue.len()
    }
}

/// Pick a free slot for `key`:
/// - Keyed requests (`Some(key)`) prefer the slot that last served `key`, then
///   a never-used slot, then a slot that last served an unkeyed utility call,
///   and finally the least-recently-used free slot.
/// - Unkeyed requests (`None`, e.g. intent classification, HyDE, verification)
///   prefer a slot that already ran an unkeyed request — isolating ephemeral
///   utility calls to one reusable slot so they do not overwrite active
///   conversation KV caches — then a never-used slot, then the least-recently-
///   used free slot.
fn choose_slot(slots: &[Slot], free: &[usize], key: Option<&str>) -> usize {
    let lru_free = || {
        free.iter()
            .copied()
            .min_by_key(|&index| slots.get(index).map_or(0, |s| s.last_used))
    };
    let never_used = || {
        free.iter()
            .copied()
            .find(|&index| slots.get(index).is_some_and(|s| s.last_used == 0))
    };
    let unkeyed_used = || {
        free.iter().copied().find(|&index| {
            slots
                .get(index)
                .is_some_and(|s| s.last_used > 0 && s.last_key.is_none())
        })
    };

    match key {
        Some(key) => free
            .iter()
            .copied()
            .find(|&index| slots.get(index).and_then(|s| s.last_key.as_deref()) == Some(key))
            .or_else(never_used)
            .or_else(unkeyed_used)
            .or_else(lru_free)
            .unwrap_or(0),
        None => unkeyed_used()
            .or_else(never_used)
            .or_else(lru_free)
            .unwrap_or(0),
    }
}

/// A caller's place in the queue. Whatever way the wait ends — admitted,
/// cancelled, or the future dropped — the scheduler is left consistent.
struct Waiting<'a> {
    scheduler: &'a Arc<InferenceScheduler>,
    ticket: u64,
    priority: InferencePriority,
    settled: bool,
}

impl Waiting<'_> {
    /// Collect the grant when `admitted`; otherwise leave the queue, or hand
    /// back a grant that raced the cancellation.
    fn settle(&mut self, admitted: bool) -> Option<Grant> {
        self.settled = true;
        let mut state = self.scheduler.state.lock();
        match state.granted.remove(&self.ticket) {
            Some(grant) if admitted => Some(grant),
            Some(grant) => {
                self.scheduler.release(&mut state, grant);
                None
            }
            None => {
                state.queue.remove(&(Reverse(self.priority), self.ticket));
                self.scheduler.moved();
                None
            }
        }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.settle(false);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vision_tests;

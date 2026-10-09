//! One admission point in front of each inference backend.
//!
//! Every model call that reaches a backend — a chat turn, the grounding judge,
//! memory consolidation, lesson generation — is admitted here first, by a
//! scheduler that knows how many requests the backend runs at once and, for a
//! llama-server, the context window its slots share. Before this each feature
//! brought its own limiter (Ollama's semaphore, one lesson job at a time, one
//! consolidation at a time, three judge calls at a time), none of them knew
//! about the others, and chat itself was not limited at all: a turn could wait
//! behind a lesson generation it had no way to outrank.
//!
//! # Admission rules
//!
//! - Highest [`InferencePriority`] first; arrival order within a priority.
//! - When the backend has more than one slot, one is kept for interactive
//!   work: anything else is admitted only while two or more slots are free.
//! - On a shared window, a request reserves its estimated prompt plus its
//!   output allowance, and is admitted only while every reservation in flight
//!   still fits. A request with nothing else in flight is always admitted, so
//!   a prompt larger than the window fails at the server rather than waiting
//!   forever here.
//! - Strictly in order: when the head of the queue cannot be admitted, nothing
//!   behind it is either, so a large interactive prompt is never starved by a
//!   stream of small background ones.
//! - A queued request whose cancellation fires leaves the queue with an error.
//!
//! # Slots
//!
//! On llama-server every admitted request is pinned to a slot (`id_slot`). A
//! request carrying a cache key goes back to the slot that last served that
//! key when it is free, so the server reuses the shared prefix from its KV
//! cache instead of prefilling the whole conversation again.

mod estimator;
mod registry;
mod scheduled;
mod tokenize;

pub(crate) use estimator::TokenEstimator;
pub(crate) use registry::{
    cloud_scheduler, llama_server_capacity, ollama_scheduler, remote_llama_cpp_scheduler,
};
pub(crate) use scheduled::ScheduledLlm;
pub(crate) use tokenize::ServerTokenizer;

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::application::ports::llm_port::InferencePriority;
use crate::shared::error::{AppError, Result};

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
}

struct State {
    next_ticket: u64,
    /// Waiting requests, highest priority first, then oldest first.
    queue: BTreeMap<(Reverse<InferencePriority>, u64), Waiter>,
    /// Admitted requests whose callers have not yet collected the grant.
    granted: HashMap<u64, Grant>,
    slots: Vec<Slot>,
    /// Tokens reserved by everything in flight.
    reserved: usize,
}

#[derive(Default)]
struct Slot {
    /// Tokens the request running here reserved; `None` when the slot is free.
    running: Option<usize>,
    /// The cache key of the last request this slot served. Its prefix is what
    /// the slot's KV cache still holds.
    last_key: Option<String>,
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
                queue: BTreeMap::new(),
                granted: HashMap::new(),
                slots,
                reserved: 0,
            }),
        }
    }

    pub fn capacity(&self) -> BackendCapacity {
        self.capacity
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
            if head.priority != InferencePriority::Interactive
                && state.slots.len() > 1
                && free.len() < 2
            {
                return;
            }
            let in_flight = state.slots.len() - free.len();
            if let Some(window) = self.capacity.shared_window {
                if in_flight > 0 && state.reserved.saturating_add(head.tokens) > window {
                    return;
                }
            }
            let slot = choose_slot(&state.slots, &free, head.cache_key.as_deref());
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
            if let Some(chosen) = state.slots.get_mut(slot) {
                chosen.running = Some(waiter.tokens);
                chosen.last_key = waiter.cache_key;
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
    }

    #[cfg(test)]
    fn queued(&self) -> usize {
        self.state.lock().queue.len()
    }
}

/// The slot that last served `key` when it is free; otherwise the lowest free
/// slot no other key is waiting to reuse; otherwise the lowest free slot.
fn choose_slot(slots: &[Slot], free: &[usize], key: Option<&str>) -> usize {
    let affine = key.and_then(|key| {
        free.iter()
            .copied()
            .find(|index| slots.get(*index).and_then(|s| s.last_key.as_deref()) == Some(key))
    });
    affine
        .or_else(|| {
            free.iter()
                .copied()
                .find(|index| slots.get(*index).is_some_and(|s| s.last_key.is_none()))
        })
        .or_else(|| free.first().copied())
        .unwrap_or(0)
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

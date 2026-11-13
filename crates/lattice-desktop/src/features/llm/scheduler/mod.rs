//! One admission point in front of each inference backend: every model call
//! is admitted here, by a scheduler that knows the backend's concurrency and,
//! for a llama-server, the window its slots share.
//!
//! Interactive requests go first, then unranked ones by [`InferencePriority`],
//! then the ranked backlog by rank, lowest first, and by priority within a
//! rank; arrival order breaks every tie. With several slots one is kept for
//! interactive work, and while the lowest rank seen here has a live job one is
//! kept for that rank too, so a course's first lesson
//! never waits behind its later ones. On a shared window a request
//! reserves its estimated prompt plus output and waits while reservations
//! would overflow, though a request alone is always admitted. Admission is
//! strictly in order, so a large interactive prompt is never starved, and a
//! cancelled queued request leaves with an error. On llama-server a keyed
//! request is pinned back to the slot that last served its key, so the server
//! reuses the cached prefix, and a held key's slot is kept for interactive
//! work while the hold lasts ([`InferenceScheduler::hold`]).
//!
//! A cloud provider's capacity is not ours to know: its scheduler admits by
//! a congestion window that the provider's own answers move
//! ([`InferenceScheduler::accepted`], [`InferenceScheduler::throttled`]).

mod congestion;
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
use std::time::Duration;
use tokio::time::Instant;

use congestion::CongestionWindow;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use lattice_core::application::ports::llm_port::{InferencePriority, RequestLoad};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    current_observer, HeldSlot, QueuedRequest, RankTicket, RankWatcher,
};
use std::sync::Weak;
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
    /// Whether the slots follow a congestion window the backend's answers
    /// move, which starts at one slot.
    pub adaptive: bool,
}

impl BackendCapacity {
    /// A backend that runs `slots` requests, each with its own context.
    pub fn concurrent(slots: usize) -> Self {
        Self {
            slots: slots.max(1),
            shared_window: None,
            pins_slots: false,
            adaptive: false,
        }
    }

    /// A backend whose capacity is learned from its answers: one request
    /// until it has answered, then as many as it keeps accepting.
    pub fn adaptive() -> Self {
        Self {
            adaptive: true,
            ..Self::concurrent(1)
        }
    }

    /// A llama-server: `slots` slots over one `window`-token KV cache.
    pub fn llama_server(slots: usize, window: usize) -> Self {
        Self {
            slots: slots.max(1),
            shared_window: Some(window.max(1)),
            pins_slots: true,
            adaptive: false,
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
    /// The rank of the job this call is made for, if it has one.
    pub rank: Option<RankTicket>,
}

/// Where a waiting request queues: interactive requests first, then unranked
/// ones, then the ranked backlog, which rank orders before priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Lane {
    Interactive,
    Unranked(Reverse<InferencePriority>),
    Ranked(i64, Reverse<InferencePriority>),
}

impl Lane {
    fn of(priority: InferencePriority, rank: Option<&RankTicket>) -> Self {
        match rank {
            _ if priority == InferencePriority::Interactive => Self::Interactive,
            Some(ticket) => Self::Ranked(ticket.rank(), Reverse(priority)),
            None => Self::Unranked(Reverse(priority)),
        }
    }
}

type QueueKey = (Lane, u64);

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
    /// Waiting requests in admission order.
    queue: BTreeMap<QueueKey, Waiter>,
    /// Admitted requests whose callers have not yet collected the grant.
    granted: HashMap<u64, Grant>,
    slots: Vec<Slot>,
    /// Tokens reserved by everything in flight.
    reserved: usize,
    /// Cache keys held, by how many holds each.
    held: HashMap<String, usize>,
    /// The ranks requests here have carried whose scopes may still be alive.
    ranks: Vec<RankTicket>,
    /// What the provider's answers allow, for an adaptive backend.
    congestion: Option<CongestionWindow>,
}

#[derive(Default)]
struct Slot {
    /// Cut by a shrinking window: never granted again, and gone once the
    /// request still running here has finished.
    retired: bool,
    /// Tokens the request running here reserved; `None` when the slot is free.
    running: Option<usize>,
    /// The rank of the request running here, if it has one.
    rank: Option<i64>,
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
    rank: Option<i64>,
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
    _held: Option<HeldSlot>,
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

impl RankWatcher for InferenceScheduler {
    fn rank_ended(&self) {
        let mut state = self.state.lock();
        self.dispatch(&mut state);
        drop(state);
        self.moved();
    }
}

/// The error a cancelled request ends with, queued or in flight.
pub(crate) fn cancelled_error() -> AppError {
    AppError::Cancelled("The model request was cancelled.".into())
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
                ranks: Vec::new(),
                congestion: capacity.adaptive.then(CongestionWindow::new),
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

    /// What this backend can run now; an adaptive one's slots are its window.
    pub fn capacity(&self) -> BackendCapacity {
        let state = self.state.lock();
        BackendCapacity {
            slots: active_slots(&state.slots),
            ..self.capacity
        }
    }

    /// The requests running and waiting now.
    pub fn load(&self) -> RequestLoad {
        let state = self.state.lock();
        RequestLoad {
            backend: self.label.clone(),
            slots: active_slots(&state.slots),
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
        let observer = current_observer();
        // Outside the lock: if the scope has just ended, this is where that is
        // announced, and the announcement takes the lock.
        if let Some(rank) = &request.rank {
            let watcher: Weak<Self> = Arc::downgrade(self);
            rank.watch(watcher);
        }
        let lane = Lane::of(request.priority, request.rank.as_ref());
        let (key, woken, granted_at_once) = {
            let mut state = self.state.lock();
            let key = (lane, state.next_ticket);
            state.next_ticket += 1;
            let (wake, woken) = oneshot::channel();
            if let Some(rank) = &request.rank {
                if rank.is_alive() && !state.ranks.iter().any(|known| known.is_scope_of(rank)) {
                    state.ranks.push(rank.clone());
                }
            }
            state.queue.insert(
                key,
                Waiter {
                    priority: request.priority,
                    tokens,
                    cache_key: request.cache_key,
                    rank: request.rank.as_ref().map(RankTicket::rank),
                    wake,
                },
            );
            self.dispatch(&mut state);
            let granted_at_once = state.granted.contains_key(&key.1);
            (key, woken, granted_at_once)
        };
        self.moved();
        let queued = observer
            .clone()
            .filter(|_| !granted_at_once)
            .map(QueuedRequest::new);
        let mut waiting = Waiting {
            scheduler: self,
            key,
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
                let held = observer.map(HeldSlot::new);
                if let Some(queued) = queued {
                    queued.granted();
                }
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
                    _held: held,
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
        state.ranks.retain(RankTicket::is_alive);
        let head_rank = state.ranks.iter().map(RankTicket::rank).min();
        loop {
            if state
                .congestion
                .as_ref()
                .is_some_and(|window| !window.is_open(Instant::now()))
            {
                return;
            }
            let Some((&key, head)) = state.queue.iter().next() else {
                return;
            };
            let in_flight = running_slots(&state.slots);
            if state
                .congestion
                .as_ref()
                .is_some_and(|window| in_flight >= window.limit())
            {
                return;
            }
            let free: Vec<usize> = state
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.running.is_none() && !slot.retired)
                .map(|(index, _)| index)
                .collect();
            if free.is_empty() {
                return;
            }
            let interactive = head.priority == InferencePriority::Interactive;
            if !interactive {
                let for_interactive = usize::from(active_slots(&state.slots) > 1);
                let behind_the_head = head_rank
                    .zip(head.rank)
                    .is_some_and(|(lowest, rank)| rank > lowest);
                let head_has_a_slot = state.slots.iter().any(|slot| {
                    slot.running.is_some() && slot.rank.is_some() && slot.rank == head_rank
                });
                let for_the_head = usize::from(behind_the_head && !head_has_a_slot);
                if free.len() < for_interactive + 1 + for_the_head {
                    return;
                }
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
                chosen.rank = waiter.rank;
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
        drop_retired_tail(&mut state.slots);
        state.reserved = state.reserved.saturating_sub(grant.tokens);
        self.dispatch(state);
        self.moved();
    }

    /// The provider accepted a request. The window grows when requests were
    /// being held back by it: waiting, or every slot in use.
    pub fn accepted(&self) {
        let mut state = self.state.lock();
        let in_flight = running_slots(&state.slots);
        let queued = !state.queue.is_empty();
        let Some(window) = state.congestion.as_mut() else {
            return;
        };
        let before = window.limit();
        window.accepted(queued || in_flight >= before);
        let after = window.limit();
        if after == before {
            return;
        }
        tracing::debug!(backend = %self.label, before, after, "Provider accepted requests: window grew");
        self.follow_window(&mut state);
        self.dispatch(&mut state);
        drop(state);
        self.moved();
    }

    /// The provider refused a request for load. Admission closes for the
    /// provider's `retry_after`, never less than the backoff consecutive
    /// refusals have earned, and the window shrinks. Returns how long until
    /// admission reopens.
    pub fn throttled(self: &Arc<Self>, retry_after: Option<Duration>) -> Duration {
        let now = Instant::now();
        let mut state = self.state.lock();
        let Some(window) = state.congestion.as_mut() else {
            return Duration::ZERO;
        };
        let (before, closed_before) = (window.limit(), window.open_at());
        let counted = window.is_open(now);
        let reopen = window.throttled(now, retry_after);
        let after = window.limit();
        let delay = reopen.saturating_duration_since(now);
        if counted {
            tracing::info!(
                backend = %self.label,
                window_before = before,
                window_after = after,
                retry_after_ms = ?retry_after.map(|wait| wait.as_millis()),
                reopen_in_ms = delay.as_millis(),
                "Provider throttled requests: window shrunk and admission closed"
            );
        }
        self.follow_window(&mut state);
        drop(state);
        self.moved();
        if closed_before != Some(reopen) {
            self.wake_at(reopen);
        }
        delay
    }

    /// Resolves when admission is open: at once for a backend that has not
    /// been throttled.
    pub async fn until_open(&self) {
        loop {
            let reopen = self
                .state
                .lock()
                .congestion
                .as_ref()
                .and_then(CongestionWindow::open_at);
            match reopen {
                Some(at) if Instant::now() < at => tokio::time::sleep_until(at).await,
                _ => return,
            }
        }
    }

    /// Runs admission again at `at`, when a throttle's closing ends. Holds the
    /// scheduler weakly, so it keeps nothing alive.
    fn wake_at(self: &Arc<Self>, at: Instant) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let scheduler: Weak<Self> = Arc::downgrade(self);
        runtime.spawn(async move {
            tokio::time::sleep_until(at).await;
            if let Some(scheduler) = scheduler.upgrade() {
                scheduler.rank_ended();
            }
        });
    }

    /// Make the slots as many as the window allows: free retired slots come
    /// back or new ones are added, and surplus ones, free first, are retired.
    fn follow_window(&self, state: &mut State) {
        let Some(limit) = state.congestion.as_ref().map(CongestionWindow::limit) else {
            return;
        };
        let mut active = active_slots(&state.slots);
        for slot in state.slots.iter_mut() {
            if active >= limit {
                break;
            }
            if slot.retired && slot.running.is_none() {
                slot.retired = false;
                active += 1;
            }
        }
        while active < limit {
            state.slots.push(Slot::default());
            active += 1;
        }
        for only_free in [true, false] {
            for slot in state.slots.iter_mut().rev() {
                if active <= limit {
                    break;
                }
                if !slot.retired && (!only_free || slot.running.is_none()) {
                    slot.retired = true;
                    active -= 1;
                }
            }
        }
        drop_retired_tail(&mut state.slots);
    }

    #[cfg(test)]
    fn queued(&self) -> usize {
        self.state.lock().queue.len()
    }

    /// How long until admission reopens, while a throttle keeps it closed.
    #[cfg(test)]
    pub(crate) fn reopens_in(&self) -> Option<Duration> {
        let open_at = self.state.lock().congestion.as_ref()?.open_at()?;
        let wait = open_at.saturating_duration_since(Instant::now());
        (!wait.is_zero()).then_some(wait)
    }
}

fn active_slots(slots: &[Slot]) -> usize {
    slots.iter().filter(|slot| !slot.retired).count()
}

fn running_slots(slots: &[Slot]) -> usize {
    slots.iter().filter(|slot| slot.running.is_some()).count()
}

/// A retired slot is gone once nothing runs on it; only the tail can go
/// without moving the index a running request holds.
fn drop_retired_tail(slots: &mut Vec<Slot>) {
    while slots
        .last()
        .is_some_and(|slot| slot.retired && slot.running.is_none())
    {
        slots.pop();
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
    key: QueueKey,
    settled: bool,
}

impl Waiting<'_> {
    /// Collect the grant when `admitted`; otherwise leave the queue, or hand
    /// back a grant that raced the cancellation.
    fn settle(&mut self, admitted: bool) -> Option<Grant> {
        self.settled = true;
        let mut state = self.scheduler.state.lock();
        match state.granted.remove(&self.key.1) {
            Some(grant) if admitted => Some(grant),
            Some(grant) => {
                self.scheduler.release(&mut state, grant);
                None
            }
            None => {
                state.queue.remove(&self.key);
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
mod adaptive_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vision_tests;

#[cfg(test)]
mod waiting_tests;

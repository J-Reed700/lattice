use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a cancellation that arrived before its turn registered stays armed.
/// Long enough to cover a cold model load, short enough that a stop press for a
/// turn that already finished cannot sit in memory indefinitely.
const PENDING_CANCEL_TTL: Duration = Duration::from_secs(600);
/// Hard ceiling on armed cancellations, so a client that spams stop for turns
/// that never start cannot grow the map without bound.
const MAX_PENDING_CANCELS: usize = 256;

#[derive(Debug, Default)]
struct TurnCancellationState {
    conversation_id: String,
    cancel_requested: AtomicBool,
}

#[derive(Debug)]
struct PendingCancel {
    conversation_id: String,
    requested_at: Instant,
}

/// Turns currently in flight, and cancellations that arrived before their turn
/// registered. Both live under one lock: a stop press racing the start of the
/// turn it targets must land on exactly one of the two maps, never slip between
/// them and be lost.
#[derive(Default)]
struct Registry {
    turns: HashMap<String, Arc<TurnCancellationState>>,
    pending: HashMap<String, PendingCancel>,
}

static CANCELLATION_REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));

fn lock() -> std::sync::MutexGuard<'static, Registry> {
    match CANCELLATION_REGISTRY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn prune_pending(pending: &mut HashMap<String, PendingCancel>) {
    let now = Instant::now();
    pending.retain(|_, entry| now.duration_since(entry.requested_at) < PENDING_CANCEL_TTL);
    while pending.len() >= MAX_PENDING_CANCELS {
        let Some(oldest) = pending
            .iter()
            .min_by_key(|(_, entry)| entry.requested_at)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        pending.remove(&oldest);
    }
}

// Each entry is one turn, keyed by its request id. A conversation may have
// more than one turn only when an older/non-store caller overlaps requests;
// those turns must remain independently cancellable and independently owned.
pub(super) fn begin_turn(request_id: &str, conversation_id: &str) -> bool {
    let mut registry = lock();
    if registry.turns.contains_key(request_id) {
        return false;
    }
    // A stop press that arrived before this turn registered still applies to it:
    // start the turn already cancelled rather than running a generation the user
    // has asked for twice over.
    let already_cancelled = registry
        .pending
        .remove(request_id)
        .is_some_and(|entry| entry.conversation_id == conversation_id);
    registry.turns.insert(
        request_id.to_string(),
        Arc::new(TurnCancellationState {
            conversation_id: conversation_id.to_string(),
            cancel_requested: AtomicBool::new(already_cancelled),
        }),
    );
    true
}

pub(super) fn finish_turn(request_id: &str) {
    let mut registry = lock();
    registry.turns.remove(request_id);
    registry.pending.remove(request_id);
}

pub(super) fn is_cancel_requested(request_id: &str) -> bool {
    let registry = lock();
    registry
        .turns
        .get(request_id)
        .is_some_and(|state| state.cancel_requested.load(Ordering::SeqCst))
}

pub(super) fn request_cancel(conversation_id: &str, request_id: Option<&str>) -> bool {
    let mut registry = lock();

    if let Some(request_id) = request_id {
        if let Some(state) = registry.turns.get(request_id) {
            if state.conversation_id != conversation_id {
                return false;
            }
            state.cancel_requested.store(true, Ordering::SeqCst);
            return true;
        }
        // The turn has not registered yet — it is still validating input,
        // loading the model, or opening the conversation. Arm the cancellation
        // so `begin_turn` honours it, instead of reporting an idle conversation
        // and leaving the user's stop press with no effect at all.
        prune_pending(&mut registry.pending);
        registry.pending.insert(
            request_id.to_string(),
            PendingCancel {
                conversation_id: conversation_id.to_string(),
                requested_at: Instant::now(),
            },
        );
        return true;
    }

    // Compatibility for older callers that do not yet send a request id.
    // Cancel every matching turn rather than selecting an arbitrary HashMap
    // entry and potentially leaving an overlapping generation unstoppable.
    // Without a request id there is nothing to arm, so a turn that has not
    // registered yet cannot be reached this way.
    let mut cancelled = false;
    for state in registry
        .turns
        .values()
        .filter(|state| state.conversation_id == conversation_id)
    {
        state.cancel_requested.store(true, Ordering::SeqCst);
        cancelled = true;
    }
    cancelled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_turns_are_cancelled_and_finished_independently() {
        let suffix = uuid::Uuid::new_v4();
        let conversation_id = format!("conversation-{suffix}");
        let first = format!("request-a-{suffix}");
        let second = format!("request-b-{suffix}");

        assert!(begin_turn(&first, &conversation_id));
        assert!(begin_turn(&second, &conversation_id));
        assert!(request_cancel(&conversation_id, Some(&first)));
        assert!(is_cancel_requested(&first));
        assert!(!is_cancel_requested(&second));

        finish_turn(&first);
        assert!(!is_cancel_requested(&first));
        assert!(request_cancel(&conversation_id, Some(&second)));
        assert!(is_cancel_requested(&second));

        finish_turn(&second);
    }

    #[test]
    fn request_id_cannot_cancel_a_different_conversation() {
        let suffix = uuid::Uuid::new_v4();
        let request_id = format!("request-{suffix}");
        let owner = format!("owner-{suffix}");
        let other = format!("other-{suffix}");

        assert!(begin_turn(&request_id, &owner));
        assert!(!request_cancel(&other, Some(&request_id)));
        assert!(!is_cancel_requested(&request_id));
        finish_turn(&request_id);
    }

    #[test]
    fn cancel_before_the_turn_registers_still_stops_it() {
        let suffix = uuid::Uuid::new_v4();
        let conversation_id = format!("conversation-{suffix}");
        let request_id = format!("request-{suffix}");

        // The stop press lands while the turn is still loading the model.
        assert!(request_cancel(&conversation_id, Some(&request_id)));

        assert!(begin_turn(&request_id, &conversation_id));
        assert!(is_cancel_requested(&request_id));
        finish_turn(&request_id);
    }

    #[test]
    fn armed_cancellation_does_not_leak_to_another_conversation() {
        let suffix = uuid::Uuid::new_v4();
        let request_id = format!("request-{suffix}");
        let cancelled = format!("cancelled-{suffix}");
        let other = format!("other-{suffix}");

        assert!(request_cancel(&cancelled, Some(&request_id)));
        assert!(begin_turn(&request_id, &other));
        assert!(!is_cancel_requested(&request_id));
        finish_turn(&request_id);
    }

    #[test]
    fn a_finished_turn_does_not_arm_a_later_one() {
        let suffix = uuid::Uuid::new_v4();
        let conversation_id = format!("conversation-{suffix}");
        let request_id = format!("request-{suffix}");

        assert!(begin_turn(&request_id, &conversation_id));
        finish_turn(&request_id);
        // A stop press that arrives after the turn ended arms nothing that a
        // later turn with the same id would inherit.
        assert!(request_cancel(&conversation_id, Some(&request_id)));
        finish_turn(&request_id);

        assert!(begin_turn(&request_id, &conversation_id));
        assert!(!is_cancel_requested(&request_id));
        finish_turn(&request_id);
    }
}

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct TurnCancellationState {
    conversation_id: String,
    cancel_requested: AtomicBool,
}

// Each entry is one turn, keyed by its request id. A conversation may have
// more than one turn only when an older/non-store caller overlaps requests;
// those turns must remain independently cancellable and independently owned.
static CANCELLATION_REGISTRY: Lazy<Mutex<HashMap<String, Arc<TurnCancellationState>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub(super) fn begin_turn(request_id: &str, conversation_id: &str) -> bool {
    let mut registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    if registry.contains_key(request_id) {
        return false;
    }
    registry.insert(
        request_id.to_string(),
        Arc::new(TurnCancellationState {
            conversation_id: conversation_id.to_string(),
            cancel_requested: AtomicBool::new(false),
        }),
    );
    true
}

pub(super) fn finish_turn(request_id: &str) {
    let mut registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    registry.remove(request_id);
}

pub(super) fn is_cancel_requested(request_id: &str) -> bool {
    let registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    registry
        .get(request_id)
        .is_some_and(|state| state.cancel_requested.load(Ordering::SeqCst))
}

pub(super) fn request_cancel(conversation_id: &str, request_id: Option<&str>) -> bool {
    let registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(request_id) = request_id {
        if let Some(state) = registry.get(request_id) {
            if state.conversation_id != conversation_id {
                return false;
            }
            state.cancel_requested.store(true, Ordering::SeqCst);
            return true;
        }
        return false;
    }

    // Compatibility for older callers that do not yet send a request id.
    // Cancel every matching turn rather than selecting an arbitrary HashMap
    // entry and potentially leaving an overlapping generation unstoppable.
    let mut cancelled = false;
    for state in registry
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
}

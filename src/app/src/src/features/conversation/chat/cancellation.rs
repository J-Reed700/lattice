use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct TurnCancellationState {
    in_flight: AtomicBool,
    cancel_requested: AtomicBool,
}

static CANCELLATION_REGISTRY: Lazy<Mutex<HashMap<String, Arc<TurnCancellationState>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn get_or_create_state(conversation_id: &str) -> Arc<TurnCancellationState> {
    let mut registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    registry
        .entry(conversation_id.to_string())
        .or_insert_with(|| Arc::new(TurnCancellationState::default()))
        .clone()
}

pub(super) fn begin_turn(conversation_id: &str) {
    let state = get_or_create_state(conversation_id);
    state.cancel_requested.store(false, Ordering::SeqCst);
    state.in_flight.store(true, Ordering::SeqCst);
}

pub(super) fn finish_turn(conversation_id: &str) {
    let mut registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(state) = registry.remove(conversation_id) {
        state.cancel_requested.store(false, Ordering::SeqCst);
        state.in_flight.store(false, Ordering::SeqCst);
    }
}

pub(super) fn is_cancel_requested(conversation_id: &str) -> bool {
    let registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(state) = registry.get(conversation_id) {
        state.in_flight.load(Ordering::SeqCst) && state.cancel_requested.load(Ordering::SeqCst)
    } else {
        false
    }
}

pub(super) fn request_cancel(conversation_id: &str) -> bool {
    let registry = match CANCELLATION_REGISTRY.lock() {
        Ok(lock) => lock,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(state) = registry.get(conversation_id) {
        if !state.in_flight.load(Ordering::SeqCst) {
            return false;
        }
        state.cancel_requested.store(true, Ordering::SeqCst);
        return true;
    }
    false
}

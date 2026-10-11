//! One scheduler per backend, shared by every role that reaches it.
//!
//! The chat, router and utility roles each load their own port, but two roles
//! on one server compete for the same slots, so they must queue in the same
//! place. Local llama-servers carry their scheduler on the process handle;
//! remote endpoints, Ollama and the cloud providers are keyed here. Entries are
//! weak: a scheduler lives exactly as long as some port still uses it.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;

use super::{BackendCapacity, InferenceScheduler};

/// Requests in flight to one cloud provider. Their capacity is not ours to
/// measure; this keeps a burst of background work from tripping rate limits
/// while still letting a turn and its judge run side by side.
const CLOUD_CONCURRENCY: usize = 8;

/// `/props` answers instantly on a healthy server.
const PROPS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, PartialEq, Eq, Hash)]
enum BackendKey {
    LlamaCpp(String),
    Ollama(String),
    Cloud(String),
}

static SHARED: Lazy<Mutex<HashMap<BackendKey, Weak<InferenceScheduler>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn live(key: &BackendKey) -> Option<Arc<InferenceScheduler>> {
    SHARED.lock().get(key).and_then(Weak::upgrade)
}

fn shared(key: BackendKey, make: impl FnOnce() -> InferenceScheduler) -> Arc<InferenceScheduler> {
    let mut shared = SHARED.lock();
    shared.retain(|_, scheduler| scheduler.strong_count() > 0);
    if let Some(live) = shared.get(&key).and_then(Weak::upgrade) {
        return live;
    }
    let scheduler = Arc::new(make());
    shared.insert(key, Arc::downgrade(&scheduler));
    scheduler
}

/// The scheduler for one Ollama endpoint. Ollama serves requests side by side
/// up to its own `OLLAMA_NUM_PARALLEL`; `concurrency` mirrors what we allow it.
pub(crate) fn ollama_scheduler(endpoint: &str, concurrency: usize) -> Arc<InferenceScheduler> {
    shared(BackendKey::Ollama(endpoint.to_owned()), || {
        InferenceScheduler::new(
            format!("ollama {endpoint}"),
            BackendCapacity::concurrent(concurrency),
        )
    })
}

/// The scheduler for one cloud provider.
pub(crate) fn cloud_scheduler(provider: &str) -> Arc<InferenceScheduler> {
    shared(BackendKey::Cloud(provider.to_owned()), || {
        InferenceScheduler::new(provider, BackendCapacity::concurrent(CLOUD_CONCURRENCY))
    })
}

/// The scheduler for a remote llama.cpp server, measured from its `/props`
/// the first time any role connects. A server that does not answer `/props`
/// is not known to be llama-server: it gets one slot and no slot pinning, so
/// nothing it might reject is sent.
pub(crate) async fn remote_llama_cpp_scheduler(
    http: &reqwest::Client,
    server_root: &str,
    configured_window: usize,
) -> (Arc<InferenceScheduler>, bool) {
    let key = BackendKey::LlamaCpp(server_root.to_owned());
    if let Some(live) = live(&key) {
        let is_llama_server = live.capacity().pins_slots;
        return (live, is_llama_server);
    }
    let props = llama_server_capacity(http, server_root).await;
    let capacity = match props {
        Some((slots, window)) => {
            BackendCapacity::llama_server(slots, window.unwrap_or(configured_window))
        }
        None => BackendCapacity {
            slots: 1,
            shared_window: Some(configured_window.max(1)),
            pins_slots: false,
        },
    };
    tracing::info!(
        server = server_root,
        slots = capacity.slots,
        window = ?capacity.shared_window,
        pins_slots = capacity.pins_slots,
        "Remote llama.cpp scheduler sized"
    );
    let scheduler = shared(key, || {
        InferenceScheduler::new(format!("llama.cpp {server_root}"), capacity)
    });
    let is_llama_server = scheduler.capacity().pins_slots;
    (scheduler, is_llama_server)
}

/// A llama-server's slot count and per-slot context window, from `GET /props`.
/// `None` when the server does not answer like a llama-server.
pub(crate) async fn llama_server_capacity(
    http: &reqwest::Client,
    server_root: &str,
) -> Option<(usize, Option<usize>)> {
    let url = format!("{}/props", server_root.trim_end_matches('/'));
    let response = http.get(url).timeout(PROPS_TIMEOUT).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    parse_props(&body)
}

/// `total_slots`, and `n_ctx` from `default_generation_settings` (or the top
/// level, where some builds put it). With a unified KV cache every slot
/// reports the whole window, which is the window the slots share.
pub(super) fn parse_props(body: &Value) -> Option<(usize, Option<usize>)> {
    let slots = body.get("total_slots").and_then(Value::as_u64)?;
    let window = body
        .pointer("/default_generation_settings/n_ctx")
        .or_else(|| body.get("n_ctx"))
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n > 0);
    Some((usize::try_from(slots).ok()?.max(1), window))
}

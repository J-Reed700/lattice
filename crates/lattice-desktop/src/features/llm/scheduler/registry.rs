//! One scheduler per backend, shared by every role that reaches it.
//!
//! The chat, router and utility roles each load their own port, but two roles
//! on one server compete for the same slots, so they must queue in the same
//! place. Local llama-servers carry their scheduler on the process handle;
//! remote endpoints, Ollama and the cloud providers are keyed in a
//! [`SchedulerRegistry`], one per app, which every model load is handed.
//! Entries are weak: a scheduler lives exactly as long as some port still
//! uses it.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::watch;

use super::{BackendCapacity, InferenceScheduler};

/// `/props` answers instantly on a healthy server.
const PROPS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, PartialEq, Eq, Hash)]
enum BackendKey {
    LlamaCpp(String),
    Ollama(String),
    Cloud(String),
}

/// The schedulers of the backends this app's models run on, keyed by backend.
///
/// The container owns one and every model load is handed it, so two roles on
/// one backend queue in one place however they were loaded.
pub(crate) struct SchedulerRegistry {
    shared: Mutex<HashMap<BackendKey, Weak<InferenceScheduler>>>,
    /// Cached `/props` thinking capability per live remote llama-server URL,
    /// so subsequent role loads share the initial `/props` probe.
    remote_thinking: Mutex<HashMap<String, (Weak<InferenceScheduler>, bool)>>,
    /// Moved by every scheduler this registry made, local ones included,
    /// whenever its queue changes.
    changed: Arc<watch::Sender<u64>>,
}

impl Default for SchedulerRegistry {
    fn default() -> Self {
        Self {
            shared: Mutex::default(),
            remote_thinking: Mutex::default(),
            changed: Arc::new(watch::channel(0).0),
        }
    }
}

impl SchedulerRegistry {
    /// Moves whenever a request joins, leaves or is admitted on any backend.
    pub(crate) fn changes(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// A scheduler that tells this registry's watchers of its changes: one
    /// shared here, or one a local server's process carries.
    pub(crate) fn scheduler(
        &self,
        label: impl Into<String>,
        capacity: BackendCapacity,
    ) -> InferenceScheduler {
        InferenceScheduler::observed(label, capacity, Arc::clone(&self.changed))
    }

    fn live(&self, key: &BackendKey) -> Option<Arc<InferenceScheduler>> {
        self.shared.lock().get(key).and_then(Weak::upgrade)
    }

    fn shared(
        &self,
        key: BackendKey,
        make: impl FnOnce() -> InferenceScheduler,
    ) -> Arc<InferenceScheduler> {
        let mut shared = self.shared.lock();
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
    pub(crate) fn ollama(&self, endpoint: &str, concurrency: usize) -> Arc<InferenceScheduler> {
        self.shared(BackendKey::Ollama(endpoint.to_owned()), || {
            self.scheduler(
                format!("ollama {endpoint}"),
                BackendCapacity::concurrent(concurrency),
            )
        })
    }

    /// The scheduler for one cloud provider. Its capacity is whatever the
    /// provider's answers allow; see [`BackendCapacity::adaptive`].
    pub(crate) fn cloud(&self, provider: &str) -> Arc<InferenceScheduler> {
        self.shared(BackendKey::Cloud(provider.to_owned()), || {
            self.scheduler(provider, BackendCapacity::adaptive())
        })
    }

    /// The scheduler for a remote llama.cpp server, measured from its `/props`
    /// the first time any role connects. A server that does not answer `/props`
    /// is not known to be llama-server: it gets one slot and no slot pinning, so
    /// nothing it might reject is sent.
    ///
    /// Returns `(scheduler, is_llama_server, supports_thinking)` where
    /// `supports_thinking` comes from `/props`'s `chat_template` (`false` when
    /// omitted or unavailable, never guessed from the model name).
    pub(crate) async fn remote_llama_cpp(
        &self,
        http: &reqwest::Client,
        server_root: &str,
        configured_window: usize,
    ) -> (Arc<InferenceScheduler>, bool, bool) {
        let key = BackendKey::LlamaCpp(server_root.to_owned());
        if let Some(live) = self.live(&key) {
            let is_llama_server = live.capacity().pins_slots;
            let supports_thinking = self
                .remote_thinking
                .lock()
                .get(server_root)
                .filter(|(owner, _)| owner.as_ptr() == Arc::as_ptr(&live))
                .map(|(_, supports)| *supports)
                .unwrap_or(false);
            return (live, is_llama_server, supports_thinking);
        }
        let props = llama_server_props(http, server_root).await;
        let supports_thinking = props
            .as_ref()
            .and_then(|props| props.supports_thinking)
            .unwrap_or(false);
        let capacity = match props {
            Some(props) => BackendCapacity::llama_server(
                props.slots,
                props.window.unwrap_or(configured_window),
            ),
            None => BackendCapacity {
                slots: 1,
                shared_window: Some(configured_window.max(1)),
                pins_slots: false,
                adaptive: false,
            },
        };
        tracing::info!(
            server = server_root,
            slots = capacity.slots,
            window = ?capacity.shared_window,
            pins_slots = capacity.pins_slots,
            supports_thinking,
            "Remote llama.cpp scheduler sized"
        );
        let scheduler = self.shared(key, || {
            self.scheduler(format!("llama.cpp {server_root}"), capacity)
        });
        {
            let mut thinking = self.remote_thinking.lock();
            thinking.retain(|_, (owner, _)| owner.strong_count() > 0);
            thinking.insert(
                server_root.to_owned(),
                (Arc::downgrade(&scheduler), supports_thinking),
            );
        }
        let is_llama_server = scheduler.capacity().pins_slots;
        (scheduler, is_llama_server, supports_thinking)
    }
}

/// What `GET /props` reports about a llama-server: slot count, per-slot
/// context window, and whether its `chat_template` declares native thinking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LlamaServerProps {
    pub slots: usize,
    pub window: Option<usize>,
    /// `None` when `/props` omitted `chat_template`.
    pub supports_thinking: Option<bool>,
}

/// A llama-server's slot count, per-slot context window, and `chat_template`
/// thinking capability from `GET /props`. `None` when the server does not
/// answer like a llama-server.
pub(crate) async fn llama_server_props(
    http: &reqwest::Client,
    server_root: &str,
) -> Option<LlamaServerProps> {
    let url = format!("{}/props", server_root.trim_end_matches('/'));
    let response = http.get(url).timeout(PROPS_TIMEOUT).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: Value = response.json().await.ok()?;
    let (slots, window) = parse_props(&body)?;
    Some(LlamaServerProps {
        slots,
        window,
        supports_thinking: parse_props_thinking(&body),
    })
}

/// A llama-server's slot count and per-slot context window, from `GET /props`.
/// `None` when the server does not answer like a llama-server.
pub(crate) async fn llama_server_capacity(
    http: &reqwest::Client,
    server_root: &str,
) -> Option<(usize, Option<usize>)> {
    let props = llama_server_props(http, server_root).await?;
    Some((props.slots, props.window))
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

/// Whether `GET /props`'s `chat_template` declares native thinking markers,
/// using the same detector as local GGUF headers. `None` when `/props` omits
/// `chat_template`.
pub(super) fn parse_props_thinking(body: &Value) -> Option<bool> {
    body.get("chat_template")
        .and_then(Value::as_str)
        .map(crate::features::llm::engine::gguf_metadata::chat_template_supports_thinking)
}

#[cfg(test)]
mod props_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_props_thinking_detects_template_markers_and_handles_missing_field() {
        assert_eq!(
            parse_props_thinking(&json!({
                "total_slots": 2,
                "chat_template": "{% if enable_thinking is defined %}<think>{% endif %}"
            })),
            Some(true)
        );
        assert_eq!(
            parse_props_thinking(&json!({
                "total_slots": 2,
                "chat_template": "{{ bos_token }}<think>{{ reasoning }}</think>"
            })),
            Some(true)
        );
        assert_eq!(
            parse_props_thinking(&json!({
                "total_slots": 2,
                "chat_template": "{{ bos_token }}{% for m in messages %}{{ m.content }}{% endfor %}"
            })),
            Some(false)
        );
        assert_eq!(parse_props_thinking(&json!({"total_slots": 2})), None);
    }
}

//! Which loaded models a change makes stale.

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::ports::llm_port::InvalidationCause;
use lattice_core::domain::models::model_ref::ModelRef;

use super::ModelKey;
use crate::features::llm::loading::launch_profile_for;

/// Whether `cause` makes the model loaded for `key` stale.
pub(super) fn matches(key: &ModelKey, cause: &InvalidationCause) -> bool {
    match cause {
        InvalidationCause::ModelDeleted { model_id } => {
            matches!(&key.model, ModelRef::Local { model_id: id } if id == model_id)
        }
        InvalidationCause::CredentialsChanged { provider } => {
            matches!(&key.model, ModelRef::Cloud { provider: p, .. } if p == provider)
        }
        InvalidationCause::SettingsChanged {
            previous: Some(previous),
            current: Some(current),
        } => client_changed(key, previous, current),
        InvalidationCause::SettingsChanged { .. } => true,
    }
}

/// A name for `cause` in logs.
pub(super) fn describe(cause: &InvalidationCause) -> &'static str {
    match cause {
        InvalidationCause::ModelDeleted { .. } => "model_deleted",
        InvalidationCause::SettingsChanged { .. } => "settings_changed",
        InvalidationCause::CredentialsChanged { .. } => "credentials_changed",
    }
}

/// Whether the settings `key`'s client and server were built from differ
/// between `before` and `after`.
///
/// What a role resolves to — the provider, the chat model, the active row, the
/// Learning model, the router's or utility's model name — is not among them:
/// a request resolves its model when it is made, so changing what it resolves
/// to unloads nothing, and the model it no longer names is evicted only when
/// its memory is wanted.
fn client_changed(key: &ModelKey, before: &LLMSettingsDto, after: &LLMSettingsDto) -> bool {
    // Every client is built with its generation settings baked in.
    if key.tuning.generation(before) != key.tuning.generation(after) {
        return true;
    }
    match &key.model {
        // The model's own saved configuration and Settings' server options
        // make up its server; the key carries the launch it was started with.
        ModelRef::Local { .. } => launch_profile_for(&key.model, after) != key.launch,
        ModelRef::Ollama { .. } => {
            before.ollama_url != after.ollama_url
                || before.ollama_auth_header_name != after.ollama_auth_header_name
                || before.ollama_auth_header_value != after.ollama_auth_header_value
        }
        ModelRef::LlamaCpp { .. } => {
            before.llama_cpp.url != after.llama_cpp.url
                || before.llama_cpp.auth_header_name != after.llama_cpp.auth_header_name
                || before.llama_cpp.auth_header_value != after.llama_cpp.auth_header_value
                || before.context_window != after.context_window
                || before.timeout_seconds != after.timeout_seconds
        }
        ModelRef::Cloud { .. } => {
            before.context_window != after.context_window
                || before.timeout_seconds != after.timeout_seconds
        }
    }
}

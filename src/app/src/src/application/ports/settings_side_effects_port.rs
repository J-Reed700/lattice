//! Side-effects port for settings mutations.
//!
//! When a settings update (or reset) lands, certain in-memory caches in
//! Container need to be invalidated so the next request picks up the
//! new config:
//!
//! - LLM client cache (`llm_cache`) — model path / GPU layers / chat
//!   model selection may have changed.
//! - Router LLM client cache — same, for the optional small router
//!   model used by the adaptive RAG path.
//! - Function executor's custom-tool registry — mirrors
//!   `settings.llm.custom_tools` for runtime tool dispatch.
//!
//! # Why a port (and why not the existing event bus)
//!
//! Audit P0-3 (2026-04-29) flagged the original implementation: the
//! Tauri command `update_settings` invoked
//! `container.invalidate_llm_cache()` directly *after* the use case
//! returned. Any non-IPC caller of `UpdateSettingsUseCase` (internal
//! Rust code, tests, future migration scripts) would silently leak
//! stale caches because the side effect lived only in the plugin
//! layer — outside the SSOT boundary.
//!
//! Two reasonable fixes:
//!
//! 1. Publish a `SettingsUpdated` event on the existing `EventBus<T>`
//!    and add a subscriber-saga that owns invalidation.
//! 2. Inject this port into the use cases. Use case calls
//!    `on_settings_updated(category)` after a successful write.
//!
//! We picked (2) because cache invalidation is an *intra-process*
//! concern with one consumer (Container). The event bus exists for
//! cross-feature broadcast (model downloads, conversation summaries);
//! using it here would add a new bus type, a saga loop, and async
//! plumbing for what is fundamentally three direct method calls.
//!
//! Tests pass `MockSettingsSideEffects` which records calls so the
//! "update settings → invalidate caches" invariant can be asserted
//! at the use-case layer without spinning up the full DI container.

use crate::features::settings::dto::SettingsCategory;
use async_trait::async_trait;

/// Hints emitted by the use case when something policy-relevant
/// transitioned in this update. Lets the side-effect impl react to
/// transitions without having to read both before/after states itself.
#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsTransitionHints {
    /// True iff `vault.enabled` flipped from false to true in this
    /// update — triggers the one-shot backfill of existing notes into
    /// the vault folder. Use case sets this; side-effect impl reacts.
    pub vault_just_enabled: bool,
}

/// Port for settings-mutation side effects (cache invalidation,
/// runtime configuration refresh).
///
/// Implementors must be `Send + Sync` and tolerate being called from
/// async contexts. Implementations should be best-effort — a side
/// effect that fails should log a warning but not propagate the
/// error to the caller. The user's settings change has already been
/// persisted by the time this is called; rolling back due to a cache
/// invalidation failure would be worse than continuing.
#[async_trait]
pub trait SettingsSideEffectsPort: Send + Sync {
    /// Notify that settings have been updated. `category` is `Some`
    /// when only a single category was modified (the common case),
    /// `None` for a global update or full reset. `hints` carries
    /// transition flags the use case computed before/after the write.
    ///
    /// Implementations decide which side effects to fire based on
    /// the category and hints. The port doesn't dictate the policy;
    /// Container's impl owns it.
    async fn on_settings_updated(
        &self,
        category: Option<SettingsCategory>,
        hints: SettingsTransitionHints,
    );
}

/// No-op implementation for tests that don't care about side effects.
pub struct NoopSettingsSideEffects;

#[async_trait]
impl SettingsSideEffectsPort for NoopSettingsSideEffects {
    async fn on_settings_updated(
        &self,
        _category: Option<SettingsCategory>,
        _hints: SettingsTransitionHints,
    ) {
    }
}

/// Recording mock for tests that want to assert the use case invokes
/// the port. Stores every category passed to `on_settings_updated` in
/// order; tests inspect via `calls()`.
#[cfg(test)]
pub struct RecordingSettingsSideEffects {
    calls: parking_lot::Mutex<Vec<Option<SettingsCategory>>>,
}

#[cfg(test)]
impl RecordingSettingsSideEffects {
    pub fn new() -> Self {
        Self {
            calls: parking_lot::Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<Option<SettingsCategory>> {
        self.calls.lock().clone()
    }
}

#[cfg(test)]
impl Default for RecordingSettingsSideEffects {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[async_trait]
impl SettingsSideEffectsPort for RecordingSettingsSideEffects {
    async fn on_settings_updated(
        &self,
        category: Option<SettingsCategory>,
        _hints: SettingsTransitionHints,
    ) {
        self.calls.lock().push(category);
    }
}

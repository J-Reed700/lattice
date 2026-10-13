//! Cache-invalidation + transition reactions fired by settings mutation use cases
//! after a successful write. Implementations are best-effort —
//! failures must not propagate, the user's setting has already been
//! persisted. The one exception is `check_vault_enable`, which runs before
//! the write so it can still refuse it.

use crate::application::contracts::settings::SettingsCategory;
use crate::shared::error::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, Default)]
pub struct SettingsTransitionHints {
    /// `vault.enabled` flipped false → true in this update.
    pub vault_just_enabled: bool,
}

#[async_trait]
pub trait SettingsSideEffectsPort: Send + Sync {
    /// `category` is `Some` for single-category updates, `None` for
    /// global update, import, or reset.
    async fn on_settings_updated(
        &self,
        category: Option<SettingsCategory>,
        hints: SettingsTransitionHints,
    );

    /// Runs before an update that turns the vault on. An error refuses the
    /// update and nothing is written. `vault_path` is the configured folder,
    /// empty for the default. Accepts every folder unless overridden.
    async fn check_vault_enable(&self, _vault_path: &str) -> Result<()> {
        Ok(())
    }
}

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

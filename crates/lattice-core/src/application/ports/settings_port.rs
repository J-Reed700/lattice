//! # Settings Repository Port
//!
//! Port interface for settings storage and retrieval.
//!
//! This port abstracts settings persistence, allowing different implementations
//! (file-based, database, remote, etc.) while keeping the application layer independent.

use crate::application::contracts::settings::{SettingsCategory, SettingsDto};
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Recursively merges a partial JSON settings update into an existing value.
/// Object fields are preserved unless explicitly replaced by the update.
pub fn merge_json_update(target: &mut serde_json::Value, update: serde_json::Value) {
    match (target, update) {
        (serde_json::Value::Object(target), serde_json::Value::Object(update)) => {
            for (key, value) in update {
                match target.get_mut(&key) {
                    Some(existing) => merge_json_update(existing, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, update) => *target = update,
    }
}

/// Port interface for settings repository.
///
/// Implementations handle persistence of application settings,
/// supporting read, write, reset, import, and export operations.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` for concurrent access.
#[async_trait]
pub trait SettingsRepositoryPort: Send + Sync {
    /// Get all settings.
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be loaded
    async fn get_all(&self) -> Result<SettingsDto>;

    /// Get a specific settings category.
    ///
    /// # Errors
    ///
    /// Returns error if category doesn't exist or cannot be loaded
    async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value>;

    /// Save all settings.
    ///
    /// # Errors
    ///
    /// Returns error if settings cannot be persisted
    async fn save_all(&self, settings: &SettingsDto) -> Result<()>;

    /// Update specific settings fields.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Invalid category
    /// - Invalid field names
    /// - Invalid field values
    /// - Cannot save updated settings
    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto>;

    /// Reset settings to defaults.
    ///
    /// # Errors
    ///
    /// Returns error if reset cannot be saved
    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto>;

    /// Export settings to a file.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Path is invalid
    /// - Cannot write to path
    /// - Cannot serialize settings
    async fn export(&self, path: &str) -> Result<()>;

    /// Import settings from a file.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File doesn't exist
    /// - Cannot read file
    /// - Invalid JSON format
    /// - Invalid settings structure
    async fn import(&self, path: &str, merge: bool) -> Result<SettingsImportOutcome>;
}

/// The committed import and its vault transition, observed in one serialized mutation.
pub struct SettingsImportOutcome {
    pub settings: SettingsDto,
    pub vault_just_enabled: bool,
}

/// Mock implementation for testing.
#[cfg(any(test, feature = "test-support"))]
pub struct MockSettingsRepository {
    settings: tokio::sync::RwLock<SettingsDto>,
}

#[cfg(any(test, feature = "test-support"))]
impl MockSettingsRepository {
    /// Create a new mock repository with default settings.
    pub fn new() -> Self {
        Self {
            settings: tokio::sync::RwLock::new(SettingsDto::default()),
        }
    }

    /// Create with custom initial settings.
    pub fn with_settings(settings: SettingsDto) -> Self {
        Self {
            settings: tokio::sync::RwLock::new(settings),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Default for MockSettingsRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl SettingsRepositoryPort for MockSettingsRepository {
    async fn get_all(&self) -> Result<SettingsDto> {
        Ok(self.settings.read().await.clone())
    }

    async fn get_category(&self, category: SettingsCategory) -> Result<serde_json::Value> {
        let settings = self.settings.read().await;
        let value = match category {
            SettingsCategory::Indexing => serde_json::to_value(&settings.indexing)?,
            SettingsCategory::Search => serde_json::to_value(&settings.search)?,
            SettingsCategory::Llm => serde_json::to_value(&settings.llm)?,
            SettingsCategory::Ui => serde_json::to_value(&settings.ui)?,
            SettingsCategory::Sync => serde_json::to_value(&settings.sync)?,
            SettingsCategory::Backup => serde_json::to_value(&settings.backup)?,
            SettingsCategory::Privacy => serde_json::to_value(&settings.privacy)?,
            SettingsCategory::Vault => serde_json::to_value(&settings.vault)?,
            SettingsCategory::Onboarding => serde_json::to_value(&settings.onboarding)?,
            SettingsCategory::Sourcing => serde_json::to_value(&settings.sourcing)?,
        };
        Ok(value)
    }

    async fn save_all(&self, settings: &SettingsDto) -> Result<()> {
        *self.settings.write().await = settings.clone();
        Ok(())
    }

    async fn update(
        &self,
        category: Option<SettingsCategory>,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<SettingsDto> {
        let mut settings = self.settings.write().await;

        match category {
            Some(cat) => {
                let category_value = match cat {
                    SettingsCategory::Indexing => serde_json::to_value(&settings.indexing)?,
                    SettingsCategory::Search => serde_json::to_value(&settings.search)?,
                    SettingsCategory::Llm => serde_json::to_value(&settings.llm)?,
                    SettingsCategory::Ui => serde_json::to_value(&settings.ui)?,
                    SettingsCategory::Sync => serde_json::to_value(&settings.sync)?,
                    SettingsCategory::Backup => serde_json::to_value(&settings.backup)?,
                    SettingsCategory::Privacy => serde_json::to_value(&settings.privacy)?,
                    SettingsCategory::Vault => serde_json::to_value(&settings.vault)?,
                    SettingsCategory::Onboarding => serde_json::to_value(&settings.onboarding)?,
                    SettingsCategory::Sourcing => serde_json::to_value(&settings.sourcing)?,
                };

                let mut category_map = category_value
                    .as_object()
                    .ok_or_else(|| {
                        crate::shared::error::AppError::InvalidInput(
                            "Invalid category structure".to_string(),
                        )
                    })?
                    .clone();
                for (key, value) in updates {
                    match category_map.get_mut(&key) {
                        Some(existing) => merge_json_update(existing, value),
                        None => {
                            category_map.insert(key, value);
                        }
                    }
                }

                match cat {
                    SettingsCategory::Indexing => {
                        settings.indexing =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Search => {
                        settings.search =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Llm => {
                        settings.llm =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Ui => {
                        settings.ui =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Sync => {
                        settings.sync =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Backup => {
                        settings.backup =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Privacy => {
                        settings.privacy =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Vault => {
                        settings.vault =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Onboarding => {
                        settings.onboarding =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                    SettingsCategory::Sourcing => {
                        settings.sourcing =
                            serde_json::from_value(serde_json::Value::Object(category_map))?;
                    }
                }
            }
            None => {
                let mut settings_value = serde_json::to_value(&*settings)?;
                let settings_map = settings_value.as_object_mut().ok_or_else(|| {
                    crate::shared::error::AppError::InvalidInput(
                        "Invalid settings structure".to_string(),
                    )
                })?;

                for (key, value) in updates {
                    match settings_map.get_mut(&key) {
                        Some(existing) => merge_json_update(existing, value),
                        None => {
                            settings_map.insert(key, value);
                        }
                    }
                }

                *settings =
                    serde_json::from_value(serde_json::Value::Object(settings_map.clone()))?;
            }
        }

        Ok(settings.clone())
    }

    async fn reset(&self, category: Option<SettingsCategory>) -> Result<SettingsDto> {
        let mut settings = self.settings.write().await;

        match category {
            Some(cat) => match cat {
                SettingsCategory::Indexing => settings.indexing = Default::default(),
                SettingsCategory::Search => settings.search = Default::default(),
                SettingsCategory::Llm => settings.llm = Default::default(),
                SettingsCategory::Ui => settings.ui = Default::default(),
                SettingsCategory::Sync => settings.sync = Default::default(),
                SettingsCategory::Backup => settings.backup = Default::default(),
                SettingsCategory::Privacy => settings.privacy = Default::default(),
                SettingsCategory::Vault => settings.vault = Default::default(),
                SettingsCategory::Onboarding => settings.onboarding = Default::default(),
                SettingsCategory::Sourcing => settings.sourcing = Default::default(),
            },
            None => {
                *settings = SettingsDto::default();
            }
        }

        Ok(settings.clone())
    }

    async fn export(&self, _path: &str) -> Result<()> {
        Ok(())
    }

    async fn import(&self, _path: &str, _merge: bool) -> Result<SettingsImportOutcome> {
        Ok(SettingsImportOutcome {
            settings: self.settings.read().await.clone(),
            vault_just_enabled: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_repository_get_all() {
        let repo = MockSettingsRepository::new();
        let settings = repo.get_all().await.unwrap();

        assert_eq!(settings.indexing.chunk_size, 800);
        assert_eq!(settings.search.max_results, 10);
    }

    #[tokio::test]
    async fn test_mock_repository_get_category() {
        let repo = MockSettingsRepository::new();
        let category = repo.get_category(SettingsCategory::Search).await.unwrap();

        assert!(category.is_object());
        assert_eq!(category["maxResults"], 10);
    }

    #[tokio::test]
    async fn test_mock_repository_update() {
        let repo = MockSettingsRepository::new();
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), serde_json::json!(20));

        let updated = repo
            .update(Some(SettingsCategory::Search), updates)
            .await
            .unwrap();

        assert_eq!(updated.search.max_results, 20);
    }

    #[tokio::test]
    async fn test_mock_repository_reset() {
        let repo = MockSettingsRepository::new();

        // Modify settings
        let mut updates = HashMap::new();
        updates.insert("maxResults".to_string(), serde_json::json!(50));
        repo.update(Some(SettingsCategory::Search), updates)
            .await
            .unwrap();

        let reset = repo.reset(Some(SettingsCategory::Search)).await.unwrap();

        assert_eq!(reset.search.max_results, 10); // Back to default
    }
}

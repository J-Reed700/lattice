//! Import Settings Use Case

use crate::application::ports::settings_side_effects_port::SettingsTransitionHints;
use crate::application::ports::{
    NoopSettingsSideEffects, SettingsRepositoryPort, SettingsSideEffectsPort,
};
use crate::features::settings::dto::{ImportSettingsRequestDto, ImportSettingsResponseDto};
use crate::shared::error::Result;
use std::path::Path;
use std::sync::Arc;

/// Use case for importing application settings from a file.
///
/// # Responsibilities
///
/// - Import settings from JSON file
/// - Support merge or replace mode
/// - Validate imported settings
/// - Handle file I/O errors
pub struct ImportSettingsUseCase {
    repository: Arc<dyn SettingsRepositoryPort>,
    side_effects: Arc<dyn SettingsSideEffectsPort>,
}

impl ImportSettingsUseCase {
    /// Create a new use case instance.
    ///
    /// # Arguments
    ///
    /// * `repository` - Settings repository port implementation
    pub fn new(repository: Arc<dyn SettingsRepositoryPort>) -> Self {
        Self::with_side_effects(repository, Arc::new(NoopSettingsSideEffects))
    }

    pub fn with_side_effects(
        repository: Arc<dyn SettingsRepositoryPort>,
        side_effects: Arc<dyn SettingsSideEffectsPort>,
    ) -> Self {
        Self {
            repository,
            side_effects,
        }
    }

    /// Execute the use case to import settings.
    ///
    /// # Arguments
    ///
    /// * `request` - Import request with source path and merge flag
    ///
    /// # Returns
    ///
    /// Import response with updated settings and status
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File doesn't exist
    /// - Cannot read file
    /// - Invalid JSON format
    /// - Invalid settings structure
    /// - Settings validation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let use_case = ImportSettingsUseCase::new(repository);
    ///
    /// // Replace all settings
    /// let request = ImportSettingsRequestDto {
    ///     path: "/home/user/settings-backup.json".to_string(),
    ///     merge: false,
    /// };
    /// let response = use_case.execute(request).await?;
    ///
    /// // Merge with existing settings
    /// let request = ImportSettingsRequestDto {
    ///     path: "/home/user/partial-settings.json".to_string(),
    ///     merge: true,
    /// };
    /// let response = use_case.execute(request).await?;
    /// ```
    pub async fn execute(
        &self,
        request: ImportSettingsRequestDto,
    ) -> Result<ImportSettingsResponseDto> {
        self.validate_import_path(&request.path)?;

        // Import validates and persists once, with merge and secret restoration
        // serialized against other settings writes. Never save its result again.
        let imported = self.repository.import(&request.path, request.merge).await?;
        self.side_effects
            .on_settings_updated(
                None,
                SettingsTransitionHints {
                    vault_just_enabled: imported.vault_just_enabled,
                },
            )
            .await;

        let status = if request.merge {
            "Settings imported and merged successfully"
        } else {
            "Settings imported successfully"
        };

        Ok(ImportSettingsResponseDto {
            settings: imported.settings,
            status: status.to_string(),
        })
    }

    /// Validate the import path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to validate
    ///
    /// # Errors
    ///
    /// Returns error if path is empty or file doesn't exist
    fn validate_import_path(&self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(crate::shared::error::AppError::InvalidInput(
                "Import path cannot be empty".to_string(),
            ));
        }

        let path_buf = Path::new(path);

        // repository-barrier-allow: import consumes the user-selected file itself.
        if !path_buf.exists() {
            return Err(crate::shared::error::AppError::InvalidInput(format!(
                "Import file does not exist: {}",
                path
            )));
        }

        // repository-barrier-allow: import requires that selected resource to be a regular file.
        if !path_buf.is_file() {
            return Err(crate::shared::error::AppError::InvalidInput(format!(
                "Import path is not a file: {}",
                path
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::settings::{
        dto::{SettingsCategory, SettingsDto},
        repository::SettingsRepository,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;

    struct ObservedEffects {
        repository: Arc<SettingsRepository>,
        observed: Mutex<Vec<(Option<SettingsCategory>, bool, SettingsDto)>>,
    }
    #[async_trait]
    impl SettingsSideEffectsPort for ObservedEffects {
        async fn on_settings_updated(
            &self,
            category: Option<SettingsCategory>,
            hints: SettingsTransitionHints,
        ) {
            let saved = self.repository.get_all().await.unwrap();
            self.observed
                .lock()
                .push((category, hints.vault_just_enabled, saved));
        }
    }

    #[tokio::test]
    async fn successful_import_refreshes_runtime_after_persist_in_both_modes() {
        for merge in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let repository = Arc::new(
                SettingsRepository::new(dir.path().to_path_buf())
                    .await
                    .unwrap(),
            );
            let effects = Arc::new(ObservedEffects {
                repository: repository.clone(),
                observed: Mutex::new(vec![]),
            });
            let use_case =
                ImportSettingsUseCase::with_side_effects(repository.clone(), effects.clone());
            let mut imported = repository.get_all().await.unwrap();
            imported.search.max_results = 42;
            imported.llm.ollama_url = "http://localhost:11435".into();
            imported.vault.enabled = true;
            let path = dir.path().join("import.json");
            tokio::fs::write(&path, serde_json::to_vec(&imported).unwrap())
                .await
                .unwrap();
            let response = use_case
                .execute(ImportSettingsRequestDto {
                    path: path.to_string_lossy().into(),
                    merge,
                })
                .await
                .unwrap();
            assert_eq!(response.settings.search.max_results, 42);
            let observed = effects.observed.lock();
            assert_eq!(observed.len(), 1);
            assert_eq!(
                observed[0].0, None,
                "all runtime settings must be refreshed"
            );
            assert_eq!(
                observed[0].1, !merge,
                "merge preserves the existing vault category"
            );
            assert_eq!(observed[0].2.llm.ollama_url, imported.llm.ollama_url);
            assert_eq!(
                observed[0].2.search.max_results, 42,
                "effects must see committed settings"
            );
        }
    }

    #[tokio::test]
    async fn invalid_import_neither_persists_nor_refreshes_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let repository = Arc::new(
            SettingsRepository::new(dir.path().to_path_buf())
                .await
                .unwrap(),
        );
        let effects = Arc::new(ObservedEffects {
            repository: repository.clone(),
            observed: Mutex::new(vec![]),
        });
        let use_case =
            ImportSettingsUseCase::with_side_effects(repository.clone(), effects.clone());
        let before = serde_json::to_value(repository.get_all().await.unwrap()).unwrap();
        let mut imported = repository.get_all().await.unwrap();
        imported.indexing.chunk_size = 0;
        let path = dir.path().join("invalid.json");
        tokio::fs::write(&path, serde_json::to_vec(&imported).unwrap())
            .await
            .unwrap();
        assert!(use_case
            .execute(ImportSettingsRequestDto {
                path: path.to_string_lossy().into(),
                merge: false
            })
            .await
            .is_err());
        assert!(effects.observed.lock().is_empty());
        assert_eq!(
            serde_json::to_value(repository.get_all().await.unwrap()).unwrap(),
            before
        );
    }

    #[tokio::test]
    async fn import_requires_an_existing_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let repository = Arc::new(
            SettingsRepository::new(dir.path().to_path_buf())
                .await
                .unwrap(),
        );
        let use_case = ImportSettingsUseCase::new(repository);
        assert!(use_case.validate_import_path("").is_err());
        assert!(use_case
            .validate_import_path(dir.path().join("missing.json").to_str().unwrap())
            .is_err());
        assert!(use_case
            .validate_import_path(dir.path().to_str().unwrap())
            .is_err());
    }
}

//! Settings feature dependency injection.

use std::sync::Arc;

use crate::application::ports::SettingsRepositoryPort;
use crate::features::settings::repository::SettingsRepository;
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use std::path::Path;

#[derive(Clone)]
pub struct SettingsDi {
    pub settings_repo: Arc<dyn SettingsRepositoryPort>,
    pub get_settings_use_case: Arc<GetSettingsUseCase>,
    pub update_settings_use_case: Arc<UpdateSettingsUseCase>,
    pub reset_settings_use_case: Arc<ResetSettingsUseCase>,
    pub export_settings_use_case: Arc<ExportSettingsUseCase>,
    pub import_settings_use_case: Arc<ImportSettingsUseCase>,
    pub validate_settings_use_case: Arc<ValidateSettingsUseCase>,
}

pub async fn build(settings_path: &Path) -> Result<SettingsDi> {
    let settings_repo = Arc::new(SettingsRepository::new(settings_path.to_path_buf()).await?)
        as Arc<dyn SettingsRepositoryPort>;

    Ok(SettingsDi {
        get_settings_use_case: Arc::new(GetSettingsUseCase::new(settings_repo.clone())),
        update_settings_use_case: Arc::new(UpdateSettingsUseCase::new(settings_repo.clone())),
        reset_settings_use_case: Arc::new(ResetSettingsUseCase::new(settings_repo.clone())),
        export_settings_use_case: Arc::new(ExportSettingsUseCase::new(settings_repo.clone())),
        import_settings_use_case: Arc::new(ImportSettingsUseCase::new(settings_repo.clone())),
        validate_settings_use_case: Arc::new(ValidateSettingsUseCase::new(settings_repo.clone())),
        settings_repo,
    })
}

/// Settings' registrar surface on `Container`.
impl Container {
    // Settings (from SystemModule)
    pub fn get_settings_use_case(&self) -> Arc<GetSettingsUseCase> {
        Arc::clone(self.system.get_settings_use_case())
    }

    /// Settings update use case, wired with the side-effects port so
    /// LLM cache invalidation fires after a successful write.
    ///
    /// Constructs a fresh use case on each call rather than returning
    /// the pre-built `Arc` from `SystemModule`. SystemModule's stored
    /// instance is built before the side-effects port exists
    /// (bootstrap order: System → AI → side-effects), so it has only
    /// noop side effects. Production code paths should always go
    /// through Container, never directly through SystemModule.
    /// Audit P0-3 fix.
    pub fn update_settings_use_case(&self) -> Arc<UpdateSettingsUseCase> {
        Arc::new(UpdateSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    /// Settings reset use case, wired with the side-effects port. See
    /// `update_settings_use_case` for the bootstrap-order rationale.
    /// Audit P0-3 fix: reset previously did NOT invalidate any caches,
    /// so a "reset to defaults" left stale LLM caches until app
    /// restart.
    pub fn reset_settings_use_case(&self) -> Arc<ResetSettingsUseCase> {
        Arc::new(ResetSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    pub fn export_settings_use_case(&self) -> Arc<ExportSettingsUseCase> {
        Arc::clone(self.system.export_settings_use_case())
    }

    pub fn import_settings_use_case(&self) -> Arc<ImportSettingsUseCase> {
        Arc::new(ImportSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    pub fn validate_settings_use_case(&self) -> Arc<ValidateSettingsUseCase> {
        Arc::clone(self.system.validate_settings_use_case())
    }
}

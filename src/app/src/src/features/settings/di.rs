//! Settings feature dependency injection.

use std::sync::Arc;

use crate::application::ports::SettingsRepositoryPort;
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};
use crate::infrastructure::persistence::repositories::SettingsRepository;
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

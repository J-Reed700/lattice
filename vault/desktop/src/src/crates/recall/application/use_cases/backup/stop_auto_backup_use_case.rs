use crate::application::dtos::settings::SettingsCategory;
use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::shared::error::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

pub struct StopAutoBackupUseCase {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    backup_scheduler: Arc<dyn BackupSchedulerPort>,
}

impl StopAutoBackupUseCase {
    pub fn new(
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        backup_scheduler: Arc<dyn BackupSchedulerPort>,
    ) -> Self {
        Self {
            settings_repo,
            backup_scheduler,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let mut updates = HashMap::new();
        updates.insert("autoBackupEnabled".to_string(), json!(false));

        self.settings_repo
            .update(Some(SettingsCategory::Backup), updates)
            .await?;

        self.backup_scheduler.stop().await?;
        Ok(())
    }
}

use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::settings::dto::SettingsCategory;
use crate::shared::error::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

pub struct StartAutoBackupUseCase {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    backup_scheduler: Arc<dyn BackupSchedulerPort>,
}

impl StartAutoBackupUseCase {
    pub fn new(
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        backup_scheduler: Arc<dyn BackupSchedulerPort>,
    ) -> Self {
        Self {
            settings_repo,
            backup_scheduler,
        }
    }

    pub async fn execute(&self, schedule: String) -> Result<()> {
        let interval = super::parse_backup_schedule(&schedule)?;

        let mut updates = HashMap::new();
        updates.insert("autoBackupEnabled".to_string(), json!(true));
        updates.insert("backupFrequency".to_string(), json!(schedule));

        self.settings_repo
            .update(Some(SettingsCategory::Backup), updates)
            .await?;

        self.backup_scheduler.start(interval).await?;
        Ok(())
    }
}

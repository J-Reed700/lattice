use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::backup::archive::service::ArchiveService;
use crate::features::settings::dto::SettingsCategory;
use crate::shared::error::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

pub struct StopAutoBackupUseCase {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    backup_scheduler: Arc<dyn BackupSchedulerPort>,
    archive_service: Arc<ArchiveService>,
}

impl StopAutoBackupUseCase {
    pub fn new(
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        backup_scheduler: Arc<dyn BackupSchedulerPort>,
        archive_service: Arc<ArchiveService>,
    ) -> Self {
        Self {
            settings_repo,
            backup_scheduler,
            archive_service,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let mut updates = HashMap::new();
        updates.insert("autoBackupEnabled".to_string(), json!(false));

        self.settings_repo
            .update(Some(SettingsCategory::Backup), updates)
            .await?;

        // The ticker also drives the off-device archive; only stop it when
        // nothing else needs it. The scheduler reads the flag each tick, so
        // the local snapshot stops immediately either way.
        if !self.archive_service.is_active().await {
            self.backup_scheduler.stop().await?;
        }
        Ok(())
    }
}

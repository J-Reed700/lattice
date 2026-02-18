use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::shared::error::Result;
use std::sync::Arc;

pub struct StartupAutoBackupUseCase {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    backup_scheduler: Arc<dyn BackupSchedulerPort>,
}

impl StartupAutoBackupUseCase {
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
        let settings = self.settings_repo.get_all().await?;
        if !settings.backup.auto_backup_enabled {
            return Ok(());
        }

        let interval = super::parse_backup_schedule(&settings.backup.backup_frequency)?;
        self.backup_scheduler.start(interval).await?;
        Ok(())
    }
}

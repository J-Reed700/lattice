//! Starts the backup ticker at launch (and after archive setup) when either
//! the local auto-backup or the off-device archive is switched on.

use crate::application::ports::{BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::backup::archive::service::ArchiveService;
use crate::shared::error::Result;
use std::sync::Arc;

pub struct StartupAutoBackupUseCase {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    backup_scheduler: Arc<dyn BackupSchedulerPort>,
    archive_service: Arc<ArchiveService>,
}

impl StartupAutoBackupUseCase {
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
        let settings = self.settings_repo.get_all().await?;
        let archive_active = self.archive_service.is_active().await;
        if !settings.backup.auto_backup_enabled && !archive_active {
            return Ok(());
        }

        // The archive shares the local schedule; when only the archive is on
        // and the frequency string is unset or invalid, fall back to daily
        // rather than silently never backing up.
        let interval = match super::parse_backup_schedule(&settings.backup.backup_frequency) {
            Ok(interval) => interval,
            Err(e) if archive_active && !settings.backup.auto_backup_enabled => {
                tracing::warn!(error = %e, "Invalid backup frequency; archiving daily");
                std::time::Duration::from_secs(60 * 60 * 24)
            }
            Err(e) => return Err(e),
        };
        self.backup_scheduler.start(interval).await?;
        Ok(())
    }
}

//! Backup feature dependency injection.

use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{BackupPort, BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::backup::adapter::BackupAdapter;
use crate::features::backup::scheduler::BackupScheduler;
use crate::features::backup::use_cases::{
    CreateBackupUseCase, ListBackupsUseCase, RestoreBackupUseCase, StartAutoBackupUseCase,
    StartupAutoBackupUseCase, StopAutoBackupUseCase,
};

#[derive(Clone)]
pub struct BackupDi {
    pub backup: Arc<dyn BackupPort>,
    pub backup_scheduler: Arc<BackupScheduler>,
    pub create_backup_use_case: Arc<CreateBackupUseCase>,
    pub restore_backup_use_case: Arc<RestoreBackupUseCase>,
    pub list_backups_use_case: Arc<ListBackupsUseCase>,
    pub start_auto_backup_use_case: Arc<StartAutoBackupUseCase>,
    pub stop_auto_backup_use_case: Arc<StopAutoBackupUseCase>,
    pub startup_auto_backup_use_case: Arc<StartupAutoBackupUseCase>,
}

pub fn build(
    db_pool: SqlitePool,
    db_path: PathBuf,
    settings_repo: Arc<dyn SettingsRepositoryPort>,
) -> BackupDi {
    let backup = Arc::new(BackupAdapter::new(db_pool, db_path)) as Arc<dyn BackupPort>;

    let create_backup_use_case = Arc::new(CreateBackupUseCase::new(backup.clone()));
    let backup_scheduler = Arc::new(BackupScheduler::new(
        create_backup_use_case.clone(),
        settings_repo.clone(),
    ));
    let scheduler_port: Arc<dyn BackupSchedulerPort> = backup_scheduler.clone();

    BackupDi {
        restore_backup_use_case: Arc::new(RestoreBackupUseCase::new(backup.clone())),
        list_backups_use_case: Arc::new(ListBackupsUseCase::new(backup.clone())),
        start_auto_backup_use_case: Arc::new(StartAutoBackupUseCase::new(
            settings_repo.clone(),
            scheduler_port.clone(),
        )),
        stop_auto_backup_use_case: Arc::new(StopAutoBackupUseCase::new(
            settings_repo.clone(),
            scheduler_port.clone(),
        )),
        startup_auto_backup_use_case: Arc::new(StartupAutoBackupUseCase::new(
            settings_repo,
            scheduler_port,
        )),
        create_backup_use_case,
        backup_scheduler,
        backup,
    }
}

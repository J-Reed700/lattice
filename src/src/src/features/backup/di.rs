//! Backup feature dependency injection.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{BackupPort, BackupSchedulerPort, SettingsRepositoryPort};
use crate::features::backup::adapter::BackupAdapter;
use crate::features::backup::archive::key_store::MasterKeyStore;
use crate::features::backup::archive::restore::ArchiveRestorer;
use crate::features::backup::archive::service::ArchiveService;
use crate::features::backup::archive::writer::ArchiveWriter;
use crate::features::backup::scheduler::BackupScheduler;
use crate::features::backup::use_cases::{
    CreateBackupUseCase, RestoreBackupUseCase, StartAutoBackupUseCase, StartupAutoBackupUseCase,
    StopAutoBackupUseCase,
};
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct BackupDi {
    pub backup: Arc<dyn BackupPort>,
    pub backup_scheduler: Arc<BackupScheduler>,
    pub create_backup_use_case: Arc<CreateBackupUseCase>,
    pub restore_backup_use_case: Arc<RestoreBackupUseCase>,
    pub start_auto_backup_use_case: Arc<StartAutoBackupUseCase>,
    pub stop_auto_backup_use_case: Arc<StopAutoBackupUseCase>,
    pub startup_auto_backup_use_case: Arc<StartupAutoBackupUseCase>,
    /// Encrypted off-device archives. Owns the setup wizard's in-memory
    /// state, so there must be exactly one of these per app.
    pub archive_service: Arc<ArchiveService>,
}

pub fn build(
    db_pool: SqlitePool,
    db_path: PathBuf,
    app_data_dir: PathBuf,
    settings_repo: Arc<dyn SettingsRepositoryPort>,
) -> BackupDi {
    let backup =
        Arc::new(BackupAdapter::new(db_pool.clone(), db_path.clone())) as Arc<dyn BackupPort>;

    let key_store = Arc::new(MasterKeyStore::new(app_data_dir.clone()));
    // Resolved once at wiring time. `Err` only means "no home directory",
    // in which case there is no files library to archive either.
    let files_root =
        crate::infrastructure::storage::ContentAddressedStorage::default_library_root().ok();
    let archive_writer = Arc::new(ArchiveWriter::new(
        db_pool.clone(),
        app_data_dir.clone(),
        settings_repo.clone(),
        key_store.clone(),
        files_root.clone(),
    ));
    let db_dir = db_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| app_data_dir.clone());
    let archive_restorer = Arc::new(ArchiveRestorer::new(
        db_pool,
        db_path,
        app_data_dir.clone(),
        settings_repo.clone(),
        key_store.clone(),
        files_root,
    ));
    let archive_service = Arc::new(ArchiveService::new(
        archive_writer,
        archive_restorer,
        key_store,
        app_data_dir,
        db_dir,
    ));

    let create_backup_use_case = Arc::new(CreateBackupUseCase::new(backup.clone()));
    let backup_scheduler = Arc::new(BackupScheduler::new(
        create_backup_use_case.clone(),
        settings_repo.clone(),
        archive_service.clone(),
    ));
    let scheduler_port: Arc<dyn BackupSchedulerPort> = backup_scheduler.clone();

    BackupDi {
        restore_backup_use_case: Arc::new(RestoreBackupUseCase::new(backup.clone())),
        start_auto_backup_use_case: Arc::new(StartAutoBackupUseCase::new(
            settings_repo.clone(),
            scheduler_port.clone(),
        )),
        stop_auto_backup_use_case: Arc::new(StopAutoBackupUseCase::new(
            settings_repo.clone(),
            scheduler_port.clone(),
            archive_service.clone(),
        )),
        startup_auto_backup_use_case: Arc::new(StartupAutoBackupUseCase::new(
            settings_repo,
            scheduler_port,
            archive_service.clone(),
        )),
        create_backup_use_case,
        backup_scheduler,
        archive_service,
        backup,
    }
}

/// Backup's registrar surface on `Container`.
impl Container {
    // Backup (from SystemModule)
    pub fn create_backup_use_case(&self) -> Arc<CreateBackupUseCase> {
        Arc::clone(self.system.create_backup_use_case())
    }

    pub fn restore_backup_use_case(&self) -> Arc<RestoreBackupUseCase> {
        Arc::clone(self.system.restore_backup_use_case())
    }

    /// The backup port. `plugin_list_backups` reads through this rather than
    /// the free `list_backups_impl`, which scans the wrong directory.
    pub fn backup_port(&self) -> Arc<dyn BackupPort> {
        Arc::clone(self.system.backup_port())
    }

    pub fn start_auto_backup_use_case(&self) -> Arc<StartAutoBackupUseCase> {
        Arc::clone(self.system.start_auto_backup_use_case())
    }

    pub fn stop_auto_backup_use_case(&self) -> Arc<StopAutoBackupUseCase> {
        Arc::clone(self.system.stop_auto_backup_use_case())
    }

    pub fn startup_auto_backup_use_case(&self) -> Arc<StartupAutoBackupUseCase> {
        Arc::clone(self.system.startup_auto_backup_use_case())
    }

    /// Encrypted off-device archive orchestration.
    pub fn archive_service(&self) -> Arc<ArchiveService> {
        Arc::clone(self.system.archive_service())
    }
}

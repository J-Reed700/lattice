//! The system module: health, initialization, the model manager and settings.

use super::CoreModule;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - Health
use crate::features::health::use_cases::HealthCheckUseCase;
use crate::features::initialization::use_cases::{
    InitializeDatabaseUseCase, InitializeModelsUseCase,
};

// Application Use Cases - Settings
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};

// (Cache: no use cases — commands operate on the container's query cache.)

// Application Use Cases - Backup
use crate::features::backup::use_cases::{
    CreateBackupUseCase, RestoreBackupUseCase, StartAutoBackupUseCase, StartupAutoBackupUseCase,
    StopAutoBackupUseCase,
};

// Application Use Cases - Updates
use crate::features::updates::use_cases::{CheckForUpdatesUseCase, GetCurrentVersionUseCase};

// Application Use Cases - Metrics
use crate::features::metrics::use_cases::GetMetricsUseCase;

// Application Use Cases - Stats
use crate::features::stats::use_cases::{GetCorpusShapeUseCase, GetSystemStatsUseCase};
use crate::infrastructure::observability::metrics::Metrics;

// Application Ports
use crate::application::ports::{BackupPort, SettingsRepositoryPort, SystemInfoPort};

// Service Traits
use crate::features::model_management::model_manager::ModelManager;
use crate::infrastructure::services::traits::ModelManagerTrait;

/// System module for application-wide operations
///
/// The cache feature has no DI of its own: its commands operate on the query
/// cache the container holds.
#[derive(Clone)]
pub struct SystemModule {
    settings: crate::features::settings::di::SettingsDi,
    backup: crate::features::backup::di::BackupDi,
    updates: crate::features::updates::di::UpdatesDi,
    metrics: crate::features::metrics::di::MetricsDi,
    health: crate::features::health::di::HealthDi,
    stats: crate::features::stats::di::StatsDi,
    initialization: crate::features::initialization::di::InitializationDi,

    // Retained, not just handed to initialization: the reranker is downloaded
    // on demand rather than at boot, so a command needs to reach the manager
    // long after startup is over.
    model_manager: Arc<dyn ModelManagerTrait>,

    // System-info adapter stays here — it's a shared infra concern
    // with no dedicated feature slice.
    system_info: Arc<dyn SystemInfoPort>,
}

impl SystemModule {
    /// Build SystemModule by composing system-level feature builders.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        backend_devices: Arc<crate::features::llm::engine::system::BackendDevices>,
    ) -> crate::shared::error::Result<Self> {
        use crate::infrastructure::adapters::system_info::SystemInfoAdapter;

        let settings = crate::features::settings::di::build(core.data_dir()).await?;

        let db_path = core.data_dir().join("lattice.db");
        let backup = crate::features::backup::di::build(
            db_pool.clone(),
            db_path,
            core.data_dir().to_path_buf(),
            settings.settings_repo.clone(),
            Arc::clone(core.background()),
        );

        let models_path = core.data_dir().join("models");
        let model_manager = Arc::new(ModelManager::new(models_path)?) as Arc<dyn ModelManagerTrait>;
        let initialization =
            crate::features::initialization::di::build(db_pool.clone(), model_manager.clone());

        Ok(Self {
            settings,
            backup,
            updates: crate::features::updates::di::build(),
            metrics: crate::features::metrics::di::build(),
            health: crate::features::health::di::build(db_pool.clone()),
            stats: crate::features::stats::di::build(db_pool),
            initialization,
            model_manager,
            system_info: Arc::new(SystemInfoAdapter::new(backend_devices))
                as Arc<dyn SystemInfoPort>,
        })
    }

    // Health use case getters
    pub fn health_check_use_case(&self) -> &Arc<HealthCheckUseCase> {
        &self.health.health_check_use_case
    }

    pub fn initialize_database_use_case(&self) -> &Arc<InitializeDatabaseUseCase> {
        &self.initialization.initialize_database_use_case
    }

    pub fn initialize_models_use_case(&self) -> &Arc<InitializeModelsUseCase> {
        &self.initialization.initialize_models_use_case
    }

    pub fn model_manager(&self) -> &Arc<dyn ModelManagerTrait> {
        &self.model_manager
    }

    // Settings use case getters
    pub fn get_settings_use_case(&self) -> &Arc<GetSettingsUseCase> {
        &self.settings.get_settings_use_case
    }

    pub fn update_settings_use_case(&self) -> &Arc<UpdateSettingsUseCase> {
        &self.settings.update_settings_use_case
    }

    pub fn reset_settings_use_case(&self) -> &Arc<ResetSettingsUseCase> {
        &self.settings.reset_settings_use_case
    }

    pub fn export_settings_use_case(&self) -> &Arc<ExportSettingsUseCase> {
        &self.settings.export_settings_use_case
    }

    pub fn import_settings_use_case(&self) -> &Arc<ImportSettingsUseCase> {
        &self.settings.import_settings_use_case
    }

    pub fn validate_settings_use_case(&self) -> &Arc<ValidateSettingsUseCase> {
        &self.settings.validate_settings_use_case
    }

    /// Settings repository access. Used by Container to wire the
    /// settings-side-effects port (audit P0-3).
    pub fn settings_repo(&self) -> &Arc<dyn SettingsRepositoryPort> {
        &self.settings.settings_repo
    }

    // (Cache: no use cases — commands operate on the container's query cache.)

    // Backup use case getters
    pub fn create_backup_use_case(&self) -> &Arc<CreateBackupUseCase> {
        &self.backup.create_backup_use_case
    }

    pub fn restore_backup_use_case(&self) -> &Arc<RestoreBackupUseCase> {
        &self.backup.restore_backup_use_case
    }

    pub fn start_auto_backup_use_case(&self) -> &Arc<StartAutoBackupUseCase> {
        &self.backup.start_auto_backup_use_case
    }

    pub fn stop_auto_backup_use_case(&self) -> &Arc<StopAutoBackupUseCase> {
        &self.backup.stop_auto_backup_use_case
    }

    pub fn startup_auto_backup_use_case(&self) -> &Arc<StartupAutoBackupUseCase> {
        &self.backup.startup_auto_backup_use_case
    }

    pub fn backup_scheduler(&self) -> &Arc<crate::features::backup::scheduler::BackupScheduler> {
        &self.backup.backup_scheduler
    }

    /// The backup port itself, for reads that have no use case
    /// (`plugin_list_backups`).
    pub fn backup_port(&self) -> &Arc<dyn BackupPort> {
        &self.backup.backup
    }

    /// Encrypted off-device archive orchestration. Single instance: it holds
    /// the setup wizard's in-memory key until the user confirms it.
    pub fn archive_service(
        &self,
    ) -> &Arc<crate::features::backup::archive::service::ArchiveService> {
        &self.backup.archive_service
    }

    // Updates use case getters
    pub fn check_for_updates_use_case(&self) -> &Arc<CheckForUpdatesUseCase> {
        &self.updates.check_for_updates_use_case
    }

    pub fn get_current_version_use_case(&self) -> &Arc<GetCurrentVersionUseCase> {
        &self.updates.get_current_version_use_case
    }

    // Metrics use case getters
    pub fn get_metrics_use_case(&self) -> &Arc<GetMetricsUseCase> {
        &self.metrics.get_metrics_use_case
    }

    // Stats use case getters
    pub fn get_system_stats_use_case(&self) -> &Arc<GetSystemStatsUseCase> {
        &self.stats.get_system_stats_use_case
    }

    pub fn get_corpus_shape_use_case(&self) -> &Arc<GetCorpusShapeUseCase> {
        &self.stats.get_corpus_shape_use_case
    }

    // Adapter getters needed by Container
    pub fn system_info(&self) -> &Arc<dyn SystemInfoPort> {
        &self.system_info
    }

    pub fn metrics_service(&self) -> &Arc<Metrics> {
        &self.metrics.metrics_service
    }
}

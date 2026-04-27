//! Backup feature — use cases.
//!
//! `list` was removed — `backup/commands.rs::list_backups_impl` calls
//! `BackupAdapter::list_backups` directly.

pub mod create;
pub mod restore;
pub mod start_auto;
pub mod startup_auto;
pub mod stop_auto;

pub use create::CreateBackupUseCase;
pub use restore::RestoreBackupUseCase;
pub use start_auto::StartAutoBackupUseCase;
pub use startup_auto::StartupAutoBackupUseCase;
pub use stop_auto::StopAutoBackupUseCase;

use crate::shared::error::AppError;
use std::time::Duration;

pub(crate) fn parse_backup_schedule(schedule: &str) -> Result<Duration, AppError> {
    match schedule.to_lowercase().as_str() {
        "hourly" => Ok(Duration::from_secs(60 * 60)),
        "daily" => Ok(Duration::from_secs(60 * 60 * 24)),
        "weekly" => Ok(Duration::from_secs(60 * 60 * 24 * 7)),
        "monthly" => Ok(Duration::from_secs(60 * 60 * 24 * 30)),
        _ => Err(AppError::InvalidInput(format!(
            "Unsupported backup schedule '{}'. Use hourly, daily, weekly, or monthly.",
            schedule
        ))),
    }
}

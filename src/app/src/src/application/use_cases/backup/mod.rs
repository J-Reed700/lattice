pub mod create_backup_use_case;
pub mod list_backups_use_case;
pub mod restore_backup_use_case;
pub mod start_auto_backup_use_case;
pub mod startup_auto_backup_use_case;
pub mod stop_auto_backup_use_case;

pub use create_backup_use_case::CreateBackupUseCase;
pub use list_backups_use_case::ListBackupsUseCase;
pub use restore_backup_use_case::RestoreBackupUseCase;
pub use start_auto_backup_use_case::StartAutoBackupUseCase;
pub use startup_auto_backup_use_case::StartupAutoBackupUseCase;
pub use stop_auto_backup_use_case::StopAutoBackupUseCase;

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

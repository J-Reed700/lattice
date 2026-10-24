//! Start/stop the scheduled auto-backup job.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

/// Enables automatic scheduled backups.
pub async fn start_auto_backup(
    schedule: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    container
        .system
        .start_auto_backup_use_case()
        .execute(schedule)
        .await?;
    Ok(())
}

/// Disables automatic scheduled backups.
pub async fn stop_auto_backup(container: State<'_, Container>) -> Result<(), AppError> {
    container
        .system
        .stop_auto_backup_use_case()
        .execute()
        .await?;
    Ok(())
}

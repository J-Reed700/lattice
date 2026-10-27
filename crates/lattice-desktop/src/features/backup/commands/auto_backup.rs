//! Start/stop the scheduled auto-backup job.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

/// Enables automatic scheduled backups.
///
/// Delegates to `StartAutoBackupUseCase`, which validates the schedule, persists
/// settings, and starts the background scheduler.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid schedule format
/// * `AppError::Other` - Failed to start scheduler or persist settings
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

/// Disables automatic scheduled backups
///
/// # Errors
///
/// * `AppError::Other` - Failed to stop scheduler or persistence error
///
/// # Behavior
///
/// 1. Cancel any active scheduled backup tasks
/// 2. Clear auto-backup flag in settings
/// 3. Preserve existing backup files
/// 4. Update UI state to reflect disabled status
///
/// # Important Notes
///
/// - **Existing Backups Preserved**: Stopping auto-backup does NOT delete existing backup files
/// - **Manual Backups Still Work**: Users can still create backups manually via `create_backup()`
/// - **Re-enabling**: Call `start_auto_backup()` with schedule to re-enable
///
/// # Architecture
///
/// Command flow:
/// 1. Stop scheduler via `StopAutoBackupUseCase`
/// 2. Update backup settings
/// 3. Return success
pub async fn stop_auto_backup(container: State<'_, Container>) -> Result<(), AppError> {
    container
        .system
        .stop_auto_backup_use_case()
        .execute()
        .await?;
    Ok(())
}

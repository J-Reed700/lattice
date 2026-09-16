//! Start/stop the scheduled auto-backup job.

use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use tauri::State;

/// Enables automatic scheduled backups.
///
/// Delegates to `StartAutoBackupUseCase`, which validates the schedule, persists
/// settings, and starts the background scheduler.
///
/// # Arguments
///
/// * `schedule` - Schedule string (e.g., `"daily"`, `"weekly"`, `"0 2 * * *"`)
///
/// # Returns
///
/// * `Ok(())` - Scheduler started and settings updated
/// * `Err(AppError)` - Validation or scheduler error
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid schedule format
/// * `AppError::Other` - Failed to start scheduler or persist settings
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Enable daily backups at 2 AM
/// await invoke('start_auto_backup', {
///   schedule: 'daily'
/// });
///
/// // Use cron expression for custom schedule
/// await invoke('start_auto_backup', {
///   schedule: '0 2 * * *'  // 2 AM every day
/// });
/// ```
///
/// # Command Flow
///
/// 1. Validate schedule format
/// 2. Persist schedule to settings
/// 3. Start background scheduler
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
/// # Returns
///
/// * `Ok(())` - Scheduler stopped and settings updated
/// * `Err(AppError)` - Stop or persistence error
///
/// # Errors
///
/// * `AppError::Other` - Failed to stop scheduler or persistence error
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Disable automatic backups
/// await invoke('stop_auto_backup');
///
/// console.log('Auto-backup disabled');
/// ```
///
/// # Behavior
///
/// 1. Cancel any active scheduled backup tasks
/// 2. Clear auto-backup flag in settings
/// 3. Preserve existing backup files
/// 4. Update UI state to reflect disabled status
///
/// # Use Cases
///
/// - **Temporary Disable**: Stop scheduled backups during maintenance
/// - **Storage Management**: Disable when disk space is low
/// - **Manual Control**: Switch to manual backup workflow
/// - **Testing**: Disable auto-backup during development/testing
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

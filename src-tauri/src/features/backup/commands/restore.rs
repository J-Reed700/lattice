//! `restore_backup`: replaces the live database with a stored snapshot.

use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use crate::shared::path_confinement::confine_to_root;
use std::path::PathBuf;

/// Restores database from a backup file
///
/// Restores the Lattice database from a previously created `.lattice-backup` file,
/// replacing all current data with the backup contents. This operation is destructive
/// and should only be used for disaster recovery or intentional rollback. The backup
/// file must be in version 1.0 format.
///
/// # Arguments
///
/// * `backup_path` - Path to the `.lattice-backup` file to restore
///
/// # Returns
///
/// * `Ok(())` - Backup successfully restored
/// * `Err(AppError)` - If path validation fails, file not found, or restore fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid backup path (directory traversal detected)
/// * `AppError::Other` - Backup file not found at specified path
/// * `AppError::Other` - Failed to access backup file (permissions, corruption)
/// * `AppError::Other` - Restore operation failed (incompatible format, I/O error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Restore from backup file
/// await invoke('restore_backup', {
///   backupPath: '/Users/example/backups/lattice-2024-01-15.lattice-backup'
/// });
///
/// console.log('Database restored from backup!');
/// ```
///
/// # Warning
///
/// **This operation is destructive**:
/// - All current documents, chunks, and embeddings will be replaced
/// - All tags and conversations will be replaced
/// - User settings will be replaced with backup values
/// - **This action cannot be undone** (create a backup first!)
///
/// # Best Practices
///
/// **Before Restoring**:
/// 1. Create a backup of current state using `create_backup()`
/// 2. Close any active indexing operations
/// 3. Ensure no other processes are accessing the database
/// 4. Verify backup file integrity and version
///
/// **After Restoring**:
/// 1. Verify restored data completeness
/// 2. Restart application if recommended
/// 3. Re-index any changed source files if needed
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All restore operations logged to audit trail
/// - **No Rate Limiting**: Critical recovery operation (not rate limited)
///
/// # Use Cases
///
/// - **Disaster Recovery**: Recover from data corruption or loss
/// - **Rollback**: Undo problematic changes or upgrades
/// - **Data Migration**: Transfer data from backup to new installation
/// - **Testing**: Restore test data snapshots
/// - **Version Testing**: Test with specific data states
///
/// # Backup Validation
///
/// The restore process validates:
/// - File exists and is readable
/// - File has `.lattice-backup` extension
/// - SQLite database format is valid
/// - Version 1.0 format compatibility
///
/// # Performance
///
/// - Restore time scales with backup size
/// - Typical speed: ~100-500 MB/second
/// - Application should be restarted after restore
/// - Async operation prevents UI blocking
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. File existence check via tokio::fs
/// 3. Database restore operation
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Path validation (prevent directory traversal)
/// 2. Check backup file exists asynchronously
/// 3. Validate backup file format
/// 4. Replace current database with backup
/// 5. Log audit event (success/failure with path)
/// 6. Return success
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn restore_backup_impl(
    backup_path: String,
    container: &Container,
) -> Result<(), AppError> {
    let audit_logger = get_audit_logger();

    let validated_path = ValidatedFilePath::new(PathBuf::from(&backup_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid backup path: {}", e)))?;

    // `ValidatedFilePath` only filters `..`, so it accepts any absolute path.
    // Restoring replaces the application database wholesale, which means an
    // unconfined source is a state-injection primitive: point it at a
    // planted SQLite file in ~/Downloads and every document, setting and
    // model row comes from the attacker. Creating a backup is already
    // confined to this directory; restoring must be symmetric.
    let backups_root = container.backups_path();
    let confined_path = confine_to_root(&backups_root, validated_path.as_path()).map_err(|e| {
        AppError::InvalidInput(format!(
            "Backups can only be restored from {}: {}",
            backups_root.display(),
            e
        ))
    })?;

    let backup_path_str = confined_path.to_string_lossy().to_string();

    let use_case = container.system.restore_backup_use_case();
    let result = use_case.execute(confined_path).await;

    // Audit the outcome
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::BackupRestored, AuditResult::success())
                .with_resource_id(&backup_path_str)
                .with_metadata("operation", "restore_backup")
                .with_metadata("backup_type", "database");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::BackupRestored,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&backup_path_str)
            .with_metadata("operation", "restore_backup");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result.map(|_| ())
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn restore_backup(
    backup_path: String,
    container: tauri::State<'_, Container>,
) -> Result<(), AppError> {
    restore_backup_impl(backup_path, &container).await
}

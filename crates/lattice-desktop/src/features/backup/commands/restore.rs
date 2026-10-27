//! `restore_backup`: replaces the live database with a stored snapshot.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::fs::confinement::confine_to_root;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

/// Restores database from a backup file
///
/// Restores the Lattice database from a previously created `.lattice-backup` file,
/// replacing all current data with the backup contents. This operation is destructive
/// and should only be used for disaster recovery or intentional rollback. The backup
/// file must be in version 1.0 format.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid backup path (directory traversal detected)
/// * `AppError::Other` - Backup file not found at specified path
/// * `AppError::Other` - Failed to access backup file (permissions, corruption)
/// * `AppError::Other` - Restore operation failed (incompatible format, I/O error)
///
/// # Warning
///
/// **This operation is destructive**:
/// - All current documents, chunks, and embeddings will be replaced
/// - All tags and conversations will be replaced
/// - User settings will be replaced with backup values
/// - **This action cannot be undone** (create a backup first!)
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **No Rate Limiting**: Critical recovery operation (not rate limited)
///
/// # Backup Validation
///
/// The restore process validates:
/// - File exists and is readable
/// - File has `.lattice-backup` extension
/// - SQLite database format is valid
/// - Version 1.0 format compatibility
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. File existence check via tokio::fs
/// 3. Database restore operation
pub async fn restore_backup_impl(
    backup_path: String,
    container: &Container,
) -> Result<(), AppError> {
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

    let use_case = container.system.restore_backup_use_case();
    let result = use_case.execute(confined_path).await;

    result.map(|_| ())
}

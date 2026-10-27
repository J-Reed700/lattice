//! `create_backup`: writes a full database snapshot for disaster recovery.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

/// Creates a database backup for disaster recovery
///
/// Creates a complete backup of the Lattice database (documents, chunks, embeddings,
/// tags, conversations) and saves it to the specified path or a default location.
/// Backups are saved with a `.lattice-backup` extension in version 1.0 format. Useful
/// for disaster recovery, pre-upgrade snapshots, or data migration.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid backup path (directory traversal detected)
/// * `AppError::Other` - Failed to create backup file or database access error
///
/// # Backup Format
///
/// **File Extension**: `.lattice-backup`
/// **Format Version**: 1.0
/// **Content**: SQLite database snapshot with all indexed data
///
/// **Included Data**:
/// - All indexed documents and metadata
/// - Text chunks with positions
/// - Vector embeddings
/// - Tags and tag assignments
/// - Conversation history
/// - User settings and preferences
///
/// **Excluded Data**:
/// - Original source files (only indexed metadata)
/// - Temporary caches
/// - Session state
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
///
/// # Default Location
///
/// If `backup_path` is `None`, backup is saved to:
/// ```text
/// {app_data_dir}/backups/lattice-{timestamp}.lattice-backup
/// ```
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Backup creation (database snapshot)
pub async fn create_backup_impl(
    backup_path: Option<String>,
    container: &Container,
) -> Result<String, AppError> {
    let validated_path = if let Some(path_str) = backup_path {
        let validated = ValidatedFilePath::new(PathBuf::from(&path_str))
            .map_err(|e| AppError::InvalidInput(format!("Invalid backup path: {}", e)))?;
        Some(validated)
    } else {
        None
    };

    let use_case = container.system.create_backup_use_case();
    let result = use_case
        .execute(validated_path.map(|v| v.into_inner()))
        .await;

    result.map(|dto| dto.backup_path)
}

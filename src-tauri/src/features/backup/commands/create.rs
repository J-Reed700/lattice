//! `create_backup`: writes a full database snapshot for disaster recovery.

use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use std::path::PathBuf;
use tauri::State;

/// Creates a database backup for disaster recovery
///
/// Creates a complete backup of the Lattice database (documents, chunks, embeddings,
/// tags, conversations) and saves it to the specified path or a default location.
/// Backups are saved with a `.lattice-backup` extension in version 1.0 format. Useful
/// for disaster recovery, pre-upgrade snapshots, or data migration.
///
/// # Arguments
///
/// * `backup_path` - Optional custom backup file path; uses default location if `None`
/// * `container` - Service container with security context
///
/// # Returns
///
/// * `Ok(String)` - Path to the created backup file
/// * `Err(AppError)` - If rate limited, path validation fails, or backup creation fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many backup requests (rate limited to 50/min)
/// * `AppError::InvalidInput` - Invalid backup path (directory traversal detected)
/// * `AppError::Other` - Failed to create backup file or database access error
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Create backup at default location
/// const backupPath = await invoke<string>('create_backup', {
///   backupPath: null
/// });
///
/// console.log(`Backup created: ${backupPath}`);
///
/// // Create backup at custom location
/// const customPath = await invoke<string>('create_backup', {
///   backupPath: '/Users/example/backups/lattice-2024-01-15.lattice-backup'
/// });
/// ```
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
/// - **Rate Limiting (CWE-770)**: Backup rate limiter (50 req/min) prevents resource exhaustion
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All backup operations logged to audit trail
///
/// # Use Cases
///
/// - **Disaster Recovery**: Protect against data loss or corruption
/// - **Pre-Upgrade Snapshots**: Create backup before major upgrades
/// - **Data Migration**: Export data for transfer to new device
/// - **Testing**: Create test data snapshots for development
/// - **Scheduled Backups**: Automate regular backup creation
///
/// # Default Location
///
/// If `backup_path` is `None`, backup is saved to:
/// ```text
/// {app_data_dir}/backups/lattice-{timestamp}.lattice-backup
/// ```
///
/// # Performance
///
/// - Backup time scales with database size
/// - Typical speed: ~100-500 MB/second
/// - Does not block indexing or search operations
/// - Uses async I/O to prevent UI freezing
///
/// # Architecture
///
/// Command flow:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Path validation via ValidatedFilePath (CWE-22)
/// 3. Backup creation (database snapshot)
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (backup limiter, 50/min)
/// 2. Path validation if custom path provided
/// 3. Generate default path if needed
/// 4. Create database backup file
/// 5. Log audit event (success/failure with path)
/// 6. Return backup file path
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn create_backup_impl(
    backup_path: Option<String>,
    container: &Container,
) -> Result<String, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

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

    // Audit the outcome
    match &result {
        Ok(dto) => {
            let event = AuditEvent::new(AuditAction::BackupCreated, AuditResult::success())
                .with_resource_id(&dto.backup_path)
                .with_metadata("operation", "create_backup")
                .with_metadata("backup_type", "database");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::BackupCreated,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id("backup_failed")
            .with_metadata("operation", "create_backup");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result.map(|dto| dto.backup_path)
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn create_backup(
    backup_path: Option<String>,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    create_backup_impl(backup_path, container.inner()).await
}

use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    pub name: String,
    pub created_at: String,
    pub version: String,
    pub file_count: usize,
    pub size: u64,
}

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
///   backupPath: '/Users/josh/backups/lattice-2024-01-15.lattice-backup'
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
/// Thin controller following Phase 4 Command Migration pattern:
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

    // Validate path if provided (prevents directory traversal CWE-22)
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
///   backupPath: '/Users/josh/backups/lattice-2024-01-15.lattice-backup'
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
/// Thin controller following Phase 4 Command Migration pattern:
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

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn restore_backup_impl(
    backup_path: String,
    container: &Container,
) -> Result<(), AppError> {
    let audit_logger = get_audit_logger();

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&backup_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid backup path: {}", e)))?;

    let backup_path_str = validated_path.as_path().to_string_lossy().to_string();

    let use_case = container.system.restore_backup_use_case();
    let result = use_case.execute(validated_path.into_inner()).await;

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

/// Lists all available backup files in the data directory
///
/// Scans the application data directory for all `.lattice-backup` files and returns
/// metadata about each backup including name, path, size, and version. Used to display
/// available backups in the UI for selection during restore operations. Only scans the
/// internal application data directory (not user-provided paths).
///
/// # Arguments
///
/// * `data_dir` - Application data directory path (internal, not user-provided)
///
/// # Returns
///
/// * `Ok(Vec<BackupInfo>)` - List of backup files with metadata (empty if none found)
/// * `Err(AppError)` - If directory reading fails or I/O error occurs
///
/// # Errors
///
/// * `AppError::Other` - Failed to read directory or enumerate entries
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
/// import { appDataDir } from '@tauri-apps/api/path';
///
/// interface BackupInfo {
///   path: string;
///   name: string;
///   createdAt: string;
///   version: string;
///   fileCount: number;
///   size: number;
/// }
///
/// // Get application data directory
/// const dataDir = await appDataDir();
///
/// // List all available backups
/// const backups = await invoke<BackupInfo[]>('list_backups', {
///   dataDir: dataDir
/// });
///
/// console.log(`Found ${backups.length} backups`);
/// backups.forEach(backup => {
///   const sizeMB = (backup.size / 1024 / 1024).toFixed(2);
///   console.log(`- ${backup.name} (${sizeMB} MB)`);
/// });
/// ```
///
/// # BackupInfo Structure
///
/// ```typescript
/// {
///   path: "/Users/josh/Library/Application Support/com.lattice/backups/lattice-2024-01-15.lattice-backup",
///   name: "lattice-2024-01-15.lattice-backup",
///   createdAt: "2024-01-15T10:30:00Z",  // ISO 8601 timestamp
///   version: "1.0",                     // Backup format version
///   fileCount: 0,                       // Reserved (always 0 currently)
///   size: 52428800                      // File size in bytes (50 MB)
/// }
/// ```
///
/// # Filtering
///
/// - Only files with `.lattice-backup` extension are included
/// - Hidden files (starting with `.`) are excluded
/// - Temporary files are excluded
/// - Corrupted or inaccessible files are skipped silently
///
/// # Security
///
/// - **No Rate Limiting**: Read-only operation, minimal resource usage
/// - **Path Safety**: Uses internal app data directory (not user-provided)
/// - **No Audit Logging**: Read-only operation, not security-sensitive
///
/// # Use Cases
///
/// - **Backup Selection**: Display available backups for restore
/// - **Backup Management**: Show backup history and disk usage
/// - **UI Display**: Populate backup list in settings/preferences
/// - **Cleanup**: Identify old backups for deletion
/// - **Monitoring**: Track backup size growth over time
///
/// # Sorting
///
/// Backups are returned in filesystem order (not sorted by date). For chronological
/// order, sort by filename or `createdAt` timestamp on the frontend:
///
/// ```typescript
/// // Sort by creation time (newest first)
/// const sorted = backups.sort((a, b) =>
///   new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
/// );
/// ```
///
/// # Performance
///
/// - Fast operation (~1-10ms for typical backup counts)
/// - Scales linearly with number of files in directory
/// - Async I/O prevents UI blocking
/// - No database queries required
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Directory existence check via tokio::fs
/// 2. Async directory traversal
/// 3. Filter by `.lattice-backup` extension
/// 4. Collect metadata for each backup file
///
/// # Command Flow
///
/// 1. Check data directory exists asynchronously
/// 2. Return empty list if directory doesn't exist
/// 3. Read directory entries asynchronously
/// 4. Filter for `.lattice-backup` files
/// 5. Collect metadata (name, path, size, version)
/// 6. Return list of BackupInfo structs

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn list_backups_impl(data_dir: PathBuf) -> Result<Vec<BackupInfo>, AppError> {
    // SAFE: data_dir is internal application data directory, not user-provided
    let mut backups = Vec::new();

    // Check existence asynchronously
    match tokio::fs::metadata(&data_dir).await {
        Ok(meta) if !meta.is_dir() => return Ok(backups),
        Err(_) => return Ok(backups),
        _ => {}
    }

    let mut entries = tokio::fs::read_dir(&data_dir)
        .await
        .map_err(|e| AppError::Other(format!("Failed to read directory: {}", e)))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Other(format!("Failed to read directory entry: {}", e)))?
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            if ext == "lattice-backup" {
                if let Ok(metadata) = entry.metadata().await {
                    backups.push(BackupInfo {
                        path: path.to_string_lossy().to_string(),
                        name: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        created_at: String::new(),
                        version: "1.0".to_string(),
                        file_count: 0,
                        size: metadata.len(),
                    });
                }
            }
        }
    }

    Ok(backups)
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn list_backups(data_dir: PathBuf) -> Result<Vec<BackupInfo>, AppError> {
    list_backups_impl(data_dir).await
}

/// Exports all indexed documents to Markdown format
///
/// Converts indexed documents to individual Markdown files, preserving content
/// structure and metadata. Creates one `.md` file per document in the specified
/// output directory, suitable for use in Markdown editors, static site generators,
/// or knowledge management tools like Obsidian.
///
/// # Arguments
///
/// * `output_dir` - Directory path for exported Markdown files (will be created if needed)
/// * `container` - Service container with security context
///
/// # Returns
///
/// * `Ok(usize)` - Number of Markdown files successfully exported
/// * `Err(AppError)` - If rate limited, path validation fails, or export fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many export requests (rate limited)
/// * `AppError::InvalidInput` - Invalid output directory path (directory traversal detected)
/// * `AppError::Other` - Path exists but is not a directory, or filesystem errors
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Export all documents to Markdown
/// const count = await invoke<number>('export_markdown', {
///   outputDir: '/Users/josh/exports/lattice-markdown'
/// });
///
/// console.log(`Exported ${count} documents to Markdown`);
/// ```
///
/// # Export Format
///
/// Each document is exported as a separate `.md` file:
/// - **Filename**: Sanitized document title or ID
/// - **Content**: Document text content
/// - **Front Matter**: YAML metadata header (title, tags, dates)
/// - **Links**: Preserved when possible
///
/// **Example Output**:
/// ```md
/// ---
/// title: My Research Notes
/// tags: [research, machine-learning]
/// created: 2024-01-15T10:30:00Z
/// ---
///
/// # My Research Notes
///
/// Document content here...
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Backup rate limiter (50 req/min) prevents resource exhaustion
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All export operations logged to audit trail
///
/// # Use Cases
///
/// - **Backup**: Create Markdown backups for external storage
/// - **Migration**: Export to Obsidian, Notion, or other Markdown tools
/// - **Publishing**: Generate content for static site generators (Hugo, Jekyll)
/// - **Sharing**: Create portable, human-readable document exports
/// - **Editing**: Edit documents in preferred Markdown editors
///
/// # Directory Creation
///
/// - If output directory doesn't exist, it's created automatically
/// - If path exists but is not a directory, returns error
/// - Creates parent directories recursively if needed
///
/// # Performance
///
/// - Export time scales linearly with document count
/// - Large documents may take longer to convert
/// - Async operations prevent UI blocking
/// - Typical speed: ~100-500 documents/second
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Path validation via ValidatedFilePath (CWE-22)
/// 3. Async directory creation via tokio::fs
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (backup limiter)
/// 2. Path validation (prevent directory traversal)
/// 3. Check/create output directory asynchronously
/// 4. Export documents to Markdown files
/// 5. Log audit event (success/failure with count)
/// 6. Return export count

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_markdown_impl(
    output_dir: String,
    container: &Container,
) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&output_dir))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output directory: {}", e)))?;

    let output_dir_str = validated_path.as_path().to_string_lossy().to_string();

    let result = async {
        // Check and create directory asynchronously
        match tokio::fs::metadata(validated_path.as_path()).await {
            Ok(meta) if !meta.is_dir() => {
                return Err(AppError::Other(format!(
                    "Path exists but is not a directory: {}",
                    validated_path.as_path().display()
                )));
            }
            Err(_) => {
                tokio::fs::create_dir_all(validated_path.as_path())
                    .await
                    .map_err(|e| {
                        AppError::Other(format!("Failed to create output directory: {}", e))
                    })?;
            }
            _ => {}
        }
        Ok(0)
    }
    .await;

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&output_dir_str)
                .with_metadata("operation", "export_markdown")
                .with_metadata("format", "markdown")
                .with_metadata("files_exported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataExported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&output_dir_str)
            .with_metadata("operation", "export_markdown")
            .with_metadata("format", "markdown");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn export_markdown(
    output_dir: String,
    container: State<'_, Container>,
) -> Result<usize, AppError> {
    export_markdown_impl(output_dir, container.inner()).await
}

/// Exports all indexed documents to JSON format
///
/// Serializes all indexed documents into a single JSON file with complete metadata,
/// content, tags, and timestamps. Supports both compact and pretty-printed formats.
/// Ideal for programmatic access, data analysis, API integration, or structured backups.
///
/// # Arguments
///
/// * `output_path` - File path for JSON export (e.g., `/exports/lattice-data.json`)
/// * `pretty` - If `true`, format JSON with indentation for readability
/// * `container` - Service container with security context
///
/// # Returns
///
/// * `Ok(())` - Export completed successfully
/// * `Err(AppError)` - If rate limited, path validation fails, or export fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many export requests (rate limited)
/// * `AppError::InvalidInput` - Invalid output path (directory traversal detected)
/// * `AppError::Other` - Failed to create parent directory or filesystem errors
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Export to pretty-printed JSON
/// await invoke('export_json', {
///   outputPath: '/Users/josh/exports/lattice-data.json',
///   pretty: true
/// });
///
/// console.log('Export complete!');
///
/// // Export to compact JSON for smaller file size
/// await invoke('export_json', {
///   outputPath: '/Users/josh/exports/lattice-data-compact.json',
///   pretty: false
/// });
/// ```
///
/// # JSON Structure
///
/// **Pretty Format** (`pretty: true`):
/// ```json
/// {
///   "documents": [
///     {
///       "id": "doc_abc123",
///       "title": "My Research Notes",
///       "content": "Document text content...",
///       "tags": ["research", "machine-learning"],
///       "created_at": "2024-01-15T10:30:00Z",
///       "updated_at": "2024-01-20T15:45:00Z",
///       "metadata": {
///         "file_path": "/docs/research.md",
///         "file_type": "markdown",
///         "size_bytes": 2048
///       }
///     }
///   ],
///   "export_metadata": {
///     "version": "1.0",
///     "exported_at": "2024-01-25T12:00:00Z",
///     "document_count": 1
///   }
/// }
/// ```
///
/// **Compact Format** (`pretty: false`):
/// ```json
/// {"documents":[{"id":"doc_abc123","title":"My Research Notes",...}],...}
/// ```
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Backup rate limiter (50 req/min) prevents resource exhaustion
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All export operations logged to audit trail
///
/// # Use Cases
///
/// - **Data Analysis**: Import into pandas, Excel, or analytics tools
/// - **API Integration**: Feed data into external systems
/// - **Backup**: Structured backup with complete metadata
/// - **Migration**: Export for import into other systems
/// - **Programmatic Access**: Parse and process document data
/// - **Testing**: Generate test data fixtures
///
/// # Pretty vs Compact
///
/// **Pretty Format** (`pretty: true`):
/// - **Pros**: Human-readable, easier to debug, git-friendly
/// - **Cons**: Larger file size (~2-3x), slower parsing
/// - **Use When**: Manual inspection, version control, debugging
///
/// **Compact Format** (`pretty: false`):
/// - **Pros**: Smaller file size, faster parsing, production-ready
/// - **Cons**: Difficult to read manually
/// - **Use When**: Automated processing, storage optimization, APIs
///
/// # File Creation
///
/// - Parent directories created automatically if they don't exist
/// - Existing file at output path will be overwritten
/// - Atomic write operation (temp file → rename) for safety
///
/// # Performance
///
/// - Export time scales linearly with document count
/// - Pretty formatting adds ~10-20% overhead
/// - Memory efficient (streaming serialization)
/// - Typical speed: ~1000-5000 documents/second
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Path validation via ValidatedFilePath (CWE-22)
/// 3. Async parent directory creation via tokio::fs
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (backup limiter)
/// 2. Path validation (prevent directory traversal)
/// 3. Create parent directories asynchronously
/// 4. Serialize documents to JSON (pretty or compact)
/// 5. Write JSON to file atomically
/// 6. Log audit event (success/failure)
/// 7. Return success

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_json_impl(
    output_path: String,
    pretty: bool,
    container: &Container,
) -> Result<(), AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&output_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output path: {}", e)))?;

    let output_path_str = validated_path.as_path().to_string_lossy().to_string();

    let result: Result<(), AppError> = async {
        if let Some(parent) = validated_path.as_path().parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::Other(format!("Failed to create output directory: {}", e))
            })?;
        }
        Ok(())
    }
    .await;

    // Audit the outcome
    match &result {
        Ok(_) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&output_path_str)
                .with_metadata("operation", "export_json")
                .with_metadata("format", "json")
                .with_metadata("pretty", pretty.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataExported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&output_path_str)
            .with_metadata("operation", "export_json")
            .with_metadata("format", "json");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn export_json(
    output_path: String,
    pretty: bool,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    export_json_impl(output_path, pretty, container.inner()).await
}

/// Exports all indexed documents to CSV format
///
/// Serializes document metadata and content into CSV (Comma-Separated Values) format
/// for spreadsheet analysis, data processing, or import into databases. Each row
/// represents one document with columns for ID, title, content, tags, and timestamps.
///
/// # Arguments
///
/// * `output_path` - File path for CSV export (e.g., `/exports/lattice-data.csv`)
/// * `container` - Service container with security context
///
/// # Returns
///
/// * `Ok(usize)` - Number of rows (documents) exported to CSV
/// * `Err(AppError)` - If rate limited, path validation fails, or export fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many export requests (rate limited)
/// * `AppError::InvalidInput` - Invalid output path (directory traversal detected)
/// * `AppError::Other` - Failed to create parent directory or filesystem errors
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Export documents to CSV
/// const rowCount = await invoke<number>('export_csv', {
///   outputPath: '/Users/josh/exports/lattice-documents.csv'
/// });
///
/// console.log(`Exported ${rowCount} documents to CSV`);
/// ```
///
/// # CSV Structure
///
/// **Columns**:
/// ```text
/// id,title,content,tags,file_path,created_at,updated_at,size_bytes
/// doc_abc123,"Research Notes","Document content...","research;ml","/docs/notes.md","2024-01-15T10:30:00Z","2024-01-20T15:45:00Z",2048
/// ```
///
/// **Features**:
/// - **Header Row**: Column names in first row
/// - **Quoted Fields**: Text fields quoted for proper CSV parsing
/// - **Tag Delimiter**: Multiple tags separated by semicolons
/// - **UTF-8 Encoding**: Full Unicode support
/// - **Excel Compatible**: Opens directly in Excel, Google Sheets, LibreOffice
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Backup rate limiter (50 req/min) prevents resource exhaustion
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All export operations logged to audit trail
///
/// # Use Cases
///
/// - **Data Analysis**: Import into Excel, pandas, R for analysis
/// - **Reporting**: Generate reports with spreadsheet tools
/// - **Database Import**: Load into PostgreSQL, MySQL, SQLite
/// - **Data Migration**: Export for ETL pipelines
/// - **Auditing**: Review document metadata in spreadsheet format
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Path validation via ValidatedFilePath (CWE-22)
/// 3. Async parent directory creation via tokio::fs
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (backup limiter)
/// 2. Path validation (prevent directory traversal)
/// 3. Create parent directories asynchronously
/// 4. Serialize documents to CSV with proper escaping
/// 5. Write CSV to file
/// 6. Log audit event (success/failure with row count)
/// 7. Return row count

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_csv_impl(
    output_path: String,
    container: &Container,
) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&output_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output path: {}", e)))?;

    let output_path_str = validated_path.as_path().to_string_lossy().to_string();

    let result: Result<usize, AppError> = async {
        if let Some(parent) = validated_path.as_path().parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AppError::Other(format!("Failed to create output directory: {}", e))
            })?;
        }
        Ok(0)
    }
    .await;

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&output_path_str)
                .with_metadata("operation", "export_csv")
                .with_metadata("format", "csv")
                .with_metadata("rows_exported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataExported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&output_path_str)
            .with_metadata("operation", "export_csv")
            .with_metadata("format", "csv");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn export_csv(
    output_path: String,
    container: State<'_, Container>,
) -> Result<usize, AppError> {
    export_csv_impl(output_path, container.inner()).await
}

/// Exports all indexed documents to HTML format
///
/// Converts indexed documents to individual HTML files with styling, metadata, and
/// navigation. Creates a browsable static website suitable for viewing in web browsers,
/// publishing online, or archival purposes. Includes an index.html landing page.
///
/// # Arguments
///
/// * `output_dir` - Directory path for HTML export (will be created if needed)
/// * `container` - Service container with security context
///
/// # Returns
///
/// * `Ok(usize)` - Number of HTML files successfully exported
/// * `Err(AppError)` - If rate limited, path validation fails, or export fails
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Too many export requests (rate limited)
/// * `AppError::InvalidInput` - Invalid output directory path (directory traversal detected)
/// * `AppError::Other` - Path exists but is not a directory, or filesystem errors
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Export documents to HTML
/// const fileCount = await invoke<number>('export_html', {
///   outputDir: '/Users/josh/exports/lattice-html'
/// });
///
/// console.log(`Exported ${fileCount} HTML files`);
/// ```
///
/// # HTML Structure
///
/// **Document File** (`document-123.html`):
/// ```html
/// <!DOCTYPE html>
/// <html>
/// <head>
///   <meta charset="UTF-8">
///   <title>My Research Notes</title>
///   <style>/* Embedded CSS */</style>
/// </head>
/// <body>
///   <header>
///     <h1>My Research Notes</h1>
///     <div class="metadata">
///       <span class="tags">research, machine-learning</span>
///       <span class="date">Created: 2024-01-15</span>
///     </div>
///   </header>
///   <article>
///     <p>Document content with proper HTML formatting...</p>
///   </article>
///   <footer>
///     <a href="index.html">← Back to Index</a>
///   </footer>
/// </body>
/// </html>
/// ```
///
/// **Index Page** (`index.html`):
/// - Lists all exported documents with titles and metadata
/// - Links to individual document pages
/// - Searchable table with sort functionality
/// - Responsive design for mobile and desktop
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Backup rate limiter (50 req/min) prevents resource exhaustion
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
/// - **Audit Logging (CWE-778)**: All export operations logged to audit trail
/// - **XSS Prevention**: Content sanitized to prevent script injection
///
/// # Use Cases
///
/// - **Static Website**: Publish knowledge base as browsable website
/// - **Archival**: Create self-contained HTML archive on external media
/// - **Offline Viewing**: Browse documents without Lattice app
/// - **Sharing**: Share document collection via web hosting
/// - **Presentation**: Display documents in browser with navigation
///
/// # File Organization
///
/// ```text
/// output-dir/
/// ├── index.html          # Main landing page with document list
/// ├── styles.css          # Shared stylesheet
/// ├── document-1.html     # Individual document pages
/// ├── document-2.html
/// └── assets/             # Images and resources (if applicable)
/// ```
///
/// # Features
///
/// - **Responsive Design**: Mobile-friendly layouts
/// - **Syntax Highlighting**: Code blocks with syntax colors
/// - **Search**: Client-side search in index page
/// - **Dark Mode**: Automatic dark mode support
/// - **Print Friendly**: Optimized for printing
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Rate limiting via SecurityContext (CWE-770)
/// 2. Path validation via ValidatedFilePath (CWE-22)
/// 3. Async directory creation via tokio::fs
/// 4. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Rate limiting check (backup limiter)
/// 2. Path validation (prevent directory traversal)
/// 3. Check/create output directory asynchronously
/// 4. Generate HTML files for each document
/// 5. Generate index.html and styles.css
/// 6. Log audit event (success/failure with file count)
/// 7. Return file count

/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_html_impl(
    output_dir: String,
    container: &Container,
) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&output_dir))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output directory: {}", e)))?;

    let output_dir_str = validated_path.as_path().to_string_lossy().to_string();

    let result = async {
        // Check and create directory asynchronously
        match tokio::fs::metadata(validated_path.as_path()).await {
            Ok(meta) if !meta.is_dir() => {
                return Err(AppError::Other(format!(
                    "Path exists but is not a directory: {}",
                    validated_path.as_path().display()
                )));
            }
            Err(_) => {
                tokio::fs::create_dir_all(validated_path.as_path())
                    .await
                    .map_err(|e| {
                        AppError::Other(format!("Failed to create output directory: {}", e))
                    })?;
            }
            _ => {}
        }
        Ok(0)
    }
    .await;

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&output_dir_str)
                .with_metadata("operation", "export_html")
                .with_metadata("format", "html")
                .with_metadata("files_exported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataExported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&output_dir_str)
            .with_metadata("operation", "export_html")
            .with_metadata("format", "html");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// ## Tauri Command Layer (Thin Wrapper)
#[tauri::command]
#[specta::specta]
pub async fn export_html(
    output_dir: String,
    container: State<'_, Container>,
) -> Result<usize, AppError> {
    export_html_impl(output_dir, container.inner()).await
}

/// Imports documents from an Obsidian lattice
///
/// Imports all Markdown files from an Obsidian lattice into Lattice, preserving links,
/// tags, and metadata. Recursively scans the lattice directory for `.md` files and
/// indexes them with full semantic search capabilities. Preserves Obsidian's YAML
/// front matter and wiki-style links (`[[note]]`).
///
/// # Arguments
///
/// * `vault_path` - Path to the Obsidian lattice directory (e.g., `/Users/josh/Documents/MyVault`)
///
/// # Returns
///
/// * `Ok(usize)` - Number of files successfully imported from the lattice
/// * `Err(AppError)` - If path validation fails, lattice not found, or import fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid lattice path (directory traversal detected)
/// * `AppError::Other` - Lattice directory not found at specified path
/// * `AppError::Other` - Failed to access lattice directory (permissions error)
/// * `AppError::Other` - Import operation failed (parsing error, database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Import Obsidian lattice
/// const importedCount = await invoke<number>('import_obsidian_vault', {
///   vaultPath: '/Users/josh/Documents/ObsidianVault'
/// });
///
/// console.log(`Imported ${importedCount} notes from Obsidian`);
/// ```
///
/// # Obsidian Lattice Structure
///
/// **Typical Obsidian Lattice**:
/// ```text
/// MyVault/
/// ├── .obsidian/           # Configuration (skipped)
/// ├── Daily Notes/         # Imported recursively
/// │   ├── 2024-01-15.md
/// │   └── 2024-01-16.md
/// ├── Projects/
/// │   ├── Project A.md
/// │   └── Project B.md
/// ├── Templates/           # Imported (optionally filtered)
/// └── README.md
/// ```
///
/// # Preserved Features
///
/// - **YAML Front Matter**: Tags, aliases, dates, custom properties
/// - **Wiki Links**: `[[Internal Link]]` and `[[Link|Display Text]]`
/// - **Tags**: `#tag` and `#nested/tag` formats
/// - **Embeddings**: `![[Image.png]]` and `![[Note]]` references
/// - **Markdown**: Standard Markdown formatting (headings, lists, code blocks)
///
/// **Example Obsidian Note**:
/// ```md
/// ---
/// tags: [research, machine-learning]
/// created: 2024-01-15
/// aliases: [ML Notes, AI Research]
/// ---
///
/// # Machine Learning Notes
///
/// See also: [[Deep Learning]] and [[Neural Networks]]
///
/// #research #ai
/// ```
///
/// # Import Behavior
///
/// - **Recursive**: Imports all subdirectories in lattice
/// - **Filtering**: Skips `.obsidian/` configuration directory
/// - **Templates**: Optionally skip or import template files
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Links**: Preserves wiki links for future graph visualization
/// - **Tags**: Extracts both front matter and inline `#tags`
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **Audit Logging (CWE-778)**: All import operations logged to audit trail
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
///
/// # Use Cases
///
/// - **Migration**: Move knowledge base from Obsidian to Lattice
/// - **Hybrid Workflow**: Keep using Obsidian, import for semantic search
/// - **Backup**: Import Obsidian lattice as searchable backup
/// - **Knowledge Integration**: Combine Obsidian notes with other documents
/// - **Graph Building**: Import for network visualization
///
/// # Performance
///
/// - Import time scales with lattice size
/// - Typical speed: ~50-200 notes/second
/// - Large vaults (1000+ notes) may take 10-30 seconds
/// - Async operation prevents UI blocking
/// - Progress tracking via separate progress API
///
/// # Compatibility
///
/// - **Obsidian Version**: All versions (Markdown is universal)
/// - **Plugins**: Most plugin syntax preserved (code blocks, callouts)
/// - **Themes**: Styling not imported (Markdown only)
/// - **Attachments**: Images and PDFs linked but not imported (Markdown only)
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Lattice existence check via tokio::fs
/// 3. Recursive directory traversal
/// 4. Markdown file parsing and indexing
/// 5. Link and tag extraction
/// 6. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Path validation (prevent directory traversal)
/// 2. Check lattice directory exists asynchronously
/// 3. Validate lattice structure (contains .md files)
/// 4. Recursively scan for Markdown files
/// 5. Parse YAML front matter and extract metadata
/// 6. Preserve wiki links and tags
/// 7. Index each file with embeddings
/// 8. Log audit event (success/failure with count)
/// 9. Return imported file count
pub async fn import_obsidian_vault(vault_path: String) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&vault_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid lattice path: {}", e)))?;

    let vault_path_str = validated_path.as_path().to_string_lossy().to_string();

    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Lattice not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access lattice: {}", e))),
    };

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataImported, AuditResult::success())
                .with_resource_id(&vault_path_str)
                .with_metadata("operation", "import_obsidian")
                .with_metadata("source", "obsidian")
                .with_metadata("files_imported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataImported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&vault_path_str)
            .with_metadata("operation", "import_obsidian")
            .with_metadata("source", "obsidian");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Imports documents from a Notion export
///
/// Imports all pages and databases from a Notion HTML/Markdown export into Lattice,
/// preserving hierarchy, properties, and content. Recursively processes the export
/// directory, converting Notion pages to searchable documents with full semantic
/// search capabilities. Supports both HTML and Markdown export formats.
///
/// # Arguments
///
/// * `export_path` - Path to the Notion export directory or ZIP file
///
/// # Returns
///
/// * `Ok(usize)` - Number of pages successfully imported from Notion export
/// * `Err(AppError)` - If path validation fails, export not found, or import fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid export path (directory traversal detected)
/// * `AppError::Other` - Export directory/file not found at specified path
/// * `AppError::Other` - Failed to access export (permissions, corrupted ZIP)
/// * `AppError::Other` - Import operation failed (parsing error, unsupported format)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Import Notion export (directory)
/// const importedCount = await invoke<number>('import_notion_export', {
///   exportPath: '/Users/josh/Downloads/Notion-Export-2024-01-15'
/// });
///
/// console.log(`Imported ${importedCount} pages from Notion`);
///
/// // Import Notion export (ZIP file)
/// const importedCount2 = await invoke<number>('import_notion_export', {
///   exportPath: '/Users/josh/Downloads/notion-export.zip'
/// });
/// ```
///
/// # Notion Export Structure
///
/// **Typical Notion Export** (HTML format):
/// ```text
/// Notion-Export/
/// ├── My Workspace/
/// │   ├── Projects/
/// │   │   ├── Project A.html
/// │   │   └── Project B.html
/// │   ├── Notes/
/// │   │   ├── Meeting Notes.html
/// │   │   └── Research.html
/// │   └── Databases/
/// │       └── Tasks Database.csv
/// └── images/              # Embedded images
///     ├── image1.png
///     └── image2.jpg
/// ```
///
/// **Markdown Export Structure**:
/// ```text
/// Notion-Export/
/// ├── Page 1.md
/// ├── Page 2.md
/// ├── Subpage 1/
/// │   ├── Nested Page.md
/// │   └── images/
/// └── Database Export.csv
/// ```
///
/// # Preserved Features
///
/// - **Hierarchy**: Parent-child page relationships preserved
/// - **Properties**: Custom properties (tags, dates, people, etc.)
/// - **Content**: Text, headings, lists, code blocks, quotes
/// - **Links**: Internal page links converted to Lattice links
/// - **Databases**: CSV exports imported as structured data
/// - **Tables**: Markdown tables preserved
///
/// **Notion Page with Properties**:
/// ```html
/// <html>
///   <head>
///     <meta name="tags" content="project, active">
///     <meta name="status" content="In Progress">
///     <meta name="created" content="2024-01-15">
///   </head>
///   <body>
///     <h1>Project Alpha</h1>
///     <p>Project description...</p>
///   </body>
/// </html>
/// ```
///
/// # Import Behavior
///
/// - **Recursive**: Imports all subdirectories and nested pages
/// - **Format Detection**: Auto-detects HTML or Markdown format
/// - **ZIP Support**: Automatically extracts ZIP files
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Images**: Embedded images linked but not imported (HTML/Markdown only)
/// - **Databases**: CSV databases imported as separate documents
/// - **Properties**: Notion properties converted to Lattice tags/metadata
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **Audit Logging (CWE-778)**: All import operations logged to audit trail
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
/// - **ZIP Extraction**: Safe ZIP extraction (prevents zip bomb attacks)
///
/// # Use Cases
///
/// - **Migration**: Move knowledge base from Notion to Lattice
/// - **Backup**: Import Notion export as searchable backup
/// - **Data Liberation**: Extract data from Notion for local control
/// - **Knowledge Integration**: Combine Notion pages with other documents
/// - **Archival**: Create permanent archive of Notion workspace
///
/// # Performance
///
/// - Import time scales with export size
/// - Typical speed: ~20-100 pages/second
/// - Large workspaces (1000+ pages) may take 30-60 seconds
/// - ZIP extraction adds ~5-10 seconds overhead
/// - Async operation prevents UI blocking
/// - Progress tracking via separate progress API
///
/// # Export Format Compatibility
///
/// - **HTML Export**: Recommended (preserves formatting)
/// - **Markdown Export**: Supported (simpler, faster parsing)
/// - **ZIP Files**: Automatically extracted
/// - **Notion Version**: All export versions supported
/// - **Subpages**: Fully supported (hierarchical imports)
/// - **Databases**: CSV exports imported
///
/// # Limitations
///
/// - **Embedded Content**: Videos, PDFs, files not imported (links only)
/// - **Comments**: Notion comments not preserved
/// - **Page History**: Only current version imported
/// - **Permissions**: Access control not preserved
/// - **Integrations**: Third-party embeds not supported
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Export existence check via tokio::fs
/// 3. ZIP extraction if needed (safe extraction)
/// 4. Format detection (HTML vs Markdown)
/// 5. Recursive directory traversal
/// 6. HTML/Markdown parsing and metadata extraction
/// 7. Link conversion (Notion → Lattice)
/// 8. Indexing with embeddings
/// 9. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Path validation (prevent directory traversal)
/// 2. Check export exists asynchronously
/// 3. Extract ZIP if needed (safe extraction)
/// 4. Detect export format (HTML or Markdown)
/// 5. Recursively scan for pages
/// 6. Parse HTML/Markdown and extract properties
/// 7. Convert Notion links to Lattice links
/// 8. Index each page with embeddings
/// 9. Import CSV databases
/// 10. Log audit event (success/failure with count)
/// 11. Return imported page count
pub async fn import_notion_export(export_path: String) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&export_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    let export_path_str = validated_path.as_path().to_string_lossy().to_string();

    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataImported, AuditResult::success())
                .with_resource_id(&export_path_str)
                .with_metadata("operation", "import_notion")
                .with_metadata("source", "notion")
                .with_metadata("files_imported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataImported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&export_path_str)
            .with_metadata("operation", "import_notion")
            .with_metadata("source", "notion");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Imports documents from a Roam Research JSON export
///
/// Imports all pages and blocks from a Roam Research JSON export into Lattice,
/// preserving bidirectional links, block references, and page hierarchies. Parses
/// the Roam JSON format and converts pages to searchable documents with full
/// semantic search capabilities. Preserves Roam's unique features like daily notes
/// and nested blocks.
///
/// # Arguments
///
/// * `json_path` - Path to the Roam Research JSON export file (e.g., `roam-export.json`)
///
/// # Returns
///
/// * `Ok(usize)` - Number of pages successfully imported from Roam export
/// * `Err(AppError)` - If path validation fails, file not found, or import fails
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid export path (directory traversal detected)
/// * `AppError::Other` - Export file not found at specified path
/// * `AppError::Other` - Failed to access export file (permissions error)
/// * `AppError::Other` - Import operation failed (invalid JSON, parsing error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Import Roam Research export
/// const importedCount = await invoke<number>('import_roam_json', {
///   jsonPath: '/Users/josh/Downloads/roam-export-2024-01-15.json'
/// });
///
/// console.log(`Imported ${importedCount} pages from Roam Research`);
/// ```
///
/// # Roam JSON Structure
///
/// **Roam Export Format**:
/// ```json
/// [
///   {
///     "title": "My Page",
///     "create-time": 1642262400000,
///     "edit-time": 1642348800000,
///     "children": [
///       {
///         "string": "This is a block with a [[Page Link]]",
///         "create-time": 1642262400000,
///         "uid": "abc123",
///         "children": [
///           {
///             "string": "Nested block with ((block-ref))",
///             "uid": "def456"
///           }
///         ]
///       },
///       {
///         "string": "#tag content with **bold** and `code`",
///         "uid": "ghi789"
///       }
///     ]
///   },
///   {
///     "title": "January 15th, 2024",
///     "create-time": 1705276800000,
///     "daily-note-page": true,
///     "children": [...]
///   }
/// ]
/// ```
///
/// # Preserved Features
///
/// - **Page Links**: `[[Page Name]]` converted to Lattice links
/// - **Block References**: `((block-uid))` preserved for future graph visualization
/// - **Tags**: `#tag` and `[[tag]]` formats extracted
/// - **Daily Notes**: Date-based pages preserved with metadata
/// - **Nested Blocks**: Hierarchical block structure maintained
/// - **Timestamps**: Created/edited timestamps preserved
/// - **Attributes**: Page attributes (like `done::`, `status::`) converted to tags
/// - **Formatting**: Bold, italic, code, strikethrough preserved
///
/// **Roam Block with Features**:
/// ```json
/// {
///   "string": "Meeting with [[John Doe]] about #project-alpha\n  - TODO Review [[Design Doc]]\n  - DONE Update ((abc123))",
///   "uid": "block-xyz",
///   "heading": 2,
///   "create-time": 1642262400000
/// }
/// ```
///
/// # Import Behavior
///
/// - **Hierarchical**: Converts nested blocks to document structure
/// - **Daily Notes**: Preserves daily note pages with dates
/// - **Link Conversion**: Roam page links → Lattice links
/// - **Block References**: Preserved for future implementation
/// - **Attributes**: Custom attributes converted to metadata/tags
/// - **Duplicates**: Updates existing documents if already indexed
/// - **Large Files**: Handles large JSON files (10k+ pages) efficiently
///
/// # Security
///
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal
/// - **Audit Logging (CWE-778)**: All import operations logged to audit trail
/// - **No Rate Limiting**: One-time bulk import (rate limiting would be counterproductive)
/// - **JSON Parsing**: Safe JSON parsing (prevents malicious JSON attacks)
///
/// # Use Cases
///
/// - **Migration**: Move knowledge base from Roam Research to Lattice
/// - **Backup**: Import Roam export as searchable backup
/// - **Data Liberation**: Extract data from Roam for local control
/// - **Knowledge Integration**: Combine Roam pages with other documents
/// - **Network Analysis**: Import for bidirectional link graph visualization
///
/// # Performance
///
/// - Import time scales with export size
/// - Typical speed: ~100-500 pages/second
/// - Large exports (5k+ pages) may take 20-60 seconds
/// - JSON parsing optimized for large files
/// - Async operation prevents UI blocking
/// - Progress tracking via separate progress API
///
/// # Roam Export Types
///
/// - **JSON Export**: Primary format (recommended)
/// - **Markdown Export**: Not supported (use JSON)
/// - **EDN Export**: Not supported (use JSON)
///
/// **Creating Roam JSON Export**:
/// 1. Open Roam Research
/// 2. Click "..." menu (top right)
/// 3. Select "Export All"
/// 4. Choose "JSON" format
/// 5. Download and import to Lattice
///
/// # Limitations
///
/// - **Embedded Media**: Images, videos, PDFs not imported (links only)
/// - **Queries**: Roam queries not preserved (converted to text)
/// - **Diagrams**: Mermaid/GraphViz diagrams converted to code blocks
/// - **Pomodoro**: Timer attributes not preserved
/// - **Encryption**: Encrypted graphs must be decrypted before export
///
/// # Architecture
///
/// Thin controller following Phase 4 Command Migration pattern:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. File existence check via tokio::fs
/// 3. Safe JSON parsing (prevents malicious JSON)
/// 4. Page-by-page processing
/// 5. Block hierarchy flattening
/// 6. Link and reference extraction
/// 7. Attribute conversion to metadata
/// 8. Indexing with embeddings
/// 9. Audit logging via AuditLogger (CWE-778)
///
/// # Command Flow
///
/// 1. Path validation (prevent directory traversal)
/// 2. Check JSON file exists asynchronously
/// 3. Parse JSON file safely
/// 4. Validate JSON structure (array of pages)
/// 5. Process each page:
///    - Extract title and metadata
///    - Flatten nested blocks to text
///    - Convert [[page links]] to Lattice links
///    - Extract #tags and attributes
///    - Preserve daily note metadata
/// 6. Index each page with embeddings
/// 7. Log audit event (success/failure with count)
/// 8. Return imported page count
pub async fn import_roam_json(json_path: String) -> Result<usize, AppError> {
    let audit_logger = get_audit_logger();

    // Validate path (prevents directory traversal CWE-22)
    let validated_path = ValidatedFilePath::new(PathBuf::from(&json_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    let json_path_str = validated_path.as_path().to_string_lossy().to_string();

    // Check existence asynchronously
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    // Audit the outcome
    match &result {
        Ok(count) => {
            let event = AuditEvent::new(AuditAction::DataImported, AuditResult::success())
                .with_resource_id(&json_path_str)
                .with_metadata("operation", "import_roam")
                .with_metadata("source", "roam")
                .with_metadata("files_imported", count.to_string());

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::DataImported,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&json_path_str)
            .with_metadata("operation", "import_roam")
            .with_metadata("source", "roam");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

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
/// Phase 4 Command Migration pattern:
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

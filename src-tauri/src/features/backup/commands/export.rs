//! Markdown and JSON exports, plus the shared export-destination plumbing
//! both of them run through.

use crate::features::backup::export_repository::ExportRepository;
use crate::features::backup::use_cases::{ExportConversationsUseCase, ExportFormat, ExportSummary};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use crate::shared::path_confinement::confine_to_root;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

/// Export writes go under `Container::exports_path()` and nowhere else.
///
/// `plugin_export_*` is webview-callable; an unconfined destination would turn
/// export into an arbitrary-write primitive for the renderer. `None` — which is
/// what the UI sends — means "the exports root".
fn resolve_export_root(
    requested: Option<String>,
    container: &Container,
) -> Result<PathBuf, AppError> {
    let exports_root = container.exports_path();

    // repository-barrier-allow: creates the export destination so confinement can canonicalise it.
    std::fs::create_dir_all(&exports_root)
        .map_err(|e| AppError::Other(format!("Failed to create exports directory: {}", e)))?;

    let Some(path) = requested.filter(|p| !p.trim().is_empty()) else {
        return Ok(exports_root);
    };

    let validated = ValidatedFilePath::new(PathBuf::from(&path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output directory: {}", e)))?;

    confine_to_root(&exports_root, validated.as_path()).map_err(|e| {
        AppError::InvalidInput(format!(
            "Exports can only be written to {}: {}",
            exports_root.display(),
            e
        ))
    })
}

fn export_use_case(container: &Container) -> ExportConversationsUseCase {
    ExportConversationsUseCase::new(Arc::new(ExportRepository::new(container.db_pool().clone())))
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
///   outputDir: '/Users/example/exports/lattice-markdown'
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
/// Command flow:
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
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_markdown_impl(
    output_dir: Option<String>,
    container: &Container,
) -> Result<ExportSummary, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let output_root = resolve_export_root(output_dir, container)?;
    let output_dir_str = output_root.to_string_lossy().to_string();

    let result = export_use_case(container)
        .execute(output_root, ExportFormat::Markdown)
        .await;

    // Audit the outcome
    match &result {
        Ok(summary) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&summary.output_dir)
                .with_metadata("operation", "export_markdown")
                .with_metadata("format", "markdown")
                .with_metadata("files_exported", summary.count().to_string());

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
    output_dir: Option<String>,
    container: State<'_, Container>,
) -> Result<usize, AppError> {
    export_markdown_impl(output_dir, container.inner())
        .await
        .map(|summary| summary.count())
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
///   outputPath: '/Users/example/exports/lattice-data.json',
///   pretty: true
/// });
///
/// console.log('Export complete!');
///
/// // Export to compact JSON for smaller file size
/// await invoke('export_json', {
///   outputPath: '/Users/example/exports/lattice-data-compact.json',
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
/// Command flow:
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
///
/// ## Implementation Layer (Pure Rust - No Tauri)
pub async fn export_json_impl(
    output_path: Option<String>,
    pretty: bool,
    container: &Container,
) -> Result<ExportSummary, AppError> {
    let audit_logger = get_audit_logger();

    // Rate limiting
    container
        .security_context()
        .rate_limiters()
        .backup
        .check_rate_limit("backup")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    let output_root = resolve_export_root(output_path, container)?;
    let output_path_str = output_root.to_string_lossy().to_string();

    let result = export_use_case(container)
        .execute(output_root, ExportFormat::Json { pretty })
        .await;

    // Audit the outcome
    match &result {
        Ok(summary) => {
            let event = AuditEvent::new(AuditAction::DataExported, AuditResult::success())
                .with_resource_id(&summary.output_dir)
                .with_metadata("operation", "export_json")
                .with_metadata("format", "json")
                .with_metadata("pretty", pretty.to_string())
                .with_metadata("files_exported", summary.count().to_string());

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
    output_path: Option<String>,
    pretty: bool,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    export_json_impl(output_path, pretty, container.inner())
        .await
        .map(|_| ())
}

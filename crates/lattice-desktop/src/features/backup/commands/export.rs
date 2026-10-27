//! Markdown and JSON exports, plus the shared export-destination plumbing
//! both of them run through.

use crate::features::backup::export_repository::ExportRepository;
use crate::features::backup::use_cases::{ExportConversationsUseCase, ExportFormat, ExportSummary};
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::fs::confinement::confine_to_root;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;
use std::sync::Arc;

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
    let use_case = ExportConversationsUseCase::new(Arc::new(ExportRepository::new(
        container.db_pool().clone(),
    )));
    match lattice_core::shared::fs::roots::chat_images_root() {
        Ok(root) => use_case.with_chat_images_root(root),
        Err(_) => use_case,
    }
}

/// Exports all indexed documents to Markdown format
///
/// Converts indexed documents to individual Markdown files, preserving content
/// structure and metadata. Creates one `.md` file per document in the specified
/// output directory, suitable for use in Markdown editors, static site generators,
/// or knowledge management tools like Obsidian.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid output directory path (directory traversal detected)
/// * `AppError::Other` - Path exists but is not a directory, or filesystem errors
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
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
///
/// # Directory Creation
///
/// - If output directory doesn't exist, it's created automatically
/// - If path exists but is not a directory, returns error
/// - Creates parent directories recursively if needed
///
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Async directory creation via tokio::fs
pub async fn export_markdown_impl(
    output_dir: Option<String>,
    container: &Container,
) -> Result<ExportSummary, AppError> {
    let output_root = resolve_export_root(output_dir, container)?;

    let result = export_use_case(container)
        .execute(output_root, ExportFormat::Markdown)
        .await;

    result
}

/// Exports all indexed documents to JSON format
///
/// Serializes all indexed documents into a single JSON file with complete metadata,
/// content, tags, and timestamps. Supports both compact and pretty-printed formats.
/// Ideal for programmatic access, data analysis, API integration, or structured backups.
///
/// # Errors
///
/// * `AppError::InvalidInput` - Invalid output path (directory traversal detected)
/// * `AppError::Other` - Failed to create parent directory or filesystem errors
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
/// - **Path Validation (CWE-22)**: ValidatedFilePath prevents directory traversal attacks
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
/// # Architecture
///
/// Command flow:
/// 1. Path validation via ValidatedFilePath (CWE-22)
/// 2. Async parent directory creation via tokio::fs
pub async fn export_json_impl(
    output_path: Option<String>,
    pretty: bool,
    container: &Container,
) -> Result<ExportSummary, AppError> {
    let output_root = resolve_export_root(output_path, container)?;

    let result = export_use_case(container)
        .execute(output_root, ExportFormat::Json { pretty })
        .await;

    result
}

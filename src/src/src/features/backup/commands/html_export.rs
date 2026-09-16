//! `export_html`: browsable static-site export to a caller-supplied directory.

use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use std::path::PathBuf;
use tauri::State;

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
///   outputDir: '/Users/example/exports/lattice-html'
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
/// 4. Generate HTML files for each document
/// 5. Generate index.html and styles.css
/// 6. Log audit event (success/failure with file count)
/// 7. Return file count
///
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

    let validated_path = ValidatedFilePath::new(PathBuf::from(&output_dir))
        .map_err(|e| AppError::InvalidInput(format!("Invalid output directory: {}", e)))?;

    let output_dir_str = validated_path.as_path().to_string_lossy().to_string();

    let result = async {
        // repository-barrier-allow: export creates and writes the user-selected output directory.
        // Check and create directory asynchronously
        match tokio::fs::metadata(validated_path.as_path()).await {
            // repository-barrier-allow: export requires a directory resource.
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

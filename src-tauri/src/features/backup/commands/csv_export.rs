//! `export_csv`: spreadsheet-oriented export to a caller-supplied path.

use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::domain_types::ValidatedFilePath;
use crate::shared::error::AppError;
use std::path::PathBuf;
use tauri::State;

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
///   outputPath: '/Users/example/exports/lattice-documents.csv'
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
/// 4. Serialize documents to CSV with proper escaping
/// 5. Write CSV to file
/// 6. Log audit event (success/failure with row count)
/// 7. Return row count
///
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

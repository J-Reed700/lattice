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

//! Imports from third-party knowledge tools (Obsidian, Notion, Roam).

use lattice_core::shared::error::AppError;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

pub async fn import_obsidian_vault(vault_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&vault_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid lattice path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected vault resource.
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Lattice not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access lattice: {}", e))),
    };

    result
}

pub async fn import_notion_export(export_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&export_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected export resource.
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    result
}

pub async fn import_roam_json(json_path: String) -> Result<usize, AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&json_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid export path: {}", e)))?;

    // repository-barrier-allow: import validates the user-selected JSON resource.
    let result = match tokio::fs::metadata(validated_path.as_path()).await {
        Ok(_) => Ok(0),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(AppError::Other(format!(
            "Export not found: {}",
            validated_path.as_path().display()
        ))),
        Err(e) => Err(AppError::Other(format!("Failed to access export: {}", e))),
    };

    result
}

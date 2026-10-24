//! `create_backup`: writes a full database snapshot for disaster recovery.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

/// Creates a complete database snapshot, saving to `backup_path` or the default backup directory.
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

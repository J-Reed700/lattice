//! `restore_backup`: replaces the live database with a stored snapshot.

use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use lattice_core::shared::fs::confinement::confine_to_root;
use lattice_core::shared::types::ValidatedFilePath;
use std::path::PathBuf;

/// Restores the Lattice database from a previously created `.lattice-backup` snapshot file.
pub async fn restore_backup_impl(
    backup_path: String,
    container: &Container,
) -> Result<(), AppError> {
    let validated_path = ValidatedFilePath::new(PathBuf::from(&backup_path))
        .map_err(|e| AppError::InvalidInput(format!("Invalid backup path: {}", e)))?;

    // `ValidatedFilePath` only filters `..`, so it accepts any absolute path.
    // Restoring replaces the application database wholesale, which means an
    // unconfined source is a state-injection primitive: point it at a
    // planted SQLite file in ~/Downloads and every document, setting and
    // model row comes from the attacker. Creating a backup is already
    // confined to this directory; restoring must be symmetric.
    let backups_root = container.backups_path();
    let confined_path = confine_to_root(&backups_root, validated_path.as_path()).map_err(|e| {
        AppError::InvalidInput(format!(
            "Backups can only be restored from {}: {}",
            backups_root.display(),
            e
        ))
    })?;

    let use_case = container.system.restore_backup_use_case();
    let result = use_case.execute(confined_path).await;

    result.map(|_| ())
}

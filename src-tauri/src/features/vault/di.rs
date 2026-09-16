//! Vault feature dependency injection.
//!
//! The write-back worker handle and the suppression registry are built in
//! `Container::new`; the accessors over them belong to this feature.

use crate::features::vault::watcher::RescanSummary;
use crate::features::vault::writeback::VaultWriterHandle;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Vault's registrar surface on `Container`.
impl Container {
    pub fn vault_writer(&self) -> &VaultWriterHandle {
        &self.vault_writer
    }

    /// Belt-and-suspenders safety net for the fs watcher.
    /// No-op when vault is disabled or the watch toggle is off.
    pub async fn rescan_vault(&self) -> Result<RescanSummary> {
        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;
        if !settings.vault.enabled || !settings.vault.watch_external_changes {
            return Ok(RescanSummary {
                scanned: 0,
                imported: 0,
                deleted: 0,
            });
        }
        let vault_root =
            crate::features::vault::writeback::resolve_vault_root(&settings.vault.vault_path)
                .ok_or_else(|| {
                    AppError::InvalidConfig(
                        "Vault root could not be resolved (no home directory)".to_string(),
                    )
                })?;
        let app_handle = self.app_handle.clone().ok_or_else(|| {
            AppError::InvalidConfig(
                "AppHandle not installed — Container constructed without with_app_handle"
                    .to_string(),
            )
        })?;
        crate::features::vault::watcher::rescan_vault(
            self.core.db_pool().clone(),
            vault_root,
            self.vault_write_suppression.clone(),
            app_handle,
        )
        .await
        .map_err(AppError::Other)
    }
}

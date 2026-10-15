//! The check a settings update runs before it turns the vault on.

use lattice_core::shared::error::{AppError, Result};

/// Refuse a vault folder that cannot be resolved, created or written to.
/// `configured` is the `vault_path` setting, empty for the default folder.
pub async fn check_vault_root(configured: &str) -> Result<()> {
    let root = super::writeback::resolve_vault_root(configured).ok_or_else(|| {
        AppError::InvalidInput(
            "Vault root could not be resolved (no home directory available). \
             Pick an explicit folder in Vault settings."
                .to_string(),
        )
    })?;
    probe_vault_writable(&root).await
}

async fn probe_vault_writable(root: &std::path::Path) -> Result<()> {
    if let Err(e) = tokio::fs::create_dir_all(root).await {
        return Err(AppError::InvalidInput(format!(
            "Vault folder '{}' could not be created: {}. Pick a different folder.",
            root.display(),
            e
        )));
    }
    let sentinel = root.join(".lattice-vault-probe");
    if let Err(e) = tokio::fs::write(&sentinel, b"probe").await {
        return Err(AppError::InvalidInput(format!(
            "Vault folder '{}' is not writable: {}. Check permissions, or pick a different folder.",
            root.display(),
            e
        )));
    }
    if let Err(e) = tokio::fs::remove_file(&sentinel).await {
        tracing::warn!(
            sentinel = %sentinel.display(),
            error = %e,
            "vault writability probe: failed to clean up sentinel file"
        );
    }
    Ok(())
}

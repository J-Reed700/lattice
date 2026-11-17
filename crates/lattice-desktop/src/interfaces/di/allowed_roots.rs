//! The directories file-read IPC is allowed to reach, recomputed from settings.

use crate::infrastructure::security::FileAccessConfig;
use crate::infrastructure::storage::ContentAddressedStorage;
use lattice_core::application::ports::SettingsRepositoryPort;
use lattice_core::shared::persistence::Database;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Recomputes `FileAccessConfig`'s allowed roots. One instance is shared by
/// the Container (startup) and the settings side effects (every write).
pub(crate) struct AllowedRoots {
    settings_repo: Arc<dyn SettingsRepositoryPort>,
    database: Database,
    data_dir: PathBuf,
    file_access_config: Arc<FileAccessConfig>,
    /// Arrives in `Container::with_app_handle`, after construction.
    app_handle: OnceLock<tauri::AppHandle>,
}

impl AllowedRoots {
    pub(crate) fn new(
        settings_repo: Arc<dyn SettingsRepositoryPort>,
        database: Database,
        data_dir: PathBuf,
        file_access_config: Arc<FileAccessConfig>,
    ) -> Self {
        Self {
            settings_repo,
            database,
            data_dir,
            file_access_config,
            app_handle: OnceLock::new(),
        }
    }

    pub(crate) fn set_app_handle(&self, handle: tauri::AppHandle) {
        let _ = self.app_handle.set(handle);
    }

    /// Recompute which directories file-read IPC is allowed to reach.
    ///
    /// The policy is: the app's own data and imported-file library directories,
    /// plus the user's vault and every folder they have asked us to index.
    /// Anything else in `$HOME` — SSH keys, cloud credentials, browser profiles — stays
    /// unreadable over IPC.
    ///
    /// Must run at startup once settings are readable, and again after any
    /// change to the vault path or the indexed-folder list; otherwise a
    /// newly added folder is unreadable, or a removed one stays readable.
    pub(crate) async fn refresh(&self) -> lattice_core::shared::error::Result<()> {
        let settings = self.settings_repo.get_all().await?;

        let mut roots = vec![
            self.data_dir.clone(),
            ContentAddressedStorage::default_library_root()?,
        ];

        if settings.vault.enabled && !settings.vault.vault_path.is_empty() {
            roots.push(PathBuf::from(&settings.vault.vault_path));
        }

        for indexed in &settings.indexing.indexed_paths {
            if !indexed.is_empty() {
                roots.push(PathBuf::from(indexed));
            }
        }

        // Folders being watched are also legitimately readable, and the watch
        // list is authoritative in the database rather than in settings.
        match crate::infrastructure::persistence::repositories::file_library::watch_folder_paths(
            &self.database,
        )
        .await
        {
            Ok(paths) => roots.extend(paths.into_iter().map(PathBuf::from)),
            Err(e) => tracing::warn!(
                error = %e,
                "could not read watch folders while recomputing allowed roots"
            ),
        }

        // The viewers load images and audio over `asset://`, whose static
        // scope is only the app data and the library; the user's own folders
        // are opened to it here, as to IPC. Tauri cannot take a directory back
        // out of the scope, so a removed folder stays viewable until restart.
        if let Some(handle) = self.app_handle.get() {
            use tauri::Manager;
            let scope = handle.asset_protocol_scope();
            for root in &roots {
                scope.allow_directory(root, true).map_err(|e| {
                    lattice_core::shared::error::AppError::InternalError(format!(
                        "could not open {} to the asset protocol: {e}",
                        root.display()
                    ))
                })?;
            }
        }

        let count = roots.len();
        self.file_access_config.set_allowed_roots(roots)?;
        tracing::info!(count, "recomputed file-access allowed roots");
        Ok(())
    }
}

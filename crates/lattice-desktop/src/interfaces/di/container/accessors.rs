//! The cross-cutting core accessors: DB pool/connection, security context,
//! file-access config, and the data-dir-derived paths.

use super::Container;
use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use crate::infrastructure::storage::ContentAddressedStorage;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;

impl Container {
    pub fn db_pool(&self) -> &SqlitePool {
        self.core.db_pool()
    }

    pub fn jobs(&self) -> &Arc<lattice_core::shared::runtime::jobs::JobRuntime> {
        &self.jobs
    }

    /// The one registry of in-flight learning outline runs: the commands
    /// that start or stop a run all consult this same map.
    pub fn outline_run_registry(
        &self,
    ) -> Arc<crate::features::learning::outline_progress::OutlineRunRegistry> {
        Arc::clone(&self.outline_run_registry)
    }

    /// The one registry of learning outline drafts in use: generation,
    /// repair and acceptance of a draft all consult this same set.
    pub fn draft_run_registry(
        &self,
    ) -> Arc<crate::features::learning::outline_draft::DraftRunRegistry> {
        Arc::clone(&self.draft_run_registry)
    }

    /// The one registry of executing learning practical runs: the commands
    /// that start or cancel a run all consult this same map.
    pub fn practical_run_registry(
        &self,
    ) -> Arc<crate::features::learning::practical_runs::PracticalRunRegistry> {
        Arc::clone(&self.practical_run_registry)
    }

    pub fn folder_index(
        &self,
    ) -> Option<&Arc<crate::features::explorer::index::manager::FolderIndexManager>> {
        self.folder_index.as_ref()
    }

    /// Every course's evidence index; `None` in test fixtures.
    pub fn course_indexes(
        &self,
    ) -> Option<&Arc<crate::features::learning::references::course_index::CourseIndexes>> {
        self.course_indexes.as_ref()
    }

    pub fn db_conn(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::database::DatabaseConnection> {
        self.core.db_conn()
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        self.core.security_context()
    }

    /// Get file access configuration (for secure path validation)
    ///
    /// Returns Arc-wrapped FileAccessConfig for use by file operation use cases.
    /// This enforces path validation against allowed_roots (lattice + indexed directories).
    pub fn file_access_config(&self) -> Arc<FileAccessConfig> {
        Arc::clone(self.core.file_access_config())
    }

    /// Get the models directory path
    ///
    /// Returns the path where AI models are stored (app_data_dir/models)
    /// This path is consistent with get_model_dir() in lib.rs
    pub fn models_path(&self) -> PathBuf {
        self.core.data_dir().join("models")
    }

    /// The manager that fetches and inspects the models kept under
    /// [`Self::models_path`].
    pub fn model_manager(
        &self,
    ) -> Arc<dyn crate::infrastructure::services::traits::ModelManagerTrait> {
        Arc::clone(self.system.model_manager())
    }

    /// Reads what a local model file records about itself in its header.
    pub fn model_file_inspector(
        &self,
    ) -> Arc<dyn lattice_core::application::ports::ModelFileInspector> {
        Arc::new(crate::features::llm::engine::gguf_metadata::GgufFileInspector)
    }

    /// Get the backups directory path.
    ///
    /// Must stay in agreement with `BackupAdapter`, which derives the same
    /// location from the database path and refuses to *write* a backup
    /// anywhere else. Restore confines its *source* to this directory for the
    /// mirror-image reason: the database file it swaps in becomes the app's
    /// entire state, so accepting an arbitrary path lets a caller inject
    /// documents, settings and model rows wholesale.
    pub fn backups_path(&self) -> PathBuf {
        self.core.data_dir().join("backups")
    }

    /// Get the exports directory path.
    ///
    /// Destination for user-initiated exports. Confining writes here keeps
    /// export commands from doubling as an arbitrary-write primitive for the
    /// webview. If exporting to a user-chosen location is wanted, route it
    /// through a native save dialog so the *user*, not the renderer, picks
    /// the path.
    pub fn exports_path(&self) -> PathBuf {
        self.core.data_dir().join("exports")
    }

    /// Recompute which directories file-read IPC is allowed to reach.
    ///
    /// The policy is: the app's own data and imported-file library directories,
    /// plus the user's vault and every folder they have asked us to index.
    /// Anything else in `$HOME` — SSH keys, cloud credentials, browser profiles — stays
    /// unreadable over IPC.
    ///
    /// Must be called at startup once settings are readable, and again after
    /// any change to the vault path or the indexed-folder list; otherwise a
    /// newly added folder is unreadable, or a removed one stays readable.
    pub async fn refresh_allowed_roots(&self) -> lattice_core::shared::error::Result<()> {
        let settings = self.system.settings_repo().get_all().await?;

        let mut roots = vec![
            self.core.data_dir().to_path_buf(),
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
        match sqlx::query_scalar::<_, String>("SELECT path FROM watch_folders")
            .fetch_all(self.db_pool())
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
        if let Some(handle) = &self.app_handle {
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
        self.file_access_config().set_allowed_roots(roots)?;
        tracing::info!(count, "recomputed file-access allowed roots");
        Ok(())
    }
}

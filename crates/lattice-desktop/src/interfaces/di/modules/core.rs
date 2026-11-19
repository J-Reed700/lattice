//! The core module: the database pool, security context, data directory,
//! background scope and credentials.

use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::background::BackgroundTasks;
use std::path::PathBuf;
use std::sync::Arc;

// Application Use Cases - Credentials
use crate::features::credentials::use_cases::SetApiKeyUseCase;

// Application Ports
use lattice_core::application::ports::CredentialsPort;

/// Core shared infrastructure used by all modules
///
/// **Fields**: ~10
/// - Database connection pool
/// - Security context and file access config
/// - Application data directory and the app's own readable folders
/// - Background scope for spawned work
/// - Credentials port for API keys
#[derive(Clone)]
pub struct CoreModule {
    // Cross-cutting shared infrastructure — not owned by any feature slice.
    database: Database,
    security_context: Arc<SecurityContext>,
    file_access_config: Arc<FileAccessConfig>,
    data_dir: PathBuf,
    app_owned_roots: Vec<PathBuf>,
    background: Arc<BackgroundTasks>,

    // Feature slice
    credentials: crate::features::credentials::di::CredentialsDi,
}

impl CoreModule {
    /// Build shared Layer-1 infrastructure plus the credentials feature slice.
    pub async fn new(
        database: Database,
        data_dir: PathBuf,
        background: Arc<BackgroundTasks>,
    ) -> lattice_core::shared::error::Result<Self> {
        let security_context = Arc::new(SecurityContext::new());

        // The app's own folders exist before the policy resolves them.
        let library_root =
            crate::infrastructure::storage::ContentAddressedStorage::default_library_root()?;
        let web_archive_root = lattice_core::shared::fs::roots::web_archive_root()?;
        for (name, root) in [
            ("library", &library_root),
            ("web archive", &web_archive_root),
        ] {
            tokio::fs::create_dir_all(root).await.map_err(|error| {
                lattice_core::shared::error::AppError::FileStorage(format!(
                    "Failed to create {name} directory: {error}"
                ))
            })?;
        }
        let app_owned_roots = vec![data_dir.clone(), library_root, web_archive_root];
        // The user's vault and indexed folders are added by refresh_allowed_roots.
        let file_access_config = Arc::new(FileAccessConfig::new(app_owned_roots.clone()));

        let credentials_path = data_dir.join("credentials.json");
        let credentials = crate::features::credentials::di::build(credentials_path);

        Ok(Self {
            database,
            security_context,
            file_access_config,
            data_dir,
            app_owned_roots,
            background,
            credentials,
        })
    }

    // Infrastructure getters
    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        &self.security_context
    }

    pub fn file_access_config(&self) -> &Arc<FileAccessConfig> {
        &self.file_access_config
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// The folders the app owns: its data, the library and the web archive.
    pub fn app_owned_roots(&self) -> &[PathBuf] {
        &self.app_owned_roots
    }

    /// The app's background scope: shutdown closes it and joins its tasks
    /// before the database closes.
    pub fn background(&self) -> &Arc<BackgroundTasks> {
        &self.background
    }

    // Credentials feature getters
    pub fn credentials(&self) -> &Arc<dyn CredentialsPort> {
        &self.credentials.credentials
    }

    pub fn set_api_key_use_case(&self) -> &Arc<SetApiKeyUseCase> {
        &self.credentials.set_api_key_use_case
    }
}
